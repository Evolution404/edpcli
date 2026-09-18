//! `edpcli inspect` 的来源解析、展示与导出层。
//!
//! 扇区结构解析仍由 `inspect.rs` 负责；这里只处理物理盘/备份来源、onlyid 编号选择、
//! 展示模式和导出，避免 `cli.rs` 直接承担 inspect 业务流程。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::backup_cli::print_backup_sources;
use crate::cli::{auto_pick_disk, guard_usb_disk, InspectOpts, StdPrompter};
use crate::common::{EXIT_BACKUP, EXIT_IO, EXIT_OK, EXIT_TARGET, EXIT_USAGE, SECTOR};
use crate::diskio::{self, raw_path};
use crate::elevate;
use crate::identify::identify;
use crate::inspect::{self, InspectMeta};
use crate::selectors::DeviceSelector;
use crate::sysinfo::{self, CmdRunner};

pub(crate) fn resolve_inspect_file(backup_dir: &Path, target: &str) -> Result<PathBuf, String> {
    let raw = Path::new(target);
    let candidate = if raw.components().count() == 1 {
        backup_dir.join(raw)
    } else if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(raw)
    };
    let path = fs::canonicalize(&candidate)
        .map_err(|e| format!("文件不存在或不可访问 {}: {}", candidate.display(), e))?;
    if !path.is_file() {
        return Err(format!("不是普通文件: {}", path.display()));
    }
    Ok(path)
}

fn plain_hex(data: &[u8]) -> String {
    let mut out = String::new();
    for (line_no, line) in data.chunks(16).enumerate() {
        let base = line_no * 16;
        out.push_str(&format!("+0x{base:03X}: "));
        for i in 0..16 {
            if i == 8 {
                out.push(' ');
            }
            if let Some(&b) = line.get(i) {
                out.push_str(&format!("{b:02X} "));
            } else {
                out.push_str("   ");
            }
        }
        out.push(' ');
        for &b in line {
            out.push(if (0x20..=0x7e).contains(&b) {
                b as char
            } else {
                '.'
            });
        }
        out.push('\n');
    }
    out
}

fn export_inspect_view(dir: &Path, view: &inspect::SectorView) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let base = format!("LBA{:02}", view.lba);
    fs::write(dir.join(format!("{base}_raw.bin")), &view.raw)?;
    fs::write(dir.join(format!("{base}_decoded.bin")), &view.decoded)?;
    fs::write(dir.join(format!("{base}_raw.hex")), plain_hex(&view.raw))?;
    fs::write(
        dir.join(format!("{base}_decoded.hex")),
        plain_hex(&view.decoded),
    )?;
    Ok(())
}

fn print_inspect_meta(meta: &InspectMeta) {
    let mut parts = Vec::new();
    if let Some(id) = &meta.onlyid {
        parts.push(format!("onlyid={id}"));
    }
    if let (Some(v), Some(p)) = (&meta.vid, &meta.pid) {
        parts.push(format!("USB {v}:{p}"));
    }
    if let Some(size) = meta.size_bytes {
        parts.push(crate::common::fmt_gb(size));
    }
    if !parts.is_empty() {
        println!("{}  {}", crate::ui::bold("设备"), parts.join(" · "));
    }
    if let Some(did) = &meta.device_id {
        println!("{}  {}", crate::ui::bold("device_id"), did);
    } else {
        println!(
            "{}",
            crate::ui::yellow("device_id 未识别：LBA7/8/9/12 只能显示 RAW；可用 --id 手动指定")
        );
    }
}

