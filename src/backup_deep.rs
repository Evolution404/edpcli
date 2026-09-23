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

mod exfat;
mod fat;
pub mod keys;

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

/// SM4-ECB view for a mode2 key that has passed FileKeyCRC. Raw reads remain
/// in the underlying reader; decoded evidence is recorded separately.
pub struct DecryptedPartitionReader<'a> {
    raw: &'a mut dyn PartitionReader,
    key: [u8; 16],
    decoded: std::collections::BTreeMap<u64, Vec<u8>>,
}
impl PartitionReader for DecryptedPartitionReader<'_> {
    fn read_sector(&mut self, lba: u64) -> std::io::Result<Vec<u8>> {
        if let Some(data) = self.decoded.get(&lba) {
            return Ok(data.clone());
        }
        if self.decoded.len() >= 131_072 {
            return Err(std::io::Error::other("decoded evidence exceeds budget"));
        }
        let raw = self.raw.read_sector(lba)?;
        if raw.len() != 512 {
            return Err(std::io::Error::other("truncated encrypted sector"));
        }
        let decoded = keys::decrypt_mode2(&raw, &self.key).map_err(std::io::Error::other)?;
        self.decoded.insert(lba, decoded.clone());
        Ok(decoded)
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

/// Acquire the complete Metadata policy first, then append Deep evidence and
/// interpretation. Failed analysis never removes an existing raw artifact.
pub fn acquire_deep(
    dev: &mut dyn crate::diskio::SectorDev,
    lba0_12: &[u8],
    device_id: &str,
    total_sectors: u64,
) -> Result<crate::backup_metadata::MetadataAcquisition, String> {
    let mut out = crate::backup_metadata::acquire_metadata(dev, lba0_12, device_id, total_sectors)?;
    let partitions =
        crate::backup_metadata::parse_partition_geometry(lba0_12, device_id, total_sectors)?;
    for p in partitions
        .iter()
        .filter(|p| matches!(p.partition_type, 2 | 4))
    {
        let prefix_id = format!("raw.partition.{}.prefix", p.index);
        let prefix = out
            .artifacts
            .iter()
            .find(|a| a.id == prefix_id)
            .map(|a| a.data.clone());
        let mut report = assess_partition(p, prefix.as_deref());
        let mut file_key = None;
        if report.status == AnalysisStatus::Locked {
            match keys::default_file_key(lba0_12, device_id, p.index) {
                Ok(key) => file_key = Some(key),
                Err(error) => report.reason = error,
            }
        }
        if (report.status == AnalysisStatus::Unsupported && p.need_encrypt == 0)
            || file_key.is_some()
        {
            let mut reader = RawPartitionReader::new(dev, p);
            // Reuse captured sectors so the parser sees the same bytes as the
            // Metadata evidence, even if the live source subsequently changes.
            for a in &out.artifacts {
                if a.kind != "raw_sectors" || a.source_extent_ids.len() != 1 {
                    continue;
                }
                let Some(e) = out.extents.iter().find(|e| e.id == a.source_extent_ids[0]) else {
                    continue;
                };
                if e.start_lba < p.start_sector
                    || e.start_lba + e.sector_count > p.start_sector + p.sector_count
                {
                    continue;
                }
                for (offset, sector) in a.data.as_chunks::<512>().0.iter().enumerate() {
                    reader
                        .sectors
                        .entry(e.start_lba - p.start_sector + offset as u64)
                        .or_insert_with(|| sector.to_vec());
                }
            }
            let mut decoded = std::collections::BTreeMap::new();
            if let Some(key) = file_key {
                let mut view = DecryptedPartitionReader {
                    raw: &mut reader,
                    key,
                    decoded: Default::default(),
                };
                let mut plain_geometry = p.clone();
                plain_geometry.need_encrypt = 0;
                report = analyze_partition(&plain_geometry, &mut view);
                report.reason = format!(
                    "default password key CRC verified; mode2 decoded view: {}",
                    report.reason
                );
                decoded = view.decoded;
            } else {
                report = analyze_partition(p, &mut reader);
            }
            // Store bounded contiguous runs rather than one artifact per sector.
            let mut runs: Vec<(u64, Vec<u8>)> = Vec::new();
            for (lba, bytes) in reader.sectors {
                if let Some((start, data)) = runs.last_mut() {
                    if *start + data.len() as u64 / 512 == lba {
                        data.extend(bytes);
                        continue;
                    }
                }
                runs.push((lba, bytes));
            }
            for (ordinal, (lba, data)) in runs.into_iter().enumerate() {
                let eid = format!("extent.partition.{}.deep.{ordinal}", p.index);
                let aid = format!("raw.partition.{}.deep.{ordinal}", p.index);
                out.extents.push(crate::edpb::Extent {
                    id: eid.clone(),
                    region_id: format!("region.partition.{}.type{}", p.index, p.partition_type),
                    start_lba: p.start_sector + lba,
                    sector_count: data.len() as u64 / 512,
                    purpose: "filesystem_analysis_evidence".into(),
                });
                let end_lba = lba + data.len() as u64 / 512;
                for (&relative, bytes) in decoded.range(lba..end_lba) {
                    let decoded_id = format!("decoded.partition.{}.sector.{relative}", p.index);
                    out.artifacts.push(ArtifactInput {
                        id: decoded_id.clone(),
                        kind: "decoded_sectors".into(),
                        media_type: "application/octet-stream".into(),
                        source_extent_ids: vec![eid.clone()],
                        derivation: Some(Derivation {
                            method: format!(
                                "sm4_ecb_default_v206_crc_verified; relative_lba={relative}"
                            ),
                            source_artifact_ids: vec!["raw.protocol.lba0_12".into(), aid.clone()],
                        }),
                        restore_policy: RestorePolicy::DerivedOnly,
                        completeness: ArtifactCompleteness::Complete,
                        data: bytes.clone(),
                    });
                    report.source_artifact_ids.push(decoded_id);
                }
                out.artifacts.push(ArtifactInput {
                    id: aid.clone(),
                    kind: "raw_sectors".into(),
                    media_type: "application/octet-stream".into(),
                    source_extent_ids: vec![eid.clone()],
                    derivation: None,
                    restore_policy: RestorePolicy::EvidenceOnly,
                    completeness: ArtifactCompleteness::Complete,
                    data,
                });
                report.source_artifact_ids.push(aid);
                report.source_extent_ids.push(eid);
            }
        }
        let mut summary = report.clone().into_artifact()?;
        let mut list = summary.clone();
        list.id = format!("derived.partition.{}.file_list", p.index);
        list.kind = "file_list".into();
        list.data = serde_json::to_vec_pretty(&serde_json::json!({
            "schema":"edpcli.deep.file_list.v1", "partition_index":p.index,
            "partition_type":p.partition_type, "status":report.status, "reason":report.reason,
            "entries":report.entries,
        }))
        .map_err(|e| e.to_string())?;
        // Keep the summary compact; the list is its own derived artifact.
        let mut summary_json = serde_json::to_value(&report).map_err(|e| e.to_string())?;
        summary_json
            .as_object_mut()
            .expect("analysis object")
            .remove("entries");
        summary.data = serde_json::to_vec_pretty(&summary_json).map_err(|e| e.to_string())?;
        out.artifacts.push(summary);
        out.artifacts.push(list);
    }
    out.notes.push("Deep v1: read-only FAT16/FAT32/exFAT inventory; default 0000aaaa mode2 auto-decryption with CRC validation; NTFS unsupported; ordinary file payloads not read".into());
    Ok(out)
}
