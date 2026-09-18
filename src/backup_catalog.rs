//! 备份目录领域模型。
//!
//! CLI、inspect 与 shell completion 都必须通过这里解释 onlyid、盘内编号和备份路径，
//! 避免各自重复排序/解析后产生“同一个 [N] 指向不同文件”的行为漂移。

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