fn render_inspect_source<F>(
    source_label: &str,
    meta: &InspectMeta,
    opts: &InspectOpts,
    mut read: F,
) -> i32
where
    F: FnMut(u32) -> io::Result<Vec<u8>>,
{
    println!("{}  {}", crate::ui::bold("来源"), source_label);
    print_inspect_meta(meta);

    let export_dir = opts.export.as_deref().map(PathBuf::from);
    if opts.lbas.is_empty() && !opts.raw && !opts.hex {
        println!();
        println!("{}", crate::ui::bold("LBA 0-13 概览:"));
        for lba in 0..14u32 {
            match read(lba) {
                Ok(raw) => {
                    let view = inspect::analyze_sector(lba, &raw, meta);
                    println!("  {}", inspect::overview_line(&view));
                    if let Some(dir) = &export_dir {
                        if let Err(e) = export_inspect_view(dir, &view) {
                            eprintln!(
                                "{}",
                                crate::ui::red(&format!("错误: 导出 LBA{lba} 失败: {e}"))
                            );
                            return EXIT_IO;
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "{}",
                        crate::ui::red(&format!("错误: 读取 LBA{lba} 失败: {e}"))
                    );
                    return EXIT_IO;
                }
            }
        }
        println!();
        println!(
            "{}",
            crate::ui::dim(
                "指定 LBA 可展开结构化字段，例如: edpcli inspect --disk N --lba 6,7,12 --hex"
            )
        );
        if let Some(dir) = &export_dir {
            println!("{}  {}", crate::ui::green("已导出"), dir.display());
        }
        return EXIT_OK;
    }

    // 用户显式要求 --hex / --raw 时不能悄悄忽略旗标：未给 LBA 就展开全部 0-13。
    let detailed_lbas: Vec<u32> = if opts.lbas.is_empty() {
        (0..14u32).collect()
    } else {
        opts.lbas.clone()
    };
    for &lba in &detailed_lbas {
        let raw = match read(lba) {
            Ok(raw) => raw,
            Err(e) => {
                eprintln!(
                    "{}",
                    crate::ui::red(&format!("错误: 读取 LBA{lba} 失败: {e}"))
                );
                return EXIT_IO;
            }
        };
        let view = inspect::analyze_sector(lba, &raw, meta);
        println!();
        println!("{}  {}", crate::ui::bold(&format!("LBA{lba}")), view.method);
        let fields = inspect::render_fields(&view);
        if !fields.is_empty() {
            print!("{}", fields);
        }
        for note in &view.notes {
            println!("  {} {}", crate::ui::dim("└─"), crate::ui::dim(note));
        }
        if view.fields.is_empty() && !opts.raw && !opts.hex {
            println!(
                "  {} {}",
                crate::ui::dim("└─"),
                crate::ui::dim("未检测到可结构化展示的已知字段；如需查看字节内容请加 --hex。")
            );
        }
        if opts.raw {
            print!("{}", inspect::render_hex(&view, true));
        } else if opts.hex {
            print!("{}", inspect::render_hex(&view, false));
        }
        if let Some(dir) = &export_dir {
            if let Err(e) = export_inspect_view(dir, &view) {
                eprintln!(
                    "{}",
                    crate::ui::red(&format!("错误: 导出 LBA{lba} 失败: {e}"))
                );
                return EXIT_IO;
            }
        }
    }
    if let Some(dir) = &export_dir {
        println!();
        println!("{}  {}", crate::ui::green("已导出"), dir.display());
    }
    EXIT_OK
}

fn inspect_backup_flow(opts: InspectOpts) -> i32 {
    let bak = diskio::resolve_backup_dir(opts.backup_dir.as_deref());
    let Some(target) = opts.backup.as_deref() else {
        eprintln!("{}", crate::ui::red("错误: inspect 缺少备份文件来源"));
        return EXIT_USAGE;
    };
    let path = match resolve_inspect_file(&bak, target) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{}", crate::ui::red(&format!("错误: {msg}")));
            return EXIT_BACKUP;
        }
    };
    let parsed_meta = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(diskio::parse_backup_name);
    let source_label = path.display().to_string();
    let mut meta = parsed_meta
        .as_ref()
        .map(InspectMeta::from_backup_meta)
        .unwrap_or_default();
    if let Some(did) = &opts.device_id {
        meta.device_id = Some(did.clone());
    }
    let path_s = path.to_string_lossy().into_owned();
    render_inspect_source(&source_label, &meta, &opts, |lba| {
        diskio::read_lba(&path_s, lba)
    })
}

fn inspect_disk_flow(runner: &dyn CmdRunner, mut opts: InspectOpts) -> i32 {
    if let Some(n) = opts.disk {
        if let Err(e) = guard_usb_disk(runner, n) {
            eprintln!("{}", crate::ui::red(&e.msg));
            return e.code;
        }
    }
    if !elevate::is_root() {
        let mut argv: Vec<String> = std::env::args().skip(1).collect();
        if opts.disk.is_none() {
            let mut prompt = StdPrompter;
            let n = match auto_pick_disk(runner, &mut prompt) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("{}", crate::ui::red(&e.msg));
                    return e.code;
                }
            };
            DeviceSelector::new(None).pin_argv(&mut argv, n);
        }
        elevate::ensure_elevated(&argv);
        unreachable!();
    }
    let n = match opts.disk {
        Some(n) => n,
        None => {
            let mut prompt = StdPrompter;
            match auto_pick_disk(runner, &mut prompt) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("{}", crate::ui::red(&e.msg));
                    return e.code;
                }
            }
        }
    };
    opts.disk = Some(n);
    if let Err(e) = guard_usb_disk(runner, n) {
        eprintln!("{}", crate::ui::red(&e.msg));
        return e.code;
    }
    let path = raw_path(n);
    let raw7 = diskio::read_lba(&path, 7).ok();
    let id = raw7
        .as_deref()
        .and_then(|r| identify(runner, n, r).device_id);
    let (vid, pid) = sysinfo::usb_vid_pid(runner, n);
    let size_bytes =
        sysinfo::disk_total_sectors(runner, n).and_then(|s| s.checked_mul(SECTOR as u64));
    let onlyid = diskio::read_lba(&path, 4)
        .ok()
        .and_then(|b| diskio::lba4_label_id_from(&b[..b.len().min(32)]));
    let mut meta = InspectMeta {
        device_id: id,
        vid: (vid != "xxxx").then_some(vid),
        pid: (pid != "xxxx").then_some(pid),
        size_bytes,
        onlyid,
    };
    if let Some(did) = &opts.device_id {
        meta.device_id = Some(did.clone());
    }
    render_inspect_source(&format!("物理盘 disk{n} ({path})"), &meta, &opts, |lba| {
        diskio::read_lba(&path, lba)
    })
}

pub(crate) fn inspect_flow(runner: &dyn CmdRunner, opts: InspectOpts) -> i32 {
    if opts.backup.is_some() {
        inspect_backup_flow(opts)
    } else {
        if opts.disk.is_none() && sysinfo::list_usb_disks(runner).is_empty() {
            let bak = diskio::resolve_backup_dir(opts.backup_dir.as_deref());
            let entries = diskio::scan_backup_dir(&bak);
            if print_backup_sources(&entries, "查看备份: edpcli inspect <备份.bin>") {
                println!(
                    "{}",
                    crate::ui::yellow("未检测到外接 USB 盘；上面是当前可离线查看的备份。")
                );
                return EXIT_OK;
            }
            eprintln!(
                "{}",
                crate::ui::red("错误: 未检测到外接 USB 盘，备份目录中也没有可查看的备份。")
            );
            return EXIT_TARGET;
        }
        inspect_disk_flow(runner, opts)
    }
}
