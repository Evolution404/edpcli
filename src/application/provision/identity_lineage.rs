//! Immutable host-side identity transition records. No USB sector is used for lineage.

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

/// Temp file -> fsync file -> exclusive publication -> fsync containing directory.
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
    let final_path = directory.join(format!("{}.json", record.transaction_id));
    let mut bytes = serde_json::to_vec_pretty(record)
        .map_err(|e| format!("serialize host lineage record failed: {e}"))?;
    bytes.push(b'\n');
    crate::infrastructure::atomic_file::write(&final_path, &bytes, false)
        .map_err(|e| format!("publish immutable host lineage failed: {e}"))?;
    Ok(final_path)
}
