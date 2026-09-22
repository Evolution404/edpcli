//! 备份目录领域模型。
//!
//! CLI、inspect 与 shell completion 共享这里的目录扫描/路径解释；用户可见的全局稳定编号
//! 统一由 `BackupSelector` 生成，避免各命令各自排序后产生“同一个 [N] 指向不同文件”的漂移。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::diskio::{self, BackupEntry, Sha256Status};

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
        if path.extension().and_then(|e| e.to_str()) != Some("edpb") {
            return Err(format!("目标不是 .edpb 备份: {}", path.display()));
        }
        self.entries
            .iter()
            .find(|entry| canonical_entry_path(&entry.path) == path)
            .ok_or_else(|| "目标不是可扫描的 .edpb 备份".into())
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
    entry.size_ok && entry.sha256_ok == Sha256Status::Ok
}

/// Delete one already-scanned backup entry using content identity, not only its pathname.
///
/// The caller is responsible for higher-level retention policy (for example, keeping at least one
/// backup for a device). This function re-checks that the target is still a regular file and that
/// its content SHA-256 still matches the scan snapshot before deleting the single .edpb file.
pub fn delete_entry_verified(entry: &BackupEntry) -> Result<(), String> {
    let path = &entry.path;
    let metadata = fs::symlink_metadata(path)
        .map_err(|e| format!("删除前无法重新检查 {}: {}", path.display(), e))?;
    if !metadata.file_type().is_file() {
        return Err(format!(
            "删除前目标已不再是普通文件，拒绝删除: {}",
            path.display()
        ));
    }
    let expected = entry
        .content_sha256
        .as_deref()
        .ok_or_else(|| format!("扫描时无法取得内容摘要，拒绝删除: {}", path.display()))?;
    let current =
        fs::read(path).map_err(|e| format!("删除前无法重新读取 {}: {}", path.display(), e))?;
    let actual = crate::sha256::sha256_hex(&current);
    if actual != expected {
        return Err(format!(
            "备份在扫描/确认后内容已变化，拒绝删除同名新文件: {}",
            path.display()
        ));
    }
    if let Err(e) = fs::remove_file(path) {
        let suffix = if e.kind() == io::ErrorKind::PermissionDenied {
            "；备份目录可能由管理员账户持有且不可写，可检查目录属主/权限"
        } else {
            ""
        };
        return Err(format!("删除失败 {}: {}{}", path.display(), e, suffix));
    }
    Ok(())
}
