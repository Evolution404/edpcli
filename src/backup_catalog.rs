//! 备份目录领域模型。
//!
//! CLI、inspect 与 shell completion 都必须通过这里解释 onlyid、盘内编号和备份路径，
//! 避免各自重复排序/解析后产生“同一个 [N] 指向不同文件”的行为漂移。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::diskio::{self, BackupEntry, Md5Status};

#[derive(Debug, Clone)]
pub struct BackupCatalog {
    root: PathBuf,
    entries: Vec<BackupEntry>,
}

impl BackupCatalog {
    /// 扫描备份目录。目录不存在时保留空目录语义，具体命令可自行决定返回 0/5。
    pub fn load(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            entries: diskio::scan_backup_dir(root),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn entries(&self) -> &[BackupEntry] {
        &self.entries
    }

    /// 某 onlyid 下的备份，固定按文件名中的备份创建时间新→旧；
    /// 仅无法解析旧命名时回退 mtime。
    pub fn onlyid_group(&self, onlyid: &str) -> Result<Vec<&BackupEntry>, String> {
        let mut group: Vec<&BackupEntry> = self
            .entries
            .iter()
            .filter(|entry| {
                entry.meta.as_ref().and_then(|m| m.onlyid.as_deref()) == Some(onlyid)
            })
            .collect();
        if group.is_empty() {
            return Err(format!("未找到 onlyid={} 的备份", onlyid));
        }
        sort_newest_first(&mut group);
        Ok(group)
    }

    /// 以用户看到的 1-based 编号选取备份。
    pub fn onlyid_index(&self, onlyid: &str, index: usize) -> Result<&BackupEntry, String> {
        if index == 0 {
            return Err("备份编号从 1 开始".into());
        }
        let group = self.onlyid_group(onlyid)?;
        group.get(index - 1).copied().ok_or_else(|| {
            format!(
                "onlyid={} 只有 {} 份备份，没有编号 [{}]",
                onlyid,
                group.len(),
                index
            )
        })
    }

    /// onlyid 候选按该盘最新备份时间新→旧排序，供 UI 与补全共同使用。
    pub fn onlyid_values(&self) -> Vec<String> {
        let mut latest: BTreeMap<String, &BackupEntry> = BTreeMap::new();
        for entry in &self.entries {
            let Some(id) = entry.meta.as_ref().and_then(|m| m.onlyid.as_ref()) else {
                continue;
            };
            latest
                .entry(id.clone())
                .and_modify(|current| {
                    if diskio::cmp_backup_newest_first(entry, current).is_lt() {
                        *current = entry;
                    }
                })
                .or_insert(entry);
        }
        let mut values: Vec<(String, &BackupEntry)> = latest.into_iter().collect();
        values.sort_by(|a, b| {
            diskio::cmp_backup_newest_first(a.1, b.1).then_with(|| a.0.cmp(&b.0))
        });
        values.into_iter().map(|(id, _)| id).collect()
    }

    /// 解析精确文件目标。裸文件名相对备份目录；任意路径最终都必须位于备份根目录内。
    pub fn resolve_target(&self, target: &str) -> Result<&BackupEntry, String> {
        let root = fs::canonicalize(&self.root)
            .map_err(|e| format!("备份目录不可访问 {}: {}", self.root.display(), e))?;
        let raw = Path::new(target);
        let candidate = if raw.components().count() == 1 {
            self.root.join(raw)
        } else if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(raw)
        };
        let path = fs::canonicalize(&candidate)
            .map_err(|e| format!("备份文件不存在或不可访问 {}: {}", candidate.display(), e))?;
        if !path.starts_with(&root) {
            return Err(format!("拒绝访问备份目录之外的路径: {}", path.display()));
        }
        if path.extension().and_then(|e| e.to_str()) != Some("bin") {
            return Err(format!("目标不是 .bin 备份: {}", path.display()));
        }
        self.entries
            .iter()
            .find(|entry| canonical_entry_path(&entry.path) == path)
            .ok_or_else(|| "目标不是可扫描的 .bin 备份".into())
    }
}

pub fn sort_newest_first(entries: &mut Vec<&BackupEntry>) {
    entries.sort_by(|a, b| diskio::cmp_backup_newest_first(a, b));
}

pub fn canonical_entry_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn file_name(entry: &BackupEntry) -> &str {
    entry
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<无效文件名>")
}

pub fn is_healthy(entry: &BackupEntry) -> bool {
    entry.size_ok && entry.md5_ok == Md5Status::Ok
}
