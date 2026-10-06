//! Isolate the directory entry before checking/deleting it. A replacement at
//! the original name is never the target of the final unlink.
use crate::infrastructure::backup_store::catalog::BackupEntry;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

struct IsolatedEntry {
    original: PathBuf,
    directory: PathBuf,
    payload: PathBuf,
}

impl IsolatedEntry {
    fn take(original: &Path) -> Result<Self, String> {
        let parent = original
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|error| format!("删除隔离名称生成失败: {error}"))?;
        let nonce: String = crate::common::hex_lower(&random);
        let directory = parent.join(format!(".edpcli-delete-{nonce}"));
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            let mut builder = builder;
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
            builder
        };
        builder
            .create(&directory)
            .map_err(|error| format!("删除隔离目录创建失败: {error}"))?;
        let isolated = Self {
            original: original.to_path_buf(),
            payload: directory.join("entry.edpb"),
            directory,
        };
        fs::rename(original, &isolated.payload)
            .map_err(|error| format!("删除前无法隔离 {}: {error}", original.display()))?;
        Ok(isolated)
    }

    fn restore(&self, reason: impl std::fmt::Display) -> String {
        // hard_link creates the original name exclusively: never overwrite a
        // concurrently created file. Unsupported filesystems retain the payload.
        if fs::symlink_metadata(&self.payload).is_ok_and(|metadata| metadata.file_type().is_file())
            && fs::hard_link(&self.payload, &self.original).is_ok()
            // Make the recovered name durable before releasing the private link.
            && crate::platform::sync_directory(
                self.original.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new(".")),
            ).is_ok()
            && fs::remove_file(&self.payload).is_ok()
        {
            return format!("{reason}；原文件已恢复，未删除。");
        }
        format!(
            "{reason}；文件未删除，保留于 {}；请人工恢复，禁止覆盖原路径。",
            self.payload.display()
        )
    }
}

impl Drop for IsolatedEntry {
    fn drop(&mut self) {
        // Never recursively clean up: failed/recoverable deletions retain data.
        let _ = fs::remove_dir(&self.directory);
    }
}

#[derive(Clone, Copy)]
enum DeletePhase {
    Isolated,
    Verified,
}

pub fn delete_entry_verified(entry: &BackupEntry) -> Result<(), String> {
    delete_with_hook(entry, |_, _| {})
}

fn delete_with_hook(
    entry: &BackupEntry,
    mut hook: impl FnMut(DeletePhase, &Path),
) -> Result<(), String> {
    let expected = entry
        .content_sha256
        .as_deref()
        .ok_or_else(|| format!("扫描时无法取得内容摘要，拒绝删除: {}", entry.path.display()))?;
    let metadata = fs::symlink_metadata(&entry.path)
        .map_err(|error| format!("删除前无法重新检查 {}: {error}", entry.path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!(
            "删除前目标已不再是普通文件，拒绝删除: {}",
            entry.path.display()
        ));
    }
    let isolated = IsolatedEntry::take(&entry.path)?;
    hook(DeletePhase::Isolated, &isolated.payload);
    let checked = (|| -> io::Result<()> {
        let kind = fs::symlink_metadata(&isolated.payload)?.file_type();
        if !kind.is_file() {
            return Err(io::Error::other("隔离目标不是普通文件"));
        }
        crate::platform::sync_directory(&isolated.directory)?;
        crate::platform::sync_directory(
            isolated
                .original
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?;
        let mut file = fs::File::open(&isolated.payload)?;
        let actual = crate::sha256::sha256_reader_hex(&mut file, crate::edpb::MAX_CONTAINER_BYTES)?;
        if actual != expected {
            return Err(io::Error::other(
                "备份在扫描/确认后内容已变化，拒绝删除同名新文件",
            ));
        }
        Ok(())
    })();
    if let Err(error) = checked {
        return Err(isolated.restore(error));
    }
    hook(DeletePhase::Verified, &isolated.payload);
    fs::remove_file(&isolated.payload)
        .map_err(|error| isolated.restore(format!("删除失败: {error}")))?;
    crate::platform::sync_directory(&isolated.directory)
        .map_err(|error| format!("文件已删除，但隔离目录同步失败: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::backup_store::catalog::{BackupEntry, BackupIntegrityStatus};
    fn fixture() -> (PathBuf, BackupEntry) {
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let root = std::env::temp_dir().join(format!(
            "edpcli-delete-test-{}",
            crate::sha256::sha256_hex(&nonce)
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("sample.edpb");
        fs::write(&path, b"original").unwrap();
        let entry = BackupEntry {
            display_cached: false,
            meta: None,
            path,
            mtime: 0,
            provision_kind: None,
            integrity_status: BackupIntegrityStatus::Invalid,
            size_ok: false,
            lba8: None,
            verification_error: None,
            content_sha256: Some(crate::sha256::sha256_hex(b"original")),
            coverage: None,
            manifest: None,
            restore_preview: None,
        };
        (root, entry)
    }
    #[test]
    fn replacement_after_verification_survives_deletion() {
        let (root, entry) = fixture();
        delete_with_hook(&entry, |phase, _| {
            if matches!(phase, DeletePhase::Verified) {
                fs::write(&entry.path, b"replacement").unwrap();
            }
        })
        .unwrap();
        assert_eq!(fs::read(&entry.path).unwrap(), b"replacement");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn changed_isolated_file_is_preserved_without_overwriting_replacement() {
        let (root, entry) = fixture();
        let error = delete_with_hook(&entry, |phase, payload| {
            if matches!(phase, DeletePhase::Isolated) {
                fs::write(payload, b"changed").unwrap();
                fs::write(&entry.path, b"replacement").unwrap();
            }
        })
        .unwrap_err();
        assert!(error.contains("保留于"));
        assert_eq!(fs::read(&entry.path).unwrap(), b"replacement");
        let isolated = fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .find(|item| item.file_type().unwrap().is_dir())
            .unwrap();
        assert_eq!(
            fs::read(isolated.path().join("entry.edpb")).unwrap(),
            b"changed"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn changed_file_restores_to_an_empty_original_name() {
        let (root, entry) = fixture();
        fs::write(&entry.path, b"changed").unwrap();
        let error = delete_entry_verified(&entry).unwrap_err();
        assert!(error.contains("原文件已恢复"));
        assert_eq!(fs::read(&entry.path).unwrap(), b"changed");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
