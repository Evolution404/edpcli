//! `nopwd backup` 命令的呈现与操作层。
//!
//! 备份目录的数据解释统一交给 `BackupCatalog`；本模块只负责 CLI 渲染、交互选择、
//! 校验/清理/删除动作。这样 `cli.rs` 不再承载备份领域细节，inspect 也只依赖两个
//! 明确的选择视图接口。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::backup_catalog::{self, BackupCatalog};
use crate::cli::Prompter;
use crate::common::{EXIT_BACKUP, EXIT_CANCELLED, EXIT_OK, EXIT_USAGE, SECTOR};
use crate::diskio::{self, BackupEntry, BackupMeta, Md5Status};

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
        return crate::ui::red(&format!("大小 ✗ (应为 {}B)", 14 * SECTOR));
    }
    match entry.md5_ok {
        Md5Status::Ok => crate::ui::green("MD5 ✓"),
        Md5Status::Mismatch => crate::ui::red("MD5 ✗ 损坏"),
        Md5Status::NoSidecar => crate::ui::yellow("(缺 .md5)"),
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
        println!("      └─ {}", crate::ui::dim(backup_catalog::file_name(entry)));
    }
}

pub(crate) fn print_onlyid_backup_choices(id: &str, group: &[&BackupEntry]) {
    if let Some(meta) = group.first().and_then(|e| e.meta.as_ref()) {
        println!(
            "{} · {} · onlyid={} · {} 份",
            backup_model_name(meta),
            backup_capacity(meta),
            id,
            group.len()
        );
    } else {
        println!("onlyid={} · {} 份", id, group.len());
    }
    print_numbered_backup_entries(group);
}

