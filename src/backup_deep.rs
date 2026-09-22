//! Read-only Deep analysis. Raw evidence is never replaced by interpretation.

use crate::backup_metadata::PartitionGeometry;
use crate::edpb::{ArtifactCompleteness, ArtifactInput, Derivation, RestorePolicy};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    Parsed,
    Locked,
    Unsupported,
    ParseFailed,
    NotCaptured,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileEntry {
    pub path: String,
    pub is_directory: bool,
    pub logical_size: u64,
    pub allocated_size: Option<u64>,
    /// Filesystem-local ISO 8601 wall time; no invented UTC offset.
    pub mtime: Option<String>,
    /// Creation time (not POSIX inode change time).
    pub ctime: Option<String>,
    pub attributes: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct PartitionAnalysis {
    pub schema: &'static str,
    pub partition_index: usize,
    pub partition_type: u32,
    pub partition_bytes: u64,
    pub status: AnalysisStatus,
    pub filesystem: Option<String>,
    pub total_bytes: Option<u64>,
    pub used_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub file_count: Option<u64>,
    /// Excludes the root, which is represented explicitly in entries.
    pub directory_count: Option<u64>,
    pub entries: Option<Vec<FileEntry>>,
    pub reason: String,
    #[serde(skip)]
    source_artifact_ids: Vec<String>,
    #[serde(skip)]
    source_extent_ids: Vec<String>,
}

impl PartitionAnalysis {
    pub fn into_artifact(self) -> Result<ArtifactInput, String> {
        Ok(ArtifactInput {
            id: format!(
                "derived.partition.{}.filesystem_summary",
                self.partition_index
            ),
            kind: "filesystem_summary".into(),
            media_type: "application/json".into(),
            source_extent_ids: self.source_extent_ids.clone(),
            derivation: Some(Derivation {
                method: "deep_filesystem_v1".into(),
                source_artifact_ids: self.source_artifact_ids.clone(),
            }),
            restore_policy: RestorePolicy::DerivedOnly,
            completeness: if self.status == AnalysisStatus::Parsed {
                ArtifactCompleteness::Complete
            } else {
                ArtifactCompleteness::NotCaptured
            },
            data: serde_json::to_vec_pretty(&self).map_err(|e| e.to_string())?,
        })
    }
}

/// A prefix alone cannot prove volume statistics or a complete directory tree.
pub fn assess_partition(p: &PartitionGeometry, prefix: Option<&[u8]>) -> PartitionAnalysis {
    let (status, reason) = match prefix {
        None => (
            AnalysisStatus::NotCaptured,
            "partition prefix was not captured",
        ),
        Some(bytes) if bytes.len() < 512 => {
            (AnalysisStatus::ParseFailed, "truncated partition prefix")
        }
        Some(_) if p.need_encrypt != 0 => (
            AnalysisStatus::Locked,
            "encrypted partition; no verified decrypted reader is available",
        ),
        Some(_) => (
            AnalysisStatus::Unsupported,
            "a raw metadata prefix is insufficient for filesystem inventory",
        ),
    };
    let mut report = PartitionAnalysis {
        schema: "edpcli.deep.filesystem.v1",
        partition_index: p.index,
        partition_type: p.partition_type,
        partition_bytes: p.partition_size,
        status,
        filesystem: None,
        total_bytes: None,
        used_bytes: None,
        free_bytes: None,
        file_count: None,
        directory_count: None,
        entries: None,
        reason: reason.into(),
        source_artifact_ids: vec!["raw.protocol.lba0_12".into()],
        source_extent_ids: vec!["extent.protocol.lba0_12".into()],
    };
    if prefix.is_some() {
        report
            .source_artifact_ids
            .push(format!("raw.partition.{}.prefix", p.index));
        report
            .source_extent_ids
            .push(format!("extent.partition.{}.prefix", p.index));
    }
    report
}
