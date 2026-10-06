//! Metadata-level EDPB capture planning and read-only acquisition.
//!
//! This module never writes the source device. It derives partition geometry
//! from the current LBA12 EDPF table, captures bounded filesystem metadata
//! extents, and preserves identified historical tail backup structures.

use crate::protocol::{edpf::EdpPartitionType, lba7::Lba7PartitionMode};
use serde::Serialize;

use crate::common::SECTOR;
use crate::edpb::{
    ArtifactCompleteness, ArtifactInput, Derivation, Extent, ManifestPartition, Region,
    RestorePolicy, SemanticStatus,
};
use crate::partition_table::{PartitionSource, PartitionTableKind};
use crate::ports::SectorDev;

pub use crate::domain::geometry::{
    LBA7_COMPAT_CHS_BACKOFF_SECTORS, LBA7_COMPAT_CHS_TRACK_SECTORS, LBA7_COMPAT_EXTENT_BYTES,
    LBA7_COMPAT_EXTENT_SECTORS,
};

pub const PARTITION_PREFIX_SECTORS: u64 = 64;
pub const PARTITION_SUFFIX_SECTORS: u64 = 8;
pub const TAIL_METADATA_MIRROR_OFFSET_SECTORS: u64 = 1024;
pub const TAIL_METADATA_MIRROR_SECTORS: u64 = 9;
pub const TAIL_END4_MIRROR_OFFSET_SECTORS: u64 = 4;

pub use crate::domain::geometry::{
    FilesystemKind, FilesystemProbe, Lba7CompatibilityGeometry, Lba7CompatibilityPointer,
    PartitionGeometry,
};

#[derive(Clone, Debug, Default)]
pub struct MetadataAcquisition {
    pub partitions: Vec<ManifestPartition>,
    pub regions: Vec<Region>,
    pub extents: Vec<Extent>,
    pub artifacts: Vec<ArtifactInput>,
    pub notes: Vec<String>,
    pub issues: Vec<CaptureIssue>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CaptureIssue {
    pub region_id: String,
    pub start_lba: u64,
    pub sector_count: u64,
    pub error: String,
}

fn read_extent(
    dev: &mut dyn SectorDev,
    start_lba: u64,
    sector_count: u64,
) -> Result<Vec<u8>, String> {
    let byte_len = sector_count
        .checked_mul(SECTOR as u64)
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| "metadata extent byte length overflow".to_string())?;
    if byte_len > 64 * 1024 * 1024 {
        return Err("metadata extent exceeds allocation budget".into());
    }
    start_lba
        .checked_add(sector_count)
        .filter(|end| *end <= u64::from(u32::MAX) + 1)
        .ok_or("metadata extent exceeds SectorDev address range")?;
    let mut out = Vec::with_capacity(byte_len);
    for offset in 0..sector_count {
        let lba = start_lba
            .checked_add(offset)
            .ok_or_else(|| "metadata extent LBA overflow".to_string())?;
        let lba32 = u32::try_from(lba)
            .map_err(|_| format!("metadata extent LBA {lba} exceeds SectorDev u32 range"))?;
        let sector = dev
            .read_sector(lba32)
            .map_err(|e| format!("read metadata LBA{lba} failed: {e}"))?;
        if sector.len() != SECTOR {
            return Err(format!(
                "read metadata LBA{lba} returned {}B, expected {SECTOR}B",
                sector.len()
            ));
        }
        out.extend_from_slice(&sector);
    }
    Ok(out)
}

// Mirrors one raw-evidence record into the extent/artifact model; keeping the
// fields explicit makes call sites auditable against the on-disk capture schema.
#[allow(clippy::too_many_arguments)]
fn add_raw_extent(
    out: &mut MetadataAcquisition,
    dev: &mut dyn SectorDev,
    region_id: &str,
    extent_id: String,
    artifact_id: String,
    start_lba: u64,
    sector_count: u64,
    purpose: &str,
) -> Result<Option<Vec<u8>>, String> {
    if sector_count == 0 {
        return Ok(Some(Vec::new()));
    }
    let data = match read_extent(dev, start_lba, sector_count) {
        Ok(data) => data,
        Err(error) => {
            out.issues.push(CaptureIssue {
                region_id: region_id.to_string(),
                start_lba,
                sector_count,
                error,
            });
            return Ok(None);
        }
    };
    out.extents.push(Extent {
        id: extent_id.clone(),
        region_id: region_id.to_string(),
        start_lba,
        sector_count,
        purpose: purpose.to_string(),
    });
    out.artifacts.push(ArtifactInput {
        id: artifact_id,
        kind: "raw_sectors".into(),
        media_type: "application/octet-stream".into(),
        source_extent_ids: vec![extent_id],
        derivation: None,
        restore_policy: RestorePolicy::EvidenceOnly,
        completeness: ArtifactCompleteness::Complete,
        data: data.clone(),
    });
    Ok(Some(data))
}

