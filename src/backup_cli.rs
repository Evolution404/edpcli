//! `edpcli backup` 命令的呈现与操作层。
//!
//! 备份目录的数据解释统一交给 `BackupCatalog`；本模块只负责 CLI 渲染、交互选择、
//! 校验/清理/删除动作。这样 `cli.rs` 不再承载备份领域细节，inspect 也只依赖两个
//! 明确的选择视图接口。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::backup_catalog;
use crate::cli::Prompter;
use crate::common::{EXIT_BACKUP, EXIT_CANCELLED, EXIT_OK, SECTOR};
use crate::diskio::{self, BackupEntry, BackupMeta, Sha256Status};
use crate::metainfo;

fn backup_model_name(meta: &BackupMeta) -> String {
    let mut vendor = None;
    let mut product = None;
    let mut revision = None;
    for part in meta.device_id.split('&') {
        if let Some(v) = part.strip_prefix("ven_") {
            vendor = Some(v.replace('_', " "));
        } else if let Some(v) = part.strip_prefix("prod_") {
            product = Some(v.replace('_', " "));
        } else if let Some(v) = part.strip_prefix("rev_") {
            revision = Some(v.replace('_', " "));
        }
    }
    let mut out = match (vendor, product) {
        (Some(v), Some(p)) => format!("{} {}", v, p),
        (Some(v), None) => v,
        _ => meta.device_id.clone(),
    };
    if let Some(r) = revision {
        out.push_str(&format!(" ({})", r));
    }
    out
}

fn backup_capacity(meta: &BackupMeta) -> String {
    meta.secs
        .and_then(|s| s.checked_mul(SECTOR as u64))
        .map(crate::common::fmt_gb)
        .unwrap_or_else(|| "容量未知".into())
}

fn backup_kind(entry: &BackupEntry) -> &'static str {
    if entry.is_nopwd || entry.meta.as_ref().map(|m| m.tagged_nopwd).unwrap_or(false) {
        "[免密状态]"
    } else {
        "[加密原盘]"
    }
}

fn backup_health(entry: &BackupEntry) -> String {
    if !entry.size_ok {
        return crate::ui::red("EDPB ✗ 核心数据异常");
    }
    match entry.sha256_ok {
        Sha256Status::Ok => crate::ui::green("EDPB ✓"),
        Sha256Status::Mismatch => crate::ui::red("EDPB ✗ 损坏"),
        Sha256Status::NoSidecar => crate::ui::red("EDPB ✗ 未校验"),
    }
}

fn ownership_summary(entry: &BackupEntry) -> Option<(Option<String>, Option<String>)> {
    let ownership = metainfo::backup_ownership(entry)?;
    (ownership.dept.is_some() || ownership.user.is_some())
        .then_some((ownership.dept, ownership.user))
}

fn print_ownership(entry: &BackupEntry, indent: &str) {
    let Some((dept, user)) = ownership_summary(entry) else {
        return;
    };
    if let Some(dept) = dept {
        println!(
            "{}{}  {}",
            indent,
            crate::ui::dim("Dept"),
            crate::ui::dim(&dept)
        );
    }
    if let Some(user) = user {
        println!(
            "{}{}  {}",
            indent,
            crate::ui::dim("User"),
            crate::ui::dim(&user)
        );
    }
}

fn print_numbered_backup_entries(entries: &[&BackupEntry]) {
    let width = entries.len().max(1).to_string().len();
    for (idx, entry) in entries.iter().enumerate() {
        let time = diskio::backup_display_time(&entry.path, entry.mtime);
        println!(
            "  [{}] {}   {}   {}",
            crate::ui::pad_left(&(idx + 1).to_string(), width),
            crate::ui::pad_to(&time, 16),
            crate::ui::pad_to(backup_kind(entry), 12),
            backup_health(entry)
        );
        println!(
            "      └─ {}",
            crate::ui::dim(backup_catalog::file_name(entry))
        );
    }
}

fn print_global_numbered_backup_entries(
    entries: &[&BackupEntry],
    global_index: &BTreeMap<PathBuf, usize>,
) {
    let width = global_index.len().max(1).to_string().len();
    for entry in entries {
        let Some(index) = global_index.get(&backup_catalog::canonical_entry_path(&entry.path))
        else {
            continue;
        };
        let time = diskio::backup_display_time(&entry.path, entry.mtime);
        println!(
            "  [{}] {}   {}   {}",
            crate::ui::pad_left(&index.to_string(), width),
            crate::ui::pad_to(&time, 16),
            crate::ui::pad_to(backup_kind(entry), 12),
            backup_health(entry)
        );
        println!(
            "      └─ {}",
            crate::ui::dim(backup_catalog::file_name(entry))
        );
    }
}

