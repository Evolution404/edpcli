//! Read-only filesystem analysis used by provisioning and migration.

use crate::backup_metadata::PartitionGeometry;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    Parsed,
    Locked,
    Unsupported,
    ParseFailed,
    NotCaptured,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilePayloadExtent {
    /// Partition-relative first sector.
    pub start_lba: u64,
    pub sector_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilePayloadLocator {
    pub logical_size: u64,
    pub extents: Vec<FilePayloadExtent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PayloadReadSummary {
    pub logical_size: u64,
    pub sectors_read: u64,
    pub sha256: [u8; 32],
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
    /// Read-only physical locator used by filesystem inspection/verification.
    /// It is runtime-only analysis metadata and is not serialized.
    #[serde(skip)]
    pub payload_locator: Option<FilePayloadLocator>,
}

pub(super) fn payload_locator_from_cluster_lbas(
    logical_size: u64,
    sectors_per_cluster: u64,
    cluster_starts: &[u64],
) -> Result<FilePayloadLocator, String> {
    if sectors_per_cluster == 0 {
        return Err("payload locator has zero sectors per cluster".into());
    }
    let mut extents: Vec<FilePayloadExtent> = Vec::new();
    for &start_lba in cluster_starts {
        let end = start_lba
            .checked_add(sectors_per_cluster)
            .ok_or_else(|| "payload extent overflows".to_string())?;
        if let Some(last) = extents.last_mut() {
            let last_end = last
                .start_lba
                .checked_add(last.sector_count)
                .ok_or_else(|| "payload extent overflows".to_string())?;
            if last_end == start_lba {
                last.sector_count = end
                    .checked_sub(last.start_lba)
                    .ok_or_else(|| "payload extent underflows".to_string())?;
                continue;
            }
        }
        extents.push(FilePayloadExtent {
            start_lba,
            sector_count: sectors_per_cluster,
        });
    }
    let capacity_sectors = extents.iter().try_fold(0u64, |total, extent| {
        total
            .checked_add(extent.sector_count)
            .ok_or_else(|| "payload capacity overflows".to_string())
    })?;
    let capacity_bytes = capacity_sectors
        .checked_mul(512)
        .ok_or_else(|| "payload capacity overflows".to_string())?;
    if logical_size > capacity_bytes {
        return Err("file payload exceeds cluster allocation".into());
    }
    Ok(FilePayloadLocator {
        logical_size,
        extents,
    })
}

pub fn stream_file_payload(
    reader: &mut dyn PartitionReader,
    entry: &FileEntry,
    max_bytes: u64,
    sink: &mut dyn Write,
) -> Result<PayloadReadSummary, String> {
    if entry.is_directory {
        return Err("directory has no ordinary file payload".into());
    }
    let locator = entry
        .payload_locator
        .as_ref()
        .ok_or_else(|| "file payload locator is unavailable".to_string())?;
    if locator.logical_size != entry.logical_size {
        return Err("file payload locator logical size mismatch".into());
    }
    if entry.logical_size > max_bytes {
        return Err(format!(
            "file payload exceeds read budget: {} > {} bytes",
            entry.logical_size, max_bytes
        ));
    }

    let mut capacity_sectors = 0u64;
    for extent in &locator.extents {
        if extent.sector_count == 0 {
            return Err("file payload locator contains empty extent".into());
        }
        extent
            .start_lba
            .checked_add(extent.sector_count)
            .ok_or_else(|| "file payload extent overflows".to_string())?;
        capacity_sectors = capacity_sectors
            .checked_add(extent.sector_count)
            .ok_or_else(|| "file payload capacity overflows".to_string())?;
    }
    let capacity_bytes = capacity_sectors
        .checked_mul(512)
        .ok_or_else(|| "file payload capacity overflows".to_string())?;
    if entry.logical_size > capacity_bytes {
        return Err("file payload locator is shorter than logical size".into());
    }

    let mut remaining = entry.logical_size;
    let mut sectors_read = 0u64;
    let mut sha256 = Sha256::new();
    'extents: for extent in &locator.extents {
        for offset in 0..extent.sector_count {
            if remaining == 0 {
                break 'extents;
            }
            let lba = extent
                .start_lba
                .checked_add(offset)
                .ok_or_else(|| "file payload LBA overflows".to_string())?;
            let sector = reader
                .read_sector(lba)
                .map_err(|error| format!("file payload read failed at LBA {lba}: {error}"))?;
            if sector.len() != 512 {
                return Err(format!("file payload sector {lba} is not 512 bytes"));
            }
            let take = remaining.min(512) as usize;
            sink.write_all(&sector[..take])
                .map_err(|error| format!("file payload sink write failed: {error}"))?;
            sha256.update(&sector[..take]);
            remaining -= take as u64;
            sectors_read += 1;
        }
    }
    if remaining != 0 {
        return Err("file payload locator ended before logical size".into());
    }
    Ok(PayloadReadSummary {
        logical_size: entry.logical_size,
        sectors_read,
        sha256: sha256.finalize().into(),
    })
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
    let probe = crate::backup_metadata::probe_filesystem(p, &boot);
    if matches!(probe.kind, crate::backup_metadata::FilesystemKind::Exfat)
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
    if matches!(probe.kind, crate::backup_metadata::FilesystemKind::Ntfs)
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