pub use crate::domain::geometry::{parse_lba7_compatibility_geometry, parse_partition_geometry};

fn read_device_sector(dev: &mut dyn SectorDev, lba: u64) -> Result<Vec<u8>, String> {
    let lba = u32::try_from(lba).map_err(|_| format!("LBA{lba} exceeds SectorDev u32 range"))?;
    dev.read_sector(lba)
        .map_err(|error| format!("read LBA{lba} failed: {error}"))
}

struct PartitionFilesystemReader<'a> {
    dev: &'a mut dyn SectorDev,
    start_lba: u64,
    sector_count: u64,
}

impl crate::filesystem::FilesystemReader for PartitionFilesystemReader<'_> {
    fn sector_size(&self) -> u32 {
        SECTOR as u32
    }

    fn sector_count(&self) -> u64 {
        self.sector_count
    }

    fn read_sector(
        &mut self,
        relative_lba: u64,
    ) -> Result<[u8; SECTOR], crate::filesystem::FilesystemError> {
        if relative_lba >= self.sector_count {
            return Err(crate::filesystem::FilesystemError::for_filesystem(
                crate::filesystem::FilesystemKind::Fat16,
                crate::filesystem::FilesystemErrorKind::InvalidGeometry,
                "文件系统读取超出分区范围",
            ));
        }
        let absolute = self.start_lba.checked_add(relative_lba).ok_or_else(|| {
            crate::filesystem::FilesystemError::for_filesystem(
                crate::filesystem::FilesystemKind::Fat16,
                crate::filesystem::FilesystemErrorKind::InvalidGeometry,
                "文件系统绝对 LBA 溢出",
            )
        })?;
        let bytes = read_device_sector(self.dev, absolute).map_err(|message| {
            crate::filesystem::FilesystemError::for_filesystem(
                crate::filesystem::FilesystemKind::Fat16,
                crate::filesystem::FilesystemErrorKind::ReadFailure,
                message,
            )
        })?;
        bytes.try_into().map_err(|_| {
            crate::filesystem::FilesystemError::for_filesystem(
                crate::filesystem::FilesystemKind::Fat16,
                crate::filesystem::FilesystemErrorKind::ReadFailure,
                "文件系统扇区长度不是 512B",
            )
        })
    }
}

fn probe_filesystem_hints(
    dev: &mut dyn SectorDev,
    start_lba: u64,
    sector_count: u64,
) -> Result<(Option<String>, Option<String>), String> {
    let boot = read_device_sector(dev, start_lba)?;
    if boot.len() != SECTOR {
        return Err("filesystem boot sector is truncated".into());
    }
    let Some(kind) = crate::filesystem::detect_boot_sector(sector_count, &boot)
        .map_err(|error| error.to_string())?
    else {
        return Ok((None, None));
    };
    let filesystem = Some(kind.label().to_ascii_lowercase());
    let label = match kind {
        crate::filesystem::FilesystemKind::Fat16 => {
            let mut reader = PartitionFilesystemReader {
                dev,
                start_lba,
                sector_count,
            };
            crate::filesystem::FilesystemDriver::read_metadata(
                &crate::filesystem::FAT16_DRIVER,
                &mut reader,
            )
            .map_err(|error| error.to_string())?
            .volume_label
        }
        crate::filesystem::FilesystemKind::Fat32 => {
            let mut reader = PartitionFilesystemReader {
                dev,
                start_lba,
                sector_count,
            };
            crate::filesystem::FilesystemDriver::read_metadata(
                &crate::filesystem::FAT32_DRIVER,
                &mut reader,
            )
            .map_err(|error| error.to_string())?
            .volume_label
        }
        crate::filesystem::FilesystemKind::ExFat => {
            let mut reader = PartitionFilesystemReader {
                dev,
                start_lba,
                sector_count,
            };
            crate::filesystem::FilesystemDriver::read_metadata(
                &crate::filesystem::EXFAT_DRIVER,
                &mut reader,
            )
            .map_err(|error| error.to_string())?
            .volume_label
        }
        crate::filesystem::FilesystemKind::Fat12 | crate::filesystem::FilesystemKind::Ntfs => None,
    };
    Ok((filesystem, label))
}

