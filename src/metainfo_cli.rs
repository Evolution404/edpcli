//! `edpcli info` 的来源选择与输出层。
//!
//! 备份文件可直接作位置参数；当前盘由设备选择器确定。底层解析继续复用
//! inspect/metainfo，不维护第二套协议算法。

use crate::cli::{
    argv_with_backup_dir_for_elevation, auto_pick_disk, guard_usb_disk, InfoOpts, StdPrompter,
};
use crate::common::{EXIT_BACKUP, EXIT_IO, EXIT_OK, SECTOR};
use crate::diskio::{self, find_backups, raw_path, DiskFacts, FileDev, SectorReadCache};
use crate::elevate;
use crate::identify::identify;
use crate::inspect::InspectMeta;
use crate::inspect_cli::resolve_inspect_file;
use crate::metainfo;
use crate::selectors::DeviceSelector;
use crate::sysinfo::{self, CmdRunner};

enum BackupSummary<'a> {
    File(&'a std::path::Path),
    Device(&'a [std::path::PathBuf]),
}

fn print_summary(source: &str, summary: &metainfo::MetaInfoSummary, backups: BackupSummary<'_>) {
    print!("{}", metainfo::render_with_source(summary, Some(source)));
    println!();
    println!("{}", crate::ui::bold_cyan("备份"));
    match backups {
        BackupSummary::File(path) => {
            println!(
                "  {}  {}",
                crate::ui::dim(&crate::ui::pad_to("当前文件", 18)),
                path.display()
            );
        }
        BackupSummary::Device(paths) => {
            println!(
                "  {}  {}",
                crate::ui::dim(&crate::ui::pad_to("匹配备份", 18)),
                paths.len()
            );
            if let Some(latest) = paths.first() {
                println!(
                    "  {}  {}",
                    crate::ui::dim(&crate::ui::pad_to("最新备份", 18)),
                    diskio::backup_display_time(latest, diskio::mtime_epoch(latest))
                );
            }
        }
    }
}

fn backup_flow(opts: InfoOpts) -> i32 {
    let bak = diskio::resolve_backup_dir(opts.backup_dir.as_deref());
    let Some(target) = opts.backup.as_deref() else {
        eprintln!("{}", crate::ui::red("错误: info 缺少备份来源"));
        return EXIT_BACKUP;
    };
    let path = match resolve_inspect_file(&bak, target) {
        Ok(path) => path,
        Err(msg) => {
            eprintln!("{}", crate::ui::red(&format!("错误: {msg}")));
            return EXIT_BACKUP;
        }
    };
    let parsed_meta = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(diskio::parse_backup_name);
    let source_label = path.display().to_string();

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
                crate::ui::red(&format!(
                    "错误: 读取备份元信息失败 {}: {}",
                    path.display(),
                    e
                ))
            );
            return EXIT_IO;
        }
    };
    print_summary(&source_label, &summary, BackupSummary::File(&path));
    EXIT_OK
}

fn disk_flow(runner: &dyn CmdRunner, mut opts: InfoOpts) -> i32 {
    if let Some(n) = opts.disk {
        if let Err(e) = guard_usb_disk(runner, n) {
            eprintln!("{}", crate::ui::red(&e.msg));
            return e.code;
        }
    }
    if !elevate::is_root() {
        // 与 list/apply/backup create 一致：自动提权不能依赖 sudo/UAC 继承环境变量。
        // 当 backup_dir 来自 EDPCLI_BACKUP_DIR 时，将其转为显式 --backup-dir 跨过提权边界，
        // 否则 info 提权前后可能显示不同的备份数量。
        let mut argv = argv_with_backup_dir_for_elevation(opts.backup_dir.as_deref());
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
    let mut dev = match FileDev::open_rdonly(&path) {
        Ok(dev) => dev,
        Err(e) => {
            eprintln!(
                "{}",
                crate::ui::red(&format!("错误: 无法只读打开 disk{n}: {e}"))
            );
            return EXIT_IO;
        }
    };
    let mut reader = SectorReadCache::new(&mut dev);
    let raw7 = reader.read_sector(7).ok();
    let auto_device_id = raw7
        .as_deref()
        .and_then(|raw| identify(runner, n, raw).device_id);
    let device_id = opts.device_id.clone().or(auto_device_id);
    let (vid, pid) = sysinfo::usb_vid_pid(runner, n);
    let total_sectors = sysinfo::disk_total_sectors(runner, n);
    let size_bytes = total_sectors.and_then(|s| s.checked_mul(SECTOR as u64));
    let raw4 = reader.read_sector(4).ok();
    let onlyid = raw4.as_deref().and_then(diskio::lba4_label_id_from);
    let inspect_meta = InspectMeta {
        device_id: device_id.clone(),
        vid: (vid != "xxxx").then_some(vid.clone()),
        pid: (pid != "xxxx").then_some(pid.clone()),
        size_bytes,
        onlyid: onlyid.clone(),
    };
    let summary = match metainfo::summarize(&inspect_meta, |lba| reader.read_sector(lba)) {
        Ok(summary) => summary,
        Err(e) => {
            eprintln!(
                "{}",
                crate::ui::red(&format!("错误: 读取 disk{n} 元信息失败: {e}"))
            );
            return EXIT_IO;
        }
    };
    let facts = DiskFacts {
        disk: n,
        total_sectors,
        vid,
        pid,
        label_id: onlyid,
    };
    let tag16 = raw4.as_deref().and_then(diskio::lba4_tag16_from);
    let backup_dir = diskio::resolve_backup_dir(opts.backup_dir.as_deref());
    let backups = find_backups(&backup_dir, &facts, device_id.as_deref(), tag16);
    print_summary(
        &format!("物理盘 disk{n} ({path})"),
        &summary,
        BackupSummary::Device(&backups),
    );
    EXIT_OK
}

pub(crate) fn info_flow(runner: &dyn CmdRunner, opts: InfoOpts) -> i32 {
    if opts.backup.is_some() {
        return backup_flow(opts);
    }
    disk_flow(runner, opts)
}
