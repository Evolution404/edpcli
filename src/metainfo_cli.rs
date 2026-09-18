//! `nopwd metainfo` / `nopwd meta` 的来源选择与输出层。
//!
//! 目标是“少打命令、直接看关键信息”：当前盘直接查看；onlyid 默认最新 [1]；
//! 备份文件可直接作位置参数。底层解析继续复用 inspect/metainfo，不维护第二套算法。

use crate::backup_catalog::{self, BackupCatalog};
use crate::backup_cli::{print_backup_sources, print_onlyid_backup_choices};
use crate::cli::{auto_pick_disk, guard_usb_disk, MetaInfoOpts, StdPrompter};
use crate::common::{EXIT_BACKUP, EXIT_IO, EXIT_OK, EXIT_TARGET, SECTOR};
use crate::diskio::{self, raw_path};
use crate::elevate;
use crate::identify::identify;
use crate::inspect::InspectMeta;
use crate::inspect_cli::resolve_inspect_file;
use crate::metainfo;
use crate::sysinfo::{self, SysRunner};

fn print_summary(source: &str, summary: &metainfo::MetaInfoSummary) {
    println!(
        "{}  {}",
        crate::ui::bold_cyan("来源"),
        crate::ui::cyan(source)
    );
    println!();
    print!("{}", metainfo::render(summary));
}

fn backup_flow(opts: MetaInfoOpts) -> i32 {
    let bak = diskio::resolve_backup_dir(opts.backup_dir.as_deref());
    let (path, parsed_meta, source_label) = if let Some(id) = opts.onlyid.as_deref() {
        let catalog = BackupCatalog::load(&bak);
        let group = match catalog.onlyid_group(id) {
            Ok(group) => group,
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {msg}")));
                let _ = print_backup_sources(catalog.entries(), "查看元信息: nopwd meta <onlyid>");
                return EXIT_BACKUP;
            }
        };
        let idx = opts.index.unwrap_or(1);
        let entry = match catalog.onlyid_index(id, idx) {
            Ok(entry) => entry,
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {msg}")));
                println!();
                print_onlyid_backup_choices(id, &group);
                return EXIT_BACKUP;
            }
        };
        (
            entry.path.clone(),
            entry.meta.clone(),
            format!(
                "backup onlyid={id} [{idx}] · {}",
                backup_catalog::file_name(entry)
            ),
        )
    } else {
        let Some(target) = opts.backup.as_deref() else {
            eprintln!("{}", crate::ui::red("错误: metainfo 缺少备份来源"));
            return EXIT_BACKUP;
        };
        let path = match resolve_inspect_file(&bak, target) {
            Ok(path) => path,
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {msg}")));
                return EXIT_BACKUP;
            }
        };
        let meta = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(diskio::parse_backup_name);
        let label = path.display().to_string();
        (path, meta, label)
    };

    let mut inspect_meta = parsed_meta
        .as_ref()
        .map(InspectMeta::from_backup_meta)
        .unwrap_or_default();
    if let Some(device_id) = opts.device_id {
        inspect_meta.device_id = Some(device_id);
    }
    let summary = match metainfo::summarize_backup(&path, &inspect_meta) {
        Ok(summary) => summary,
        Err(e) => {
            eprintln!(
                "{}",
                crate::ui::red(&format!("错误: 读取备份元信息失败 {}: {}", path.display(), e))
            );
            return EXIT_IO;
        }
    };
    print_summary(&source_label, &summary);
    EXIT_OK
}

fn disk_flow(runner: &SysRunner, mut opts: MetaInfoOpts) -> i32 {
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
            argv.push("--disk".into());
            argv.push(n.to_string());
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
    let auto_device_id = raw7
        .as_deref()
        .and_then(|raw| identify(runner, n, raw).device_id);
    let device_id = opts.device_id.clone().or(auto_device_id);
    let (vid, pid) = sysinfo::usb_vid_pid(runner, n);
    let size_bytes = sysinfo::disk_total_sectors(runner, n).and_then(|s| s.checked_mul(SECTOR as u64));
    let onlyid = diskio::read_lba(&path, 4)
        .ok()
        .and_then(|raw| diskio::lba4_label_id_from(&raw));
    let inspect_meta = InspectMeta {
        device_id,
        vid: (vid != "xxxx").then_some(vid),
        pid: (pid != "xxxx").then_some(pid),
        size_bytes,
        onlyid,
    };
    let summary = match metainfo::summarize(&inspect_meta, |lba| diskio::read_lba(&path, lba)) {
        Ok(summary) => summary,
        Err(e) => {
            eprintln!("{}", crate::ui::red(&format!("错误: 读取 disk{n} 元信息失败: {e}")));
            return EXIT_IO;
        }
    };
    print_summary(&format!("物理盘 disk{n} ({path})"), &summary);
    EXIT_OK
}

pub(crate) fn metainfo_flow(runner: &SysRunner, opts: MetaInfoOpts) -> i32 {
    if opts.backup.is_some() || opts.onlyid.is_some() {
        return backup_flow(opts);
    }
    if opts.disk.is_none() && sysinfo::list_usb_disks(runner).is_empty() {
        let bak = diskio::resolve_backup_dir(opts.backup_dir.as_deref());
        let entries = diskio::scan_backup_dir(&bak);
        if print_backup_sources(&entries, "查看最新元信息: nopwd meta <onlyid>") {
            println!();
            println!(
                "{}",
                crate::ui::yellow("未检测到外接 USB 盘；上面是可直接用 meta 查看元信息的备份盘。")
            );
            return EXIT_OK;
        }
        eprintln!(
            "{}",
            crate::ui::red("错误: 未检测到外接 USB 盘，备份目录中也没有可查看的备份。")
        );
        return EXIT_TARGET;
    }
    disk_flow(runner, opts)
}