fn probe_plain_filesystem_hints(
    dev: &mut dyn SectorDev,
    partition: &crate::partition_table::PhysicalPartition,
) -> Result<(Option<String>, Option<String>), String> {
    probe_filesystem_hints(dev, partition.start_lba, partition.sector_count)
}

fn manifest_partition_from_plain(
    partition: &crate::partition_table::PhysicalPartition,
    filesystem_hint: Option<String>,
    volume_label_hint: Option<String>,
) -> ManifestPartition {
    let (role, partition_type) = match &partition.source {
        PartitionSource::Mbr {
            partition_type,
            primary_slot,
        } => (
            Some(if primary_slot.is_some() {
                "mbr_primary".to_string()
            } else {
                "mbr_logical".to_string()
            }),
            Some(format!("mbr:0x{partition_type:02X}")),
        ),
        PartitionSource::Gpt { type_guid, .. } => (
            Some("gpt_partition".to_string()),
            Some(format!(
                "gpt:{}",
                type_guid
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            )),
        ),
    };
    ManifestPartition {
        index: partition.index as u32,
        role,
        partition_type,
        start_lba: partition.start_lba,
        sector_count: partition.sector_count,
        filesystem_hint,
        volume_label_hint,
    }
}

pub fn acquire_plain_metadata(
    dev: &mut dyn SectorDev,
    total_sectors: u64,
) -> Result<MetadataAcquisition, String> {
    let (table, evidence) =
        crate::partition_table::capture_partition_table(total_sectors, |lba| {
            let lba = u32::try_from(lba)
                .map_err(|_| format!("partition-table LBA{lba} exceeds SectorDev u32 range"))?;
            dev.read_sector(lba)
                .map_err(|error| format!("read partition-table LBA{lba} failed: {error}"))
        })?;

    let mut out = MetadataAcquisition::default();
    for partition in &table.partitions {
        match probe_plain_filesystem_hints(dev, partition) {
            Ok((filesystem_hint, volume_label_hint)) => {
                out.partitions.push(manifest_partition_from_plain(
                    partition,
                    filesystem_hint,
                    volume_label_hint,
                ));
            }
            Err(error) => {
                out.notes.push(format!(
                    "P{} filesystem hint unavailable: {error}",
                    partition.index
                ));
                out.partitions
                    .push(manifest_partition_from_plain(partition, None, None));
            }
        }
    }
    let region_id = "region.plain.partition_table";
    out.regions.push(Region {
        id: region_id.into(),
        role: "plain_partition_table".into(),
        start_lba: None,
        sector_count: None,
        semantic_status: SemanticStatus::Identified,
    });
    for (index, extent) in table.table_extents.iter().enumerate() {
        let data = evidence[index].clone();
        let extent_id = format!("extent.plain.partition_table.{index}");
        out.extents.push(Extent {
            id: extent_id.clone(),
            region_id: region_id.into(),
            start_lba: extent.start_lba,
            sector_count: extent.sector_count,
            purpose: extent.label.clone(),
        });
        out.artifacts.push(ArtifactInput {
            id: format!("raw.plain.partition_table.{index}"),
            kind: "raw_sectors".into(),
            media_type: "application/octet-stream".into(),
            source_extent_ids: vec![extent_id],
            derivation: None,
            restore_policy: RestorePolicy::Restorable,
            completeness: ArtifactCompleteness::Complete,
            data,
        });
    }
    out.notes.push(match table.kind {
        PartitionTableKind::Mbr => "Plain MBR metadata only; filesystem/user data excluded".into(),
        PartitionTableKind::Gpt => {
            "Plain GPT primary/backup metadata only; filesystem/user data excluded".into()
        }
    });
    Ok(out)
}

