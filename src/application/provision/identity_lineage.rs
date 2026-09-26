//! Immutable host-side identity transition records. No USB sector is used for lineage.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::application::media_identity::MediaIdentitySnapshot;

pub(super) const SCHEMA: &str = "edpcli.identity-lineage.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct IdentityTransitionRecord {
    pub schema: String,
    pub transaction_id: String,
    pub created_epoch: i64,
    pub operation: String,
    pub before_identity: MediaIdentitySnapshot,
    pub after_identity: MediaIdentitySnapshot,
    pub mandatory_backup_path: PathBuf,
    pub mandatory_backup_sha256: String,
    pub provision_summary_digest: String,
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Temp file -> fsync file -> atomic rename -> fsync containing directory.
/// A transaction id is never reused; an existing record is never modified.
pub(super) fn persist(
    backup_dir: &Path,
    record: &IdentityTransitionRecord,
) -> Result<PathBuf, String> {
    if record.schema != SCHEMA
        || record.transaction_id.is_empty()
        || !record
            .transaction_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || !valid_digest(&record.mandatory_backup_sha256)
        || !valid_digest(&record.provision_summary_digest)
    {
        return Err("invalid host lineage record fields".into());
    }
    let directory = backup_dir.join(".edpcli/identity-lineage/v1");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("create host lineage directory failed: {error}"))?;
    let final_path = directory.join(format!("{}.json", record.transaction_id));
    if final_path.exists() {
        return Err("immutable host lineage record already exists".into());
    }

    let mut random = [0u8; 8];
    getrandom::fill(&mut random)
        .map_err(|error| format!("host lineage temp id generation failed: {error}"))?;
    let temp_path = directory.join(format!(
        ".{}-{:016x}.tmp",
        record.transaction_id,
        u64::from_be_bytes(random)
    ));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|error| format!("create host lineage temp file failed: {error}"))?;
        serde_json::to_writer_pretty(&mut file, record)
            .map_err(|error| format!("serialize host lineage record failed: {error}"))?;
        file.write_all(b"\n")
            .map_err(|error| format!("write host lineage record failed: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("sync host lineage file failed: {error}"))?;
        drop(file);
        if final_path.exists() {
            return Err("immutable host lineage record already exists".into());
        }
        fs::rename(&temp_path, &final_path)
            .map_err(|error| format!("rename host lineage file failed: {error}"))?;
        crate::platform::sync_directory(&directory)
            .map_err(|error| format!("sync host lineage directory failed: {error}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result.map(|()| final_path)
}