pub fn backup_list(backup_dir: &Path) -> i32 {
    let selector = crate::application::load_backup_selector(backup_dir);
    let catalog = selector.catalog();
    let global_index: BTreeMap<PathBuf, usize> = selector
        .numbered()
        .into_iter()
        .enumerate()
        .map(|(index, entry)| (backup_catalog::canonical_entry_path(&entry.path), index + 1))
        .collect();
    let selected: Vec<&BackupEntry> = catalog.entries().iter().collect();
    println!("备份目录 {} · {} 份", backup_dir.display(), selected.len());
    if selected.is_empty() {
        return EXIT_OK;
    }

    let mut groups: BTreeMap<String, Vec<&BackupEntry>> = BTreeMap::new();
    let mut unknown = Vec::new();
    for entry in selected {
        if let Some(key) = diskio::backup_group_key(entry) {
            groups.entry(key).or_default().push(entry);
        } else {
            unknown.push(entry);
        }
    }
    let mut grouped: Vec<Vec<&BackupEntry>> = groups.into_values().collect();
    for group in &mut grouped {
        backup_catalog::sort_newest_first(group);
    }
    grouped.sort_by(|a, b| match (a.first(), b.first()) {
        (Some(ae), Some(be)) => diskio::cmp_backup_newest_first(ae, be),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    for group in grouped {
        let Some(meta) = group.first().and_then(|entry| entry.meta.as_ref()) else {
            continue;
        };
        let identity = match &meta.onlyid {
            Some(id) => format!("onlyid={}", id),
            None => crate::ui::yellow("未知盘"),
        };
        println!();
        println!(
            "{} · {} · {} · {} 份",
            backup_model_name(meta),
            backup_capacity(meta),
            identity,
            group.len()
        );
        if let Some(entry) = group.first() {
            print_ownership(entry, "  ");
        }
        print_global_numbered_backup_entries(&group, &global_index);
    }
    if !unknown.is_empty() {
        println!();
        for entry in unknown {
            let name = entry
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("<无效文件名>");
            println!(
                "  └─ {}   {}",
                crate::ui::dim(name),
                crate::ui::dim("未识别(非本工具命名)")
            );
        }
    }
    EXIT_OK
}

pub fn backup_verify(backup_dir: &Path, target: Option<&str>) -> i32 {
    let selector = crate::application::load_backup_selector(backup_dir);
    let catalog = selector.catalog();
    let selected: Vec<&BackupEntry> = if let Some(target) = target {
        match selector.resolve_one(target) {
            Ok(entry) => vec![entry],
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                return EXIT_BACKUP;
            }
        }
    } else {
        if !backup_dir.is_dir() {
            eprintln!(
                "{}",
                crate::ui::red(&format!("错误: 备份目录不存在: {}", backup_dir.display()))
            );
            return EXIT_BACKUP;
        }
        catalog.entries().iter().collect()
    };

    if selected.is_empty() {
        println!("没有可校验的 .edpb 备份。");
        return EXIT_OK;
    }
    let mut bad = 0usize;
    for entry in selected {
        let name = entry
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<无效文件名>");
        if backup_catalog::is_healthy(entry) {
            println!("{}  {}", crate::ui::green("✓"), name);
        } else {
            bad += 1;
            println!(
                "{}  {}  {}",
                crate::ui::red("✗"),
                name,
                backup_health(entry)
            );
        }
    }
    if bad == 0 {
        println!("校验通过。");
        EXIT_OK
    } else {
        eprintln!(
            "{}",
            crate::ui::red(&format!("校验失败: {} 份备份异常。", bad))
        );
        EXIT_BACKUP
    }
}

pub fn backup_prune(backup_dir: &Path, keep: usize, yes: bool) -> i32 {
    if !backup_dir.is_dir() {
        eprintln!(
            "{}",
            crate::ui::red(&format!("错误: 备份目录不存在: {}", backup_dir.display()))
        );
        return EXIT_BACKUP;
    }
    let session = crate::application::backup::DeleteSession::open(backup_dir);
    let plan = match session.plan_prune(keep) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("{}", crate::ui::red(&format!("错误: {}", error.message())));
            return EXIT_BACKUP;
        }
    };
    let Some(stats) = plan.prune_stats.as_ref() else {
        return EXIT_BACKUP;
    };
    if plan.targets.is_empty() {
        println!("无需清理：当前策略不会删除任何备份。");
        println!(
            "保留: 加密原盘 {} 份 · 免密快照 {} 份",
            stats.originals, stats.retained_snapshots
        );
        return EXIT_OK;
    }

    println!(
        "将删除 {} 个免密状态快照(每盘保留最新 {} 份, 加密原盘永不自动删除):",
        plan.targets.len(),
        keep
    );
    for entry in &plan.targets {
        let model = entry
            .meta
            .as_ref()
            .map(backup_model_name)
            .unwrap_or_else(|| "未知盘".into());
        println!("  {}   {}", backup_catalog::file_name(entry), model);
    }
    println!();
    println!(
        "保留: 加密原盘 {} 份 · 免密快照 {} 份",
        stats.originals, stats.retained_snapshots
    );
    if !yes {
        println!("确认执行: edpcli backup prune --keep {} --yes", keep);
        return EXIT_OK;
    }

    let mut failed = 0usize;
    for (_, result) in session.execute(&plan) {
        if let Err(msg) = result {
            failed += 1;
            eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
        }
    }
    if failed == 0 {
        println!(
            "{}",
            crate::ui::green(&format!("已删除 {} 份免密状态快照。", plan.targets.len()))
        );
        EXIT_OK
    } else {
        EXIT_BACKUP
    }
}