fn manifest_partition_from_edp(
    mode: Option<Lba7PartitionMode>,
    partition: &PartitionGeometry,
    filesystem_hint: Option<String>,
    volume_label_hint: Option<String>,
) -> ManifestPartition {
    let role = match (mode, EdpPartitionType::from_raw(partition.partition_type)) {
        (Some(mode), Some(partition_type)) => {
            match crate::provision::official_partition_role(mode, partition.index, partition_type) {
                crate::provision::PartitionRole::Boot => "boot",
                crate::provision::PartitionRole::Share => "share",
                crate::provision::PartitionRole::Encrypt => "encrypt",
                crate::provision::PartitionRole::BootShareCombined => "boot_share_combined",
                crate::provision::PartitionRole::CompatibilityReserve => "compatibility_reserve",
            }
        }
        _ => match partition.partition_type {
            1 => "boot",
            2 => "share",
            4 => "encrypt",
            _ => "unknown",
        },
    };
    ManifestPartition {
        index: (partition.index + 1) as u32,
        role: Some(role.into()),
        partition_type: Some(format!("edp:{}", partition.partition_type)),
        start_lba: partition.start_sector,
        sector_count: partition.sector_count,
        filesystem_hint,
        volume_label_hint,
    }
}

pub fn acquire_metadata(
    dev: &mut dyn SectorDev,
    lba0_12: &[u8],
    device_id: &str,
    total_sectors: u64,
) -> Result<MetadataAcquisition, String> {
    if total_sectors < 13 {
        return Err("source disk is smaller than protocol region".into());
    }
    let partitions = parse_partition_geometry(lba0_12, device_id, total_sectors)?;
    let partition_types = partitions
        .iter()
        .map(|partition| partition.partition_type)
        .collect::<Vec<_>>();
    let partition_mode = Lba7PartitionMode::from_partition_types(&partition_types);
    let (share_protocol_label, encrypt_protocol_label) = lba0_12
        .get(10 * SECTOR..11 * SECTOR)
        .map(|raw| crate::protocol::semantic::lba10_volume_labels(raw, device_id))
        .unwrap_or((None, None));
    let mut out = MetadataAcquisition::default();

    let topology_json = serde_json::to_vec_pretty(&partitions)
        .map_err(|e| format!("serialize partition topology failed: {e}"))?;
    out.artifacts.push(ArtifactInput {
        id: "derived.partition_topology".into(),
        kind: "partition_topology".into(),
        media_type: "application/json".into(),
        source_extent_ids: vec!["extent.protocol.lba0_12".into()],
        derivation: Some(Derivation {
            method: "lba12_edpf_decode_v1".into(),
            source_artifact_ids: vec!["raw.protocol.lba0_12".into()],
        }),
        restore_policy: RestorePolicy::DerivedOnly,
        completeness: ArtifactCompleteness::Complete,
        data: topology_json,
    });

    for partition in &partitions {
        let region_id = format!(
            "region.partition.{}.type{}",
            partition.index, partition.partition_type
        );
        out.regions.push(Region {
            id: region_id,
            role: format!("partition.type{}", partition.partition_type),
            start_lba: Some(partition.start_sector),
            sector_count: Some(partition.sector_count),
            semantic_status: SemanticStatus::Identified,
        });

        let protocol_label = match partition.partition_type {
            2 => share_protocol_label.clone(),
            4 => encrypt_protocol_label.clone(),
            _ => None,
        };
        out.partitions.push(manifest_partition_from_edp(
            partition_mode,
            partition,
            None,
            protocol_label,
        ));
    }

    match parse_lba7_compatibility_geometry(lba0_12, device_id, total_sectors) {
        Ok(compat) => {
            let region_id = "region.lba7_compatibility_extent";
            out.regions.push(Region {
                id: region_id.into(),
                role: "lba7_legacy_partition_compatibility_extent".into(),
                start_lba: Some(compat.start_lba),
                sector_count: Some(compat.sector_count),
                semantic_status: SemanticStatus::Identified,
            });
            let raw = add_raw_extent(
                &mut out,
                dev,
                region_id,
                "extent.lba7_compatibility".into(),
                "raw.lba7_compatibility".into(),
                compat.start_lba,
                compat.sector_count,
                "lba7_compatibility_extent_ciphertext",
            )?;
            // This extent is not merely forensic: LBA7 points to it as active protocol
            // state, and official provisioning may rewrite it. Once the pointer geometry has
            // passed parse_lba7_compatibility_geometry(), keep the exact ciphertext restorable
            // so a metadata restore can roll the protocol back coherently with LBA0-12.
            if raw.is_some() {
                let artifact = out
                    .artifacts
                    .iter_mut()
                    .find(|artifact| artifact.id == "raw.lba7_compatibility")
                    .expect("captured compatibility artifact must exist");
                artifact.restore_policy = RestorePolicy::Restorable;
            }
            if let Some(expected) = compat.chs_expected_start_lba {
                if expected != compat.start_lba {
                    out.issues.push(CaptureIssue {
                        region_id: region_id.into(),
                        start_lba: compat.start_lba,
                        sector_count: compat.sector_count,
                        error: format!(
                            "LBA7 compatibility extent pointer {} differs from CHS-1792 expected {}; LBA7 pointer preserved as authoritative",
                            compat.start_lba, expected
                        ),
                    });
                }
            }
            if raw.is_some() {
                let layout = serde_json::json!({
                    "total_size": LBA7_COMPAT_EXTENT_BYTES,
                    "source": "lba7_edp_partion_info",
                    "pointer_entries": compat.lba7_pointer_entries,
                    "official_partition_mode": compat.official_partition_mode,
                    "producer_rule": "CreatePartitions preserves PartionType but rewrites legacy entries after entry0 to the same aligned 0xC00 compatibility extent",
                    "wire_semantics": {
                        "offset": 0,
                        "length": LBA7_COMPAT_EXTENT_BYTES,
                        "classification": "fixed_fat16_compatibility_image_encrypted",
                        "plaintext_sha256": "386595e473d3051e07fac43a02e0a8f8134b77858bb12e93246e4ebfbf51ee1c",
                        "crypto": "EDPSECDISK zero8 + physical backing byte-offset tweak"
                    }
                });
                out.artifacts.push(ArtifactInput {
                    id: "derived.lba7_compatibility.layout".into(),
                    kind: "lba7_compatibility_layout".into(),
                    media_type: "application/json".into(),
                    source_extent_ids: vec!["extent.lba7_compatibility".into()],
                    derivation: Some(Derivation {
                        method: "lba7_compatibility_layout_v1".into(),
                        source_artifact_ids: vec!["raw.lba7_compatibility".into()],
                    }),
                    restore_policy: RestorePolicy::DerivedOnly,
                    completeness: ArtifactCompleteness::Complete,
                    data: serde_json::to_vec_pretty(&layout).map_err(|e| {
                        format!("serialize LBA7 compatibility extent layout failed: {e}")
                    })?,
                });
            }
        }
        Err(error) => {
            let fallback_start = (total_sectors / LBA7_COMPAT_CHS_TRACK_SECTORS)
                .checked_mul(LBA7_COMPAT_CHS_TRACK_SECTORS)
                .and_then(|aligned| aligned.checked_sub(LBA7_COMPAT_CHS_BACKOFF_SECTORS))
                .unwrap_or(0);
            out.issues.push(CaptureIssue {
                region_id: "region.lba7_compatibility_extent".into(),
                start_lba: fallback_start,
                sector_count: LBA7_COMPAT_EXTENT_SECTORS,
                error: format!(
                    "LBA7 compatibility extent was not captured because LBA7 pointer validation failed: {error}"
                ),
            });
        }
    }

    if total_sectors >= TAIL_METADATA_MIRROR_OFFSET_SECTORS + TAIL_METADATA_MIRROR_SECTORS {
        let mirror_start = total_sectors - TAIL_METADATA_MIRROR_OFFSET_SECTORS;
        let region_id = "region.tail.metadata_mirror_512k";
        out.regions.push(Region {
            id: region_id.into(),
            role: "lba4_lba12_backup_mirror".into(),
            start_lba: Some(mirror_start),
            sector_count: Some(TAIL_METADATA_MIRROR_SECTORS),
            semantic_status: SemanticStatus::Identified,
        });
        let raw = add_raw_extent(
            &mut out,
            dev,
            region_id,
            "extent.tail.metadata_mirror_512k".into(),
            "raw.tail.metadata_mirror_512k".into(),
            mirror_start,
            TAIL_METADATA_MIRROR_SECTORS,
            "historical_lba4_lba12_mirror",
        )?;
        if raw.is_some() {
            out.artifacts
                .iter_mut()
                .find(|artifact| artifact.id == "raw.tail.metadata_mirror_512k")
                .expect("captured tail metadata mirror artifact")
                .restore_policy = RestorePolicy::Restorable;
        }
    }

    if total_sectors > TAIL_END4_MIRROR_OFFSET_SECTORS {
        let mirror_start = total_sectors - TAIL_END4_MIRROR_OFFSET_SECTORS;
        let region_id = "region.tail.restore_node_end4";
        out.regions.push(Region {
            id: region_id.into(),
            role: "historical_restore_node_mirror".into(),
            start_lba: Some(mirror_start),
            sector_count: Some(1),
            semantic_status: SemanticStatus::Identified,
        });
        let raw = add_raw_extent(
            &mut out,
            dev,
            region_id,
            "extent.tail.restore_node_end4".into(),
            "raw.tail.restore_node_end4".into(),
            mirror_start,
            1,
            "historical_restore_node_mirror",
        )?;
        if raw.is_some() {
            out.artifacts
                .iter_mut()
                .find(|artifact| artifact.id == "raw.tail.restore_node_end4")
                .expect("captured tail restore-node artifact")
                .restore_policy = RestorePolicy::Restorable;
        }
    }

    out.notes.push(
        "EDP metadata-only capture: filesystem boot/FAT/directory/user payload excluded".into(),
    );
    out.notes.push(
        "LBA7 compatibility extent and identified historical tail recovery structures are captured as independent restorable protocol extents"
            .into(),
    );
    if !out.issues.is_empty() {
        let issue_json = serde_json::to_vec_pretty(&out.issues)
            .map_err(|e| format!("serialize metadata capture issues failed: {e}"))?;
        out.artifacts.push(ArtifactInput {
            id: "derived.capture_issues".into(),
            kind: "capture_issues".into(),
            media_type: "application/json".into(),
            source_extent_ids: Vec::new(),
            derivation: None,
            restore_policy: RestorePolicy::DerivedOnly,
            completeness: ArtifactCompleteness::Complete,
            data: issue_json,
        });
        out.notes.push(format!(
            "metadata capture completed with {} unreadable extent(s); no missing bytes were zero-filled",
            out.issues.len()
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod manifest_role_tests {
    use super::{manifest_partition_from_edp, Lba7PartitionMode, PartitionGeometry};

    fn partition(index: usize, partition_type: u32) -> PartitionGeometry {
        PartitionGeometry {
            index,
            partition_type,
            partition_count: 3,
            need_disturb: 0,
            need_encrypt: 0,
            start_sector: 63 + index as u64 * 1024,
            sector_size: 512,
            partition_size: 1024 * 512,
            sector_count: 1024,
            user_key_crc: 0,
            file_key_crc: 0,
            encrypt_mode: 0,
        }
    }

    fn role(mode: Lba7PartitionMode, index: usize, partition_type: u32) -> String {
        manifest_partition_from_edp(Some(mode), &partition(index, partition_type), None, None)
            .role
            .expect("manifest role")
    }

    #[test]
    fn all_official_modes_have_semantic_manifest_roles() {
        assert_eq!(role(Lba7PartitionMode::DefaultThreePartition, 0, 1), "boot");
        assert_eq!(
            role(Lba7PartitionMode::DefaultThreePartition, 1, 2),
            "share"
        );
        assert_eq!(
            role(Lba7PartitionMode::DefaultThreePartition, 2, 4),
            "encrypt"
        );

        assert_eq!(
            role(Lba7PartitionMode::BootShareCombined, 0, 2),
            "boot_share_combined"
        );
        assert_eq!(role(Lba7PartitionMode::BootShareCombined, 1, 4), "encrypt");

        assert_eq!(
            role(Lba7PartitionMode::WholeDiskEncrypted, 0, 1),
            "compatibility_reserve"
        );
        assert_eq!(role(Lba7PartitionMode::WholeDiskEncrypted, 1, 4), "encrypt");

        assert_eq!(
            role(Lba7PartitionMode::IntranetExtranetDualPartition, 0, 1),
            "boot"
        );
        assert_eq!(
            role(Lba7PartitionMode::IntranetExtranetDualPartition, 1, 2),
            "share"
        );
    }
}
