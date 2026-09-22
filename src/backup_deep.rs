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

mod fat;

/// Filesystem parsers only see relative, read-only sectors. A future decrypted
/// reader must authenticate its key and preserve decoded evidence separately.
pub trait PartitionReader {
    fn read_sector(&mut self, relative_lba: u64) -> std::io::Result<Vec<u8>>;
}

/// Bounded raw partition view, recording exactly the evidence read by parsers.
pub struct RawPartitionReader<'a> {
    dev: &'a mut dyn crate::diskio::SectorDev,
    partition: &'a PartitionGeometry,
    pub(crate) sectors: std::collections::BTreeMap<u64, Vec<u8>>,
}
impl<'a> RawPartitionReader<'a> {
    pub fn new(
        dev: &'a mut dyn crate::diskio::SectorDev,
        partition: &'a PartitionGeometry,
    ) -> Self {
        Self {
            dev,
            partition,
            sectors: Default::default(),
        }
    }
}
impl PartitionReader for RawPartitionReader<'_> {
    fn read_sector(&mut self, relative_lba: u64) -> std::io::Result<Vec<u8>> {
        use std::io::{Error, ErrorKind};
        if relative_lba >= self.partition.sector_count {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "sector outside partition",
            ));
        }
        if let Some(bytes) = self.sectors.get(&relative_lba) {
            return Ok(bytes.clone());
        }
        if self.sectors.len() >= 131_072 {
            return Err(Error::other("Deep evidence exceeds 64 MiB budget"));
        }
        let absolute = self
            .partition
            .start_sector
            .checked_add(relative_lba)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| Error::other("partition LBA overflow"))?;
        let bytes = self.dev.read_sector(absolute)?;
        if bytes.len() != 512 {
            return Err(Error::new(ErrorKind::UnexpectedEof, "truncated sector"));
        }
        self.sectors.insert(relative_lba, bytes.clone());
        Ok(bytes)
    }
}

pub fn analyze_partition(
    p: &PartitionGeometry,
    reader: &mut dyn PartitionReader,
) -> PartitionAnalysis {
    let mut report = assess_partition(p, None);
    if p.need_encrypt != 0 {
        report.status = AnalysisStatus::Locked;
        report.reason = "encrypted partition; no verified decrypted reader is available".into();
        return report;
    }
    if p.sector_size != 512 || p.partition_size != p.sector_count.saturating_mul(512) {
        report.status = AnalysisStatus::ParseFailed;
        report.reason = "invalid partition geometry".into();
        return report;
    }
    let boot = match reader.read_sector(0) {
        Ok(b) if b.len() == 512 => b,
        Ok(_) => {
            report.status = AnalysisStatus::ParseFailed;
            report.reason = "truncated boot sector".into();
            return report;
        }
        Err(e) => {
            report.status = AnalysisStatus::ParseFailed;
            report.reason = e.to_string();
            return report;
        }
    };
    // Reuse Metadata's recognizer for known signatures. FAT16 is classified
    // by BPB cluster count below, not by the informational FAT label.
    let probe = crate::backup_metadata::probe_filesystem(p, &boot);
    if matches!(
        probe.kind,
        crate::backup_metadata::FilesystemKind::Ntfs
            | crate::backup_metadata::FilesystemKind::Exfat
    ) || &boot[3..11] == b"NTFS    "
        || &boot[3..11] == b"EXFAT   "
    {
        report.status = AnalysisStatus::Unsupported;
        report.reason = "filesystem inventory parser is not implemented for exFAT/NTFS yet".into();
        return report;
    }
    if !matches!(boot[0], 0xeb | 0xe9) {
        report.status = AnalysisStatus::Unsupported;
        report.reason = "unrecognized filesystem boot sector".into();
        return report;
    }
    match fat::parse(reader, p.sector_count, &boot) {
        Ok(fs) => {
            report.status = AnalysisStatus::Parsed;
            report.filesystem = Some(fs.kind.into());
            report.total_bytes = Some(fs.total);
            report.free_bytes = Some(fs.free);
            report.used_bytes = Some(fs.total - fs.free);
            report.file_count = Some(fs.entries.iter().filter(|e| !e.is_directory).count() as u64);
            report.directory_count = Some(
                fs.entries
                    .iter()
                    .filter(|e| e.is_directory && e.path != "/")
                    .count() as u64,
            );
            report.entries = Some(fs.entries);
            report.reason = "complete directory traversal; used bytes include filesystem overhead and allocated clusters".into();
        }
        Err(e) => {
            report.status = AnalysisStatus::ParseFailed;
            report.reason = e;
        }
    }
    report
}
