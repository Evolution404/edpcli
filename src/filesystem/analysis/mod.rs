//! Read-only filesystem analysis used by provisioning and migration.

use crate::domain::geometry::PartitionGeometry;
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
}

fn empty_analysis(p: &PartitionGeometry) -> PartitionAnalysis {
    PartitionAnalysis {
        schema: "edpcli.filesystem.analysis.v1",
        partition_index: p.index,
        partition_type: p.partition_type,
        partition_bytes: p.partition_size,
        status: AnalysisStatus::Unsupported,
        filesystem: None,
        total_bytes: None,
        used_bytes: None,
        free_bytes: None,
        file_count: None,
        directory_count: None,
        entries: None,
        reason: "filesystem has not been analyzed".into(),
    }
}

mod exfat;
mod fat;

/// Filesystem parsers only see relative, read-only *plaintext* sectors.
/// Physical encryption is resolved before entering this domain; callers must supply
/// an identity reader for plaintext regions or a verified decrypting reader for encrypted
/// regions. Protocol NeedEncrypt is deliberately not consulted here.
pub trait PartitionReader {
    fn read_sector(&mut self, relative_lba: u64) -> std::io::Result<Vec<u8>>;
}

pub fn analyze_partition(
    p: &PartitionGeometry,
    reader: &mut dyn PartitionReader,
) -> PartitionAnalysis {
    let mut report = empty_analysis(p);
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
    let probe = crate::filesystem::probe::probe_filesystem(p, &boot);
    if matches!(probe.kind, crate::domain::geometry::FilesystemKind::Exfat)
        || &boot[3..11] == b"EXFAT   "
    {
        match exfat::parse(reader, p.sector_count, &boot) {
            Ok(fs) => {
                report.status = AnalysisStatus::Parsed;
                report.filesystem = Some("exfat".into());
                report.total_bytes = Some(fs.total);
                report.free_bytes = Some(fs.free);
                report.used_bytes = Some(fs.total - fs.free);
                report.file_count = Some(
                    fs.entries
                        .iter()
                        .filter(|entry| !entry.is_directory)
                        .count() as u64,
                );
                report.directory_count = Some(
                    fs.entries
                        .iter()
                        .filter(|entry| entry.is_directory && entry.path != "/")
                        .count() as u64,
                );
                report.entries = Some(fs.entries);
                report.reason =
                    "complete exFAT directory traversal; used bytes include filesystem overhead and allocated clusters".into();
            }
            Err(error) => {
                report.status = AnalysisStatus::ParseFailed;
                report.filesystem = Some("exfat".into());
                report.reason = error;
            }
        }
        return report;
    }
    if matches!(probe.kind, crate::domain::geometry::FilesystemKind::Ntfs)
        || &boot[3..11] == b"NTFS    "
    {
        report.status = AnalysisStatus::Unsupported;
        report.filesystem = Some("ntfs".into());
        report.reason = "filesystem inventory parser is not implemented for NTFS yet".into();
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
