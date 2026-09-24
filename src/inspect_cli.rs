//! `edpcli inspect` 的来源解析、展示与导出层。
//!
//! 扇区结构解析仍由 `inspect.rs` 负责；这里只处理显式备份文件或当前物理盘来源、
//! 展示模式和导出，避免 `cli.rs` 直接承担 inspect 业务流程。

use std::fs;
use std::path::{Path, PathBuf};

use crate::application::inspect::{
    AdvancedInspectMode, AdvancedInspectRequest, AdvancedInspectWorkspace,
};
use crate::cli::StdPrompter;
use crate::cli_args::{InspectMode, InspectOpts};
use crate::common::{EXIT_BACKUP, EXIT_IO, EXIT_OK, EXIT_TARGET, EXIT_USAGE, SECTOR};
use crate::diskio;
use crate::elevate;
use crate::inspect::InspectMeta;
use crate::selectors::DeviceSelector;
use crate::sysinfo::CmdRunner;

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
    if path.extension().and_then(|ext| ext.to_str()) != Some("edpb") {
        return Err(format!("只接受 .edpb 备份: {}", path.display()));
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

fn selected_lbas(opts: &InspectOpts) -> Vec<u64> {
    if opts.lbas.is_empty() {
        (0..crate::common::METADATA_SECTOR_COUNT as u64).collect()
    } else {
        opts.lbas.clone()
    }
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

fn request_from_opts(opts: &InspectOpts) -> AdvancedInspectRequest {
    AdvancedInspectRequest {
        mode: match opts.mode {
            InspectMode::Raw => AdvancedInspectMode::Raw,
            InspectMode::Decode => AdvancedInspectMode::Decode,
            InspectMode::Meta => AdvancedInspectMode::Meta,
        },
        lbas: selected_lbas(opts),
        export_dir: opts.export.as_deref().map(PathBuf::from),
        device_id_override: opts.device_id.clone(),
    }
}

fn inspect_error_code(message: &str, backup_source: bool) -> i32 {
    if message.contains("越界") {
        return EXIT_TARGET;
    }
    if message.contains("EDPB 未采集")
        || message.contains("Artifact 截断")
        || message.contains("读取 LBA")
        || message.contains("读取协议上下文")
        || message.contains("无法只读打开")
        || message.contains("只读取到")
        || message.contains("导出")
    {
        return EXIT_IO;
    }
    if backup_source {
        EXIT_BACKUP
    } else {
        EXIT_TARGET
    }
}

fn render_workspace(workspace: &AdvancedInspectWorkspace) -> i32 {
    println!("{}  {}", crate::ui::bold("来源"), workspace.source);
    print_inspect_meta(&workspace.meta);
    println!("{}  {}", crate::ui::bold("模式"), workspace.mode.label());

    for item in &workspace.items {
        println!();
        let regions = item.regions.join("；");
        match workspace.mode {
            AdvancedInspectMode::Raw => {
                let offset = match item.lba.checked_mul(SECTOR as u64) {
                    Some(offset) => offset,
                    None => {
                        eprintln!("{}", crate::ui::red("错误: LBA 字节偏移溢出"));
                        return EXIT_TARGET;
                    }
                };
                println!(
                    "{}",
                    crate::ui::bold(&format!(
                        "LBA{}  offset=0x{offset:X}  区域={}  SHA-256={}  非零={}/512",
                        item.lba, regions, item.raw_sha256, item.raw_nonzero
                    ))
                );
                print!("{}", plain_hex(&item.raw));
            }
            AdvancedInspectMode::Decode => {
                let Some(decoded) = item.decoded.as_deref() else {
                    eprintln!(
                        "{}",
                        crate::ui::red(&format!("错误: LBA{} 缺少 decoded 结果", item.lba))
                    );
                    return EXIT_TARGET;
                };
                let method = item.method.as_deref().unwrap_or("decode");
                println!(
                    "{}  {}",
                    crate::ui::bold(&format!("LBA{}", item.lba)),
                    method
                );
                println!(
                    "  区域: {}  decoded SHA-256={}",
                    regions,
                    item.decoded_sha256.as_deref().unwrap_or("<missing>")
                );
                print!("{}", plain_hex(decoded));
            }
            AdvancedInspectMode::Meta => {
                let Some(text) = item.meta_text.as_deref() else {
                    eprintln!(
                        "{}",
                        crate::ui::red(&format!("错误: LBA{} 缺少 meta 结果", item.lba))
                    );
                    return EXIT_TARGET;
                };
                print!("{text}");
            }
        }
    }

    if let Some(dir) = &workspace.export_dir {
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
        Ok(path) => path,
        Err(message) => {
            eprintln!("{}", crate::ui::red(&format!("错误: {message}")));
            return EXIT_BACKUP;
        }
    };
    let request = request_from_opts(&opts);
    match crate::application::inspect::load_backup_advanced_inspect(&path, &request) {
        Ok(workspace) => render_workspace(&workspace),
        Err(message) => {
            eprintln!("{}", crate::ui::red(&format!("错误: {message}")));
            inspect_error_code(&message, true)
        }
    }
}

fn inspect_disk_flow(runner: &dyn CmdRunner, mut opts: InspectOpts) -> i32 {
    let selector = DeviceSelector::new(opts.disk);
    if !elevate::is_root() {
        let mut argv: Vec<String> = std::env::args().skip(1).collect();
        let mut prompt = StdPrompter;
        let n = match selector.resolve(runner, &mut prompt) {
            Ok(n) => n,
            Err(error) => {
                eprintln!("{}", crate::ui::red(&error.msg));
                return error.code;
            }
        };
        selector.pin_argv(&mut argv, n);
        elevate::ensure_elevated(&argv);
        unreachable!();
    }

    let mut prompt = StdPrompter;
    let n = match selector.resolve(runner, &mut prompt) {
        Ok(n) => n,
        Err(error) => {
            eprintln!("{}", crate::ui::red(&error.msg));
            return error.code;
        }
    };
    opts.disk = Some(n);
    let request = request_from_opts(&opts);
    match crate::application::inspect::load_disk_advanced_inspect(runner, n, &request) {
        Ok(workspace) => render_workspace(&workspace),
        Err(message) => {
            eprintln!("{}", crate::ui::red(&format!("错误: {message}")));
            inspect_error_code(&message, false)
        }
    }
}

pub(crate) fn inspect_flow(runner: &dyn CmdRunner, opts: InspectOpts) -> i32 {
    if opts.backup.is_some() {
        inspect_backup_flow(opts)
    } else {
        inspect_disk_flow(runner, opts)
    }
}