pub fn backup_delete(
    backup_dir: &Path,
    targets: &[String],
    yes: bool,
    prompt: &mut dyn Prompter,
) -> i32 {
    if !backup_dir.is_dir() {
        eprintln!(
            "{}",
            crate::ui::red(&format!("错误: 备份目录不存在: {}", backup_dir.display()))
        );
        return EXIT_BACKUP;
    }
    let session = crate::application::backup::DeleteSession::open(backup_dir);
    let numbered = session.selector().numbered();
    if numbered.is_empty() {
        println!("没有可删除的备份。");
        return EXIT_OK;
    }
    let numbered_index: BTreeMap<PathBuf, usize> = numbered
        .iter()
        .enumerate()
        .map(|(index, entry)| (backup_catalog::canonical_entry_path(&entry.path), index + 1))
        .collect();

    // 交互选择留在 CLI，但条目解析与确认快照仍来自同一份 DeleteSession 扫描。
    let plan = if targets.is_empty() {
        println!("请选择要删除的备份:");
        print_numbered_backup_entries(&numbered);
        let selected = loop {
            let input = prompt.prompt_line("选择 [如 2 / 1,3 / 2-4，回车取消]: ");
            let input = input.trim();
            if input.is_empty() {
                eprintln!("已取消");
                return EXIT_CANCELLED;
            }
            match session.selector().resolve_many(&[input.to_string()]) {
                Ok(entries) => break entries.into_iter().cloned().collect::<Vec<BackupEntry>>(),
                Err(msg) => eprintln!("{}", crate::ui::red(&format!("错误: {}", msg))),
            }
        };
        session.plan_resolved(selected)
    } else {
        session.plan_targets(targets)
    };
    let plan = match plan {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("{}", crate::ui::red(&format!("错误: {}", error.message())));
            return EXIT_BACKUP;
        }
    };

    let resolved: Vec<PathBuf> = plan
        .targets
        .iter()
        .map(|entry| backup_catalog::canonical_entry_path(&entry.path))
        .collect();
    println!("将删除 {} 份备份:", resolved.len());
    for (path, entry) in resolved.iter().zip(&plan.targets) {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<无效文件名>");
        let idx = numbered_index.get(path).copied().unwrap_or(0);
        let time = diskio::backup_display_time(&entry.path, entry.mtime);
        println!(
            "  [{}] {}   {}   {}",
            idx,
            time,
            backup_kind(entry),
            backup_health(entry)
        );
        println!("      └─ {}", crate::ui::dim(name));
    }
    if !yes && !prompt.confirm_yes("输入 YES 确认删除: ") {
        eprintln!("已取消");
        return EXIT_CANCELLED;
    }

    let mut failed = 0usize;
    for (_, result) in session.execute(&plan) {
        if let Err(msg) = result {
            failed += 1;
            eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
        }
    }
    if failed == 0 {
        println!(
            "{}",
            crate::ui::green(&format!("已删除 {} 份备份。", resolved.len()))
        );
        EXIT_OK
    } else {
        EXIT_BACKUP
    }
}