pub(crate) fn print_inspect_backup_sources(entries: &[BackupEntry]) -> bool {
    let mut groups: BTreeMap<String, Vec<&BackupEntry>> = BTreeMap::new();
    for entry in entries {
        let Some(id) = entry.meta.as_ref().and_then(|m| m.onlyid.as_ref()) else {
            continue;
        };
        groups.entry(id.clone()).or_default().push(entry);
    }
    if groups.is_empty() {
        return false;
    }
    let mut groups: Vec<(String, Vec<&BackupEntry>)> = groups.into_iter().collect();
    for (_, group) in &mut groups {
        backup_catalog::sort_newest_first(group);
    }
    groups.sort_by(|a, b| match (a.1.first(), b.1.first()) {
        (Some(ae), Some(be)) => {
            diskio::cmp_backup_newest_first(ae, be).then_with(|| a.0.cmp(&b.0))
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.0.cmp(&b.0),
    });
    println!("{}", crate::ui::bold("可查看的备份盘:"));
    for (id, group) in groups {
        let latest = group
            .first()
            .map(|e| diskio::backup_display_time(&e.path, e.mtime))
            .unwrap_or_default();
        let model = group
            .first()
            .and_then(|e| e.meta.as_ref())
            .map(backup_model_name)
            .unwrap_or_else(|| "未知型号".into());
        println!(
            "  {}  {} · {} 份 · 最新 {}",
            crate::ui::bold_cyan(&format!("onlyid={}", id)),
            model,
            group.len(),
            latest
        );
    }
    println!();
    println!("{}", crate::ui::dim("查看某盘: nopwd inspect --onlyid <ID>"));
    true
}

pub(crate) fn parse_backup_selection_tokens(
    tokens: &[String],
    max: usize,
) -> Result<Vec<usize>, String> {
    let mut selected = BTreeSet::new();
    for token in tokens {
        for raw in token.split(',') {
            let part = raw.trim();
            if part.is_empty() {
                return Err("备份编号不能为空".into());
            }
            if let Some((left, right)) = part.split_once('-') {
                if right.contains('-') {
                    return Err(format!("无法解析备份范围: {}", part));
                }
                let start = left
                    .parse::<usize>()
                    .map_err(|_| format!("无法解析备份编号: {}", left))?;
                let end = right
                    .parse::<usize>()
                    .map_err(|_| format!("无法解析备份编号: {}", right))?;
                if start == 0 || end == 0 || start > end || end > max {
                    return Err(format!("备份范围超出 1-{}: {}", max, part));
                }
                selected.extend(start..=end);
            } else {
                let idx = part
                    .parse::<usize>()
                    .map_err(|_| format!("无法解析备份编号: {}", part))?;
                if idx == 0 || idx > max {
                    return Err(format!("备份编号超出 1-{}: {}", max, part));
                }
                selected.insert(idx);
            }
        }
    }
    if selected.is_empty() {
        return Err("至少选择一份备份".into());
    }
    Ok(selected.into_iter().collect())
}

pub fn backup_list(backup_dir: &Path, onlyid: Option<&str>) -> i32 {
    let catalog = BackupCatalog::load(backup_dir);
    let selected: Vec<&BackupEntry> = if let Some(id) = onlyid {
        match catalog.onlyid_group(id) {
            Ok(group) => group,
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                return EXIT_BACKUP;
            }
        }
    } else {
        catalog.entries().iter().collect()
    };
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
        print_numbered_backup_entries(&group);
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

pub fn backup_verify(backup_dir: &Path, onlyid: Option<&str>, target: Option<&str>) -> i32 {
    backup_verify_select(backup_dir, onlyid, target, None)
}

pub(crate) fn backup_verify_select(
    backup_dir: &Path,
    onlyid: Option<&str>,
    target: Option<&str>,
    index: Option<usize>,
) -> i32 {
    let catalog = BackupCatalog::load(backup_dir);
    let selected: Vec<&BackupEntry> = if let Some(target) = target {
        match catalog.resolve_target(target) {
            Ok(entry) => vec![entry],
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                return EXIT_BACKUP;
            }
        }
    } else if let Some(id) = onlyid {
        if let Some(idx) = index {
            match catalog.onlyid_index(id, idx) {
                Ok(entry) => vec![entry],
                Err(msg) => {
                    eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                    if let Ok(group) = catalog.onlyid_group(id) {
                        println!();
                        print_onlyid_backup_choices(id, &group);
                    }
                    return EXIT_BACKUP;
                }
            }
        } else {
            match catalog.onlyid_group(id) {
                Ok(group) => group,
                Err(msg) => {
                    eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                    return EXIT_BACKUP;
                }
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
        println!("没有可校验的 .bin 备份。");
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

fn delete_backup_pair(path: &Path) -> Result<(), String> {
    if let Err(e) = fs::remove_file(path) {
        let suffix = if e.kind() == io::ErrorKind::PermissionDenied {
            "；备份目录可能由 root 持有且不可写，可检查目录属主/权限，必要时使用 sudo rm 手动删除"
        } else {
            ""
        };
        return Err(format!("删除失败 {}: {}{}", path.display(), e, suffix));
    }
    let sidecar = diskio::md5_sidecar_path(path);
    if sidecar.exists() {
        if let Err(e) = fs::remove_file(&sidecar) {
            let suffix = if e.kind() == io::ErrorKind::PermissionDenied {
                "；备份目录可能由 root 持有且不可写，可检查目录属主/权限，必要时使用 sudo rm 手动删除"
            } else {
                ""
            };
            return Err(format!(
                "已删除 .bin，但删除校验文件失败 {}: {}{}",
                sidecar.display(),
                e,
                suffix
            ));
        }
    }
    Ok(())
}

pub fn backup_prune(backup_dir: &Path, onlyid: Option<&str>, keep: usize, yes: bool) -> i32 {
    if !backup_dir.is_dir() {
        eprintln!(
            "{}",
            crate::ui::red(&format!("错误: 备份目录不存在: {}", backup_dir.display()))
        );
        return EXIT_BACKUP;
    }
    let catalog = BackupCatalog::load(backup_dir);
    let selected_refs: Vec<&BackupEntry> = if let Some(id) = onlyid {
        match catalog.onlyid_group(id) {
            Ok(group) => group,
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                return EXIT_BACKUP;
            }
        }
    } else {
        catalog.entries().iter().collect()
    };
    let selected: Vec<BackupEntry> = selected_refs.into_iter().cloned().collect();
    let candidates = diskio::prune_candidates(&selected, keep);
    let candidate_set: BTreeSet<PathBuf> = candidates
        .iter()
        .map(|p| backup_catalog::canonical_entry_path(p))
        .collect();

    let originals = selected
        .iter()
        .filter(|e| e.meta.is_some() && !e.is_nopwd)
        .count();
    let snapshots = selected
        .iter()
        .filter(|e| e.meta.is_some() && e.is_nopwd)
        .count();
    let keep_snapshots = snapshots.saturating_sub(candidates.len());
    if candidates.is_empty() {
        println!("无需清理：当前策略不会删除任何备份。");
        println!(
            "保留: 加密原盘 {} 份 · 免密快照 {} 份",
            originals, keep_snapshots
        );
        return EXIT_OK;
    }

    println!(
        "将删除 {} 个免密状态快照(每盘保留最新 {} 份, 加密原盘永不自动删除):",
        candidates.len(),
        keep
    );
    for path in &candidates {
        let model = selected
            .iter()
            .find(|e| {
                backup_catalog::canonical_entry_path(&e.path)
                    == backup_catalog::canonical_entry_path(path)
            })
            .and_then(|e| e.meta.as_ref())
            .map(backup_model_name)
            .unwrap_or_else(|| "未知盘".into());
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<无效文件名>");
        println!("  {}   {}", name, model);
    }
    println!();
    println!(
        "保留: 加密原盘 {} 份 · 免密快照 {} 份",
        originals, keep_snapshots
    );
    if !yes {
        match onlyid {
            Some(id) => println!(
                "确认执行: nopwd backup prune --onlyid {} --keep {} --yes",
                id, keep
            ),
            None => println!("确认执行: nopwd backup prune --keep {} --yes", keep),
        }
        return EXIT_OK;
    }

    let mut failed = 0usize;
    for entry in &selected {
        if candidate_set.contains(&backup_catalog::canonical_entry_path(&entry.path)) {
            if let Err(msg) = delete_backup_pair(&entry.path) {
                failed += 1;
                eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
            }
        }
    }
    if failed == 0 {
        println!(
            "{}",
            crate::ui::green(&format!("已删除 {} 份免密状态快照。", candidates.len()))
        );
        EXIT_OK
    } else {
        EXIT_BACKUP
    }
}

pub fn backup_rm(
    backup_dir: &Path,
    onlyid: Option<&str>,
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
    let catalog = BackupCatalog::load(backup_dir);
    let entries = catalog.entries();
    let mut numbered_index: BTreeMap<PathBuf, usize> = BTreeMap::new();
    let mut selected_group_len = None;
    let mut resolved = if let Some(id) = onlyid {
        let group = match catalog.onlyid_group(id) {
            Ok(group) => group,
            Err(msg) => {
                eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                return EXIT_BACKUP;
            }
        };
        selected_group_len = Some(group.len());
        for (idx, entry) in group.iter().enumerate() {
            numbered_index.insert(backup_catalog::canonical_entry_path(&entry.path), idx + 1);
        }

        let indices = if targets.is_empty() {
            print_onlyid_backup_choices(id, &group);
            loop {
                let input =
                    prompt.prompt_line("选择要删除的备份 [如 2 / 1,3 / 2-3，回车取消]: ");
                let input = input.trim();
                if input.is_empty() {
                    eprintln!("已取消");
                    return EXIT_CANCELLED;
                }
                match parse_backup_selection_tokens(&[input.to_string()], group.len()) {
                    Ok(v) => break v,
                    Err(msg) => eprintln!("{}", crate::ui::red(&format!("错误: {}", msg))),
                }
            }
        } else {
            match parse_backup_selection_tokens(targets, group.len()) {
                Ok(v) => v,
                Err(msg) => {
                    eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                    return EXIT_USAGE;
                }
            }
        };
        indices
            .into_iter()
            .map(|idx| backup_catalog::canonical_entry_path(&group[idx - 1].path))
            .collect::<Vec<_>>()
    } else {
        if targets.is_empty() {
            eprintln!(
                "{}",
                crate::ui::red("错误: backup rm 至少需要一个路径或文件名")
            );
            return EXIT_USAGE;
        }
        let mut paths = Vec::new();
        for target in targets {
            match catalog.resolve_target(target) {
                Ok(entry) => {
                    let path = backup_catalog::canonical_entry_path(&entry.path);
                    if !paths.contains(&path) {
                        paths.push(path);
                    }
                }
                Err(msg) => {
                    eprintln!("{}", crate::ui::red(&format!("错误: {}", msg)));
                    return EXIT_BACKUP;
                }
            }
        }
        paths
    };
    resolved.dedup();

    let mut total_per_group: BTreeMap<String, usize> = BTreeMap::new();
    let mut deleting_per_group: BTreeMap<String, usize> = BTreeMap::new();
    for entry in entries {
        if let Some(key) = diskio::backup_group_key(entry) {
            *total_per_group.entry(key.clone()).or_default() += 1;
            let ep = backup_catalog::canonical_entry_path(&entry.path);
            if resolved.contains(&ep) {
                *deleting_per_group.entry(key).or_default() += 1;
            }
        }
    }
    for (key, deleting) in &deleting_per_group {
        let total = total_per_group.get(key).copied().unwrap_or(0);
        if *deleting >= total && total > 0 {
            eprintln!(
                "{}",
                crate::ui::red(
                    "错误: 安全保护拒绝删除——该盘将被清到零份备份；至少保留 1 份。",
                )
            );
            return EXIT_BACKUP;
        }
    }

    println!("将删除 {} 份备份:", resolved.len());
    for path in &resolved {
        let entry = entries
            .iter()
            .find(|e| backup_catalog::canonical_entry_path(&e.path) == *path);
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<无效文件名>");
        match entry {
            Some(e) if onlyid.is_some() => {
                let idx = numbered_index.get(path).copied().unwrap_or(0);
                let time = diskio::backup_display_time(&e.path, e.mtime);
                println!(
                    "  [{}] {}   {}   {}",
                    idx,
                    time,
                    backup_kind(e),
                    backup_health(e)
                );
                println!("      └─ {}", crate::ui::dim(name));
            }
            Some(e) => println!("  {}   {}   {}", name, backup_kind(e), backup_health(e)),
            None => println!("  {}   {}", name, crate::ui::yellow("未识别")),
        }
    }
    if let Some(total) = selected_group_len {
        println!(
            "删除后该盘仍保留 {} 份备份。",
            total.saturating_sub(resolved.len())
        );
    }
    if !yes && !prompt.confirm_yes("输入 YES 确认删除: ") {
        eprintln!("已取消");
        return EXIT_CANCELLED;
    }

    let mut failed = 0usize;
    for path in &resolved {
        if let Err(msg) = delete_backup_pair(path) {
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
