//! Unified read-only evidence source for physical disks and EDPB backups.
//!
//! Consumers receive the same protocol image, device identity, total-sector
//! geometry and sector-reading interface regardless of where the evidence came
//! from. This module never enters a write-capable state.

use std::io;
use std::path::{Path, PathBuf};

use crate::common::{METADATA_IMAGE_LEN, METADATA_SECTOR_COUNT, SECTOR};
use crate::diskio::{self, FileDev};
use crate::edpb::Manifest;
use crate::ports::CmdRunner;

use super::target_session::{ReadOnly, TargetSession};

#[derive(Debug)]
pub enum EvidenceError {
    BackupVerify { path: PathBuf, message: String },
    BackupProtocolRead { path: PathBuf, message: String },
    BackupProtocolLength { actual: usize },
    BackupMissingGeometry { path: PathBuf },
    Target(String),
    DiskMissingGeometry { disk: u32 },
    DiskOpen { disk: u32, message: String },
    DiskProtocolRead { disk: u32, message: String },
}

impl std::fmt::Display for EvidenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BackupVerify { path, message } => {
                write!(formatter, "EDPB 校验失败 {}: {message}", path.display())
            }
            Self::BackupProtocolRead { path, message } => {
                write!(
                    formatter,
                    "读取 EDPB LBA0-12 失败 {}: {message}",
                    path.display()
                )
            }
            Self::BackupProtocolLength { actual } => write!(
                formatter,
                "EDPB LBA0-12 大小 {actual}B，预期 {METADATA_IMAGE_LEN}B"
            ),
            Self::BackupMissingGeometry { path } => write!(
                formatter,
                "EDPB 缺少 total_sectors，无法校验任意 LBA: {}",
                path.display()
            ),
            Self::Target(message) => formatter.write_str(message),
            Self::DiskMissingGeometry { disk } => {
                write!(formatter, "无法取得 disk{disk} 设备总扇区数")
            }
            Self::DiskOpen { disk, message } => {
                write!(formatter, "无法只读打开 disk{disk}: {message}")
            }
            Self::DiskProtocolRead { disk, message } => {
                write!(
                    formatter,
                    "读取 disk{disk} 协议上下文 LBA0-12 失败: {message}"
                )
            }
        }
    }
}

impl std::error::Error for EvidenceError {}

pub trait SectorReader {
    fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>>;

    /// Keep the legacy 512B protocol projection as the existing contract.
    fn logical_sector_bytes(&self) -> u32 {
        SECTOR as u32
    }

    /// Full device-native block; defaults to 512B for existing/backup readers.
    fn read_native_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        self.read_sector(lba)
    }

    fn read_range(&mut self, start_lba: u64, sector_count: usize) -> io::Result<Vec<u8>> {
        let capacity = sector_count
            .checked_mul(SECTOR)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "扇区范围字节长度溢出"))?;
        let mut out = Vec::with_capacity(capacity);
        for index in 0..sector_count {
            let lba = start_lba
                .checked_add(index as u64)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "LBA 范围溢出"))?;
            let sector = self.read_sector(lba)?;
            if sector.len() != SECTOR {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("LBA{lba} 返回 {}B，预期 {SECTOR}B", sector.len()),
                ));
            }
            out.extend_from_slice(&sector);
        }
        Ok(out)
    }
    /// Read a bounded sequence of complete *native* logical sectors.
    ///
    /// Unlike read_range(), whose legacy contract is always 512B protocol
    /// projections, this uses the reader's independently observed block
    /// width and never silently slices a 4Kn block. The width check is a
    /// pure reader contract: it does not authorize physical 1024/2048/8192B
    /// EDP provisioning, or any write operation.
    fn read_native_range(&mut self, start_lba: u64, sector_count: usize) -> io::Result<Vec<u8>> {
        let sector_bytes = self.logical_sector_bytes();
        if !(512..=65_536).contains(&sector_bytes) || !sector_bytes.is_power_of_two() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("不支持的来源原生扇区长度 {sector_bytes}B"),
            ));
        }
        let sector_bytes = sector_bytes as usize;
        let bytes = sector_count
            .checked_mul(sector_bytes)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "原生读取总字节溢出"))?;
        // A diagnostic/metadata read may never reserve unbounded memory.
        if bytes > 8 * 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "单次原生读取不得超过8MiB",
            ));
        }
        let count = u64::try_from(sector_count)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "原生扇区数量溢出"))?;
        start_lba
            .checked_add(count)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "原生LBA范围溢出"))?;
        let mut result = Vec::with_capacity(bytes);
        for index in 0..count {
            let lba = start_lba + index; // covered by the checked end above
            let native = self.read_native_sector(lba)?;
            if native.len() != sector_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!(
                        "LBA{lba} 返回 {}B，预期完整原生扇区 {sector_bytes}B",
                        native.len()
                    ),
                ));
            }
            result.extend_from_slice(&native);
        }
        Ok(result)
    }
}

impl SectorReader for FileDev {
    fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        self.read_sector_u64(lba)
    }

    fn logical_sector_bytes(&self) -> u32 {
        FileDev::logical_sector_bytes(self)
    }

    fn read_native_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        if self.logical_sector_bytes() == SECTOR as u32 {
            self.read_sector_u64(lba)
        } else {
            self.read_native_sector_u64(lba)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceIdentity {
    pub device_id: Option<String>,
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub size_bytes: Option<u64>,
    pub onlyid: Option<String>,
    pub provision_kind: Option<crate::provision::DiskProvisionKind>,
}

struct BackupSectorReader {
    snapshot: crate::edpb::VerifiedBackupReader,
    protocol: Vec<u8>,
    has_full_protocol: bool,
    native_protocol: Option<crate::protocol::image::NativeProtocolImage>,
}

impl BackupSectorReader {
    fn manifest(&self) -> &Manifest {
        &self.snapshot.verified().manifest
    }

    fn has_sector(&self, lba: u64) -> bool {
        if self.has_full_protocol && lba < METADATA_SECTOR_COUNT as u64 {
            return true;
        }
        self.manifest().extents.iter().any(|extent| {
            let Some(end) = extent.start_lba.checked_add(extent.sector_count) else {
                return false;
            };
            lba >= extent.start_lba
                && lba < end
                && self.manifest().artifacts.iter().any(|artifact| {
                    artifact.kind == "raw_sectors"
                        && artifact.source_extent_ids.iter().any(|id| id == &extent.id)
                })
        })
    }

    fn read_artifact(&self, artifact_id: &str) -> io::Result<Option<&[u8]>> {
        if !self
            .manifest()
            .artifacts
            .iter()
            .any(|artifact| artifact.id == artifact_id)
        {
            return Ok(None);
        }
        self.snapshot
            .read_artifact(artifact_id)
            .map(Some)
            .map_err(io::Error::other)
    }
}

impl SectorReader for BackupSectorReader {
    fn logical_sector_bytes(&self) -> u32 {
        self.manifest().geometry.logical_sector_size
    }

    fn read_native_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        if let Some(image) = &self.native_protocol {
            if let Some(block) = usize::try_from(lba)
                .ok()
                .and_then(|index| image.block(index))
            {
                return Ok(block.to_vec());
            }
        }
        let size = usize::try_from(self.logical_sector_bytes())
            .map_err(|_| io::Error::other("EDPB native block size overflow"))?;
        let located = self
            .manifest()
            .extents
            .iter()
            .find_map(|extent| {
                let end = extent.start_lba.checked_add(extent.sector_count)?;
                if lba < extent.start_lba || lba >= end {
                    return None;
                }
                let artifact = self.manifest().artifacts.iter().find(|artifact| {
                    artifact.kind == "raw_sectors"
                        && artifact.source_extent_ids.iter().any(|id| id == &extent.id)
                })?;
                Some((extent.start_lba, artifact.id.clone()))
            })
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "EDPB 未采集该原生 LBA"))?;
        let bytes = self
            .read_artifact(&located.1)?
            .ok_or_else(|| io::Error::other("EDPB artifact 缺失"))?;
        let start = usize::try_from(lba - located.0)
            .ok()
            .and_then(|offset| offset.checked_mul(size))
            .ok_or_else(|| io::Error::other("EDPB 原生 LBA 偏移溢出"))?;
        let end = start
            .checked_add(size)
            .ok_or_else(|| io::Error::other("EDPB 原生块结束偏移溢出"))?;
        bytes
            .get(start..end)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "EDPB 原生块截断"))
    }

    fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        if self.native_protocol.is_some() {
            let block = self.read_native_sector(lba)?;
            return Ok(block[..SECTOR].to_vec());
        }
        if self.has_full_protocol && lba < METADATA_SECTOR_COUNT as u64 {
            let start = usize::try_from(lba)
                .ok()
                .and_then(|sector| sector.checked_mul(SECTOR))
                .ok_or_else(|| io::Error::other("EDPB 协议扇区偏移溢出"))?;
            return self
                .protocol
                .get(start..start + SECTOR)
                .map(|sector| sector.to_vec())
                .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "EDPB 协议镜像截断"));
        }
        let found = self.manifest().extents.iter().find_map(|extent| {
            let end = extent.start_lba.checked_add(extent.sector_count)?;
            if lba < extent.start_lba || lba >= end {
                return None;
            }
            let artifact = self.manifest().artifacts.iter().find(|artifact| {
                artifact.kind == "raw_sectors"
                    && artifact.source_extent_ids.iter().any(|id| id == &extent.id)
            })?;
            Some((extent.start_lba, artifact.id.clone()))
        });
        let Some((start_lba, artifact_id)) = found else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("EDPB 未采集 LBA{lba} 的原始扇区"),
            ));
        };
        let data = self
            .read_artifact(&artifact_id)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "EDPB Artifact 缺失"))?;
        let offset = usize::try_from(lba - start_lba)
            .ok()
            .and_then(|sector| sector.checked_mul(SECTOR))
            .ok_or_else(|| io::Error::other("EDPB Artifact 扇区偏移溢出"))?;
        data.get(offset..offset + SECTOR)
            .map(|sector| sector.to_vec())
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "EDPB Artifact 截断"))
    }
}

enum EvidenceReader {
    Disk(FileDev),
    Backup(Box<BackupSectorReader>),
}

pub struct EvidenceSource {
    source_label: String,
    total_sectors: u64,
    protocol: Vec<u8>,
    /// Full native LBA0..12 retained for 4Kn physical sources; never log raw tails.
    native_protocol: Option<crate::protocol::image::NativeProtocolImage>,
    identity: EvidenceIdentity,
    reader: EvidenceReader,
}

fn plain_protocol_context(
    path: &Path,
    reader: &crate::edpb::VerifiedBackupReader,
) -> Result<Vec<u8>, EvidenceError> {
    let manifest = &reader.verified().manifest;
    let mut protocol = vec![0u8; METADATA_IMAGE_LEN];
    for artifact in manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == "raw_sectors")
    {
        let touches_prefix = artifact.source_extent_ids.iter().any(|extent_id| {
            manifest
                .extents
                .iter()
                .find(|extent| &extent.id == extent_id)
                .is_some_and(|extent| extent.start_lba < METADATA_SECTOR_COUNT as u64)
        });
        if !touches_prefix {
            continue;
        }
        let data = reader.read_artifact(&artifact.id).map_err(|message| {
            EvidenceError::BackupProtocolRead {
                path: path.to_path_buf(),
                message,
            }
        })?;
        for extent_id in &artifact.source_extent_ids {
            let Some(extent) = manifest
                .extents
                .iter()
                .find(|extent| &extent.id == extent_id)
            else {
                continue;
            };
            if extent.start_lba >= METADATA_SECTOR_COUNT as u64 {
                continue;
            }
            let copy_sectors = extent
                .sector_count
                .min(METADATA_SECTOR_COUNT as u64 - extent.start_lba);
            for offset in 0..copy_sectors {
                let src = usize::try_from(offset)
                    .ok()
                    .and_then(|sector| sector.checked_mul(SECTOR))
                    .ok_or(EvidenceError::BackupProtocolLength { actual: data.len() })?;
                let dst_lba = extent.start_lba + offset;
                let dst = usize::try_from(dst_lba)
                    .ok()
                    .and_then(|sector| sector.checked_mul(SECTOR))
                    .ok_or(EvidenceError::BackupProtocolLength { actual: data.len() })?;
                let Some(source) = data.get(src..src + SECTOR) else {
                    return Err(EvidenceError::BackupProtocolLength { actual: data.len() });
                };
                protocol[dst..dst + SECTOR].copy_from_slice(source);
            }
        }
    }
    Ok(protocol)
}

impl EvidenceSource {
    pub fn backup_manifest(&self) -> Option<&Manifest> {
        match &self.reader {
            EvidenceReader::Backup(reader) => Some(reader.manifest()),
            EvidenceReader::Disk(_) => None,
        }
    }

    pub fn available_requested_lbas(&self, requested: &[u64]) -> Vec<u64> {
        match &self.reader {
            EvidenceReader::Disk(_) => requested.to_vec(),
            EvidenceReader::Backup(reader) => requested
                .iter()
                .copied()
                .filter(|lba| reader.has_sector(*lba))
                .collect(),
        }
    }

    pub fn open_backup(path: &Path) -> Result<Self, EvidenceError> {
        let snapshot = crate::edpb::VerifiedBackupReader::open(path).map_err(|error| {
            EvidenceError::BackupVerify {
                path: path.to_path_buf(),
                message: error.to_string(),
            }
        })?;
        let verified = snapshot.verified();
        let canonical =
            crate::edpb::canonical_media_identity(&verified.manifest).map_err(|message| {
                EvidenceError::BackupVerify {
                    path: path.to_path_buf(),
                    message,
                }
            })?;
        let current_plain_v3 = verified.manifest.schema == "edpb.manifest.v3"
            && verified.manifest.backup_purpose == crate::edpb::BackupPurpose::MetadataOnly
            && verified.manifest.snapshot.capture_level == crate::edpb::CaptureLevel::Metadata
            && canonical.protocol.provision_kind
                == Some(crate::provision::DiskProvisionKind::Plain);
        let has_full_protocol = verified
            .manifest
            .artifacts
            .iter()
            .any(|artifact| artifact.id == crate::edpb::RAW_PROTOCOL_ARTIFACT_ID);
        if current_plain_v3 && has_full_protocol {
            return Err(EvidenceError::BackupVerify {
                path: path.to_path_buf(),
                message: "Plain v3 metadata-only 不应包含固定 LBA0-12 protocol core".into(),
            });
        }
        let native_evidence = verified.manifest.schema == "edpb.manifest.v4";
        let native_protocol = if native_evidence {
            Some(
                crate::protocol::image::NativeProtocolImage::from_native_bytes(
                    verified.manifest.geometry.logical_sector_size,
                    snapshot
                        .read_raw_protocol()
                        .map_err(|error| EvidenceError::BackupProtocolRead {
                            path: path.to_path_buf(),
                            message: error.to_string(),
                        })?
                        .to_vec(),
                )
                .map_err(|error| EvidenceError::BackupProtocolRead {
                    path: path.to_path_buf(),
                    message: error.to_string(),
                })?,
            )
        } else {
            None
        };
        let protocol = if let Some(native) = &native_protocol {
            native.protocol_projection().to_vec()
        } else if has_full_protocol {
            snapshot
                .read_raw_protocol()
                .map(<[u8]>::to_vec)
                .map_err(|error| EvidenceError::BackupProtocolRead {
                    path: path.to_path_buf(),
                    message: error.to_string(),
                })?
        } else if current_plain_v3 {
            plain_protocol_context(path, &snapshot)?
        } else {
            return Err(EvidenceError::BackupProtocolRead {
                path: path.to_path_buf(),
                message: "EDPB 缺少可读的 LBA0-12 原始 Artifact".into(),
            });
        };
        if protocol.len() != METADATA_IMAGE_LEN {
            return Err(EvidenceError::BackupProtocolLength {
                actual: protocol.len(),
            });
        }
        let manifest = &verified.manifest;
        let total_sectors = manifest.geometry.total_sectors.ok_or_else(|| {
            EvidenceError::BackupMissingGeometry {
                path: path.to_path_buf(),
            }
        })?;
        let legacy_nonplain = !manifest.snapshot.device_state.eq_ignore_ascii_case("plain");
        let effective_device_id = canonical
            .protocol
            .device_id
            .clone()
            .or_else(|| legacy_nonplain.then(|| manifest.device.device_id.clone()));
        let provision_kind = canonical
            .protocol
            .provision_kind
            .or_else(|| {
                effective_device_id.as_deref().and_then(|device_id| {
                    if native_evidence {
                        crate::provision::DiskProvisionKind::from_sectors_with_logical_size(
                            protocol.get(7 * SECTOR..8 * SECTOR)?,
                            protocol.get(12 * SECTOR..13 * SECTOR)?,
                            device_id,
                            4096,
                        )
                    } else {
                        crate::provision::DiskProvisionKind::from_metadata(&protocol, device_id)
                    }
                })
            })
            .or_else(|| {
                (!legacy_nonplain
                    && crate::partition_table::confirmed_plain_protocol_prefix(
                        &protocol,
                        total_sectors,
                    ))
                .then_some(crate::provision::DiskProvisionKind::Plain)
            });
        let identity = EvidenceIdentity {
            device_id: effective_device_id,
            vid: canonical.hardware.vid.map(|value| format!("{value:04x}")),
            pid: canonical.hardware.pid.map(|value| format!("{value:04x}")),
            size_bytes: manifest.geometry.capacity_bytes,
            onlyid: canonical.protocol.onlyid.clone().or_else(|| {
                legacy_nonplain
                    .then(|| manifest.device.onlyid.clone())
                    .flatten()
            }),
            provision_kind,
        };
        Ok(Self {
            source_label: path.display().to_string(),
            total_sectors,
            protocol: protocol.clone(),
            native_protocol: native_protocol.clone(),
            identity,
            reader: EvidenceReader::Backup(Box::new(BackupSectorReader {
                snapshot,
                protocol,
                has_full_protocol,
                native_protocol,
            })),
        })
    }

    pub fn open_disk(runner: &dyn CmdRunner, disk: u32) -> Result<Self, EvidenceError> {
        let target = TargetSession::<ReadOnly>::open_usb(runner, disk)
            .map_err(|error| EvidenceError::Target(error.msg))?;
        let native_geometry = crate::platform::system::device_geometry(runner, disk)
            .and_then(|observed| observed.native_read_geometry().ok());
        let total_sectors = native_geometry
            .map(|geometry| geometry.native_sector_count)
            .or_else(|| target.total_sectors())
            .ok_or(EvidenceError::DiskMissingGeometry { disk })?;
        let path = diskio::raw_path(disk);
        let mut dev = match native_geometry {
            Some(geometry) if geometry.logical_sector_bytes > 512 => {
                FileDev::open_rdonly_native(&path, geometry)
            }
            _ => FileDev::open_rdonly(&path),
        }
        .map_err(|error| EvidenceError::DiskOpen {
            disk,
            message: error.to_string(),
        })?;
        let (protocol, native_protocol) = if let Some(geometry) =
            native_geometry.filter(|geometry| geometry.logical_sector_bytes > 512)
        {
            // Read the complete native envelope before producing the stable
            // 6656B official protocol projection. Never discard unknown tails
            // in a physical read; future EDPB captures consume the same type.
            let native_bytes = SectorReader::read_native_range(&mut dev, 0, METADATA_SECTOR_COUNT)
                .map_err(|error| EvidenceError::DiskProtocolRead {
                    disk,
                    message: error.to_string(),
                })?;
            let image = crate::protocol::image::NativeProtocolImage::from_native_bytes(
                geometry.logical_sector_bytes,
                native_bytes,
            )
            .map_err(|error| EvidenceError::DiskProtocolRead {
                disk,
                message: error.to_string(),
            })?;
            (image.protocol_projection().to_vec(), Some(image))
        } else {
            (
                dev.read_range(0, METADATA_SECTOR_COUNT).map_err(|error| {
                    EvidenceError::DiskProtocolRead {
                        disk,
                        message: error.to_string(),
                    }
                })?,
                None,
            )
        };
        debug_assert_eq!(protocol.len(), METADATA_IMAGE_LEN);

        let canonical =
            crate::application::media_identity_observer::media_identity_from_protocol_image(
                runner, disk, &protocol,
            )
            .map_err(|error| EvidenceError::Target(error.msg))?;
        let canonical = crate::application::media_identity_observer::apply_runtime_plain_override(
            canonical,
            &protocol,
            total_sectors,
            |lba| SectorReader::read_sector(&mut dev, lba).map_err(|error| error.to_string()),
        );
        let identity = EvidenceIdentity {
            device_id: canonical.protocol.device_id.clone(),
            vid: canonical.hardware.vid.map(|value| format!("{value:04x}")),
            pid: canonical.hardware.pid.map(|value| format!("{value:04x}")),
            size_bytes: native_geometry
                .map(|geometry| geometry.capacity_bytes)
                .or_else(|| total_sectors.checked_mul(SECTOR as u64)),
            onlyid: canonical.protocol.onlyid.clone(),
            provision_kind: canonical.protocol.provision_kind,
        };
        Ok(Self {
            source_label: format!("物理盘 disk{disk} ({path})"),
            total_sectors,
            protocol,
            native_protocol,
            identity,
            reader: EvidenceReader::Disk(dev),
        })
    }

    pub fn source_label(&self) -> &str {
        &self.source_label
    }

    pub fn total_sectors(&self) -> u64 {
        self.total_sectors
    }

    pub fn protocol(&self) -> &[u8] {
        &self.protocol
    }

    /// Full native blocks, when the physical source provides >512B logical sectors.
    pub fn native_protocol_image(&self) -> Option<&crate::protocol::image::NativeProtocolImage> {
        self.native_protocol.as_ref()
    }

    pub fn identity(&self) -> &EvidenceIdentity {
        &self.identity
    }

    /// UI-neutral entry point for verified native source replay from a disk
    /// or EDPB backup. Uses this source's observed identity and the original
    /// snapshot; callers cannot override the identity or provide LCE bytes
    /// from a different reader.
    ///
    /// This API never obtains a writable handle. It does not enable native
    /// provisioning or certify the 4Kn LCE's cryptographic content.
    pub fn verified_native_replay(
        &mut self,
        plan: &crate::provision::NativeEdpLayoutPlan,
    ) -> Result<Vec<crate::filesystem::NativeFilesystemWrite>, String> {
        let device_id = self
            .identity
            .device_id
            .as_deref()
            .ok_or("只读来源未确认device_id")?
            .to_owned();
        let snapshot = match &self.native_protocol {
            Some(image) => image.clone(),
            None if self.logical_sector_bytes() == SECTOR as u32 => {
                crate::protocol::image::NativeProtocolImage::from_native_bytes(
                    SECTOR as u32,
                    self.protocol.clone(),
                )
                .map_err(|error| format!("来源协议快照构造失败: {error}"))?
            }
            None => return Err("原生来源缺少完整协议扇区快照".into()),
        };
        let source_total_sectors = self.total_sectors;
        verified_native_source_replay(self, plan, &snapshot, &device_id, source_total_sectors)
    }

    pub fn read_artifact(&mut self, artifact_id: &str) -> io::Result<Option<Vec<u8>>> {
        match &mut self.reader {
            EvidenceReader::Disk(_) => Ok(None),
            EvidenceReader::Backup(reader) => reader
                .read_artifact(artifact_id)
                .map(|data| data.map(<[u8]>::to_vec)),
        }
    }
}

/// A read-only prewrite evidence result. This is deliberately NOT a disk
/// write lease, backup restore capability, or authorization token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeBackupDiskMatch {
    pub disk: u32,
    pub logical_sector_bytes: u32,
    pub total_sectors: u64,
    pub verified_native_blocks: usize,
}

fn compare_native_backup_blocks(
    sectors: impl IntoIterator<Item = u64>,
    native_sector_bytes: usize,
    mut backup_read: impl FnMut(u64) -> io::Result<Vec<u8>>,
    mut disk_read: impl FnMut(u64) -> io::Result<Vec<u8>>,
) -> Result<usize, String> {
    let mut count = 0usize;
    for lba in sectors {
        let backed =
            backup_read(lba).map_err(|e| format!("EDPB LBA{lba} 原生证据读取失败: {e}"))?;
        let live = disk_read(lba).map_err(|e| format!("目标介质 LBA{lba} 原生读取失败: {e}"))?;
        if backed.len() != native_sector_bytes || live.len() != native_sector_bytes {
            return Err(format!(
                "LBA{lba} 备份或实盘不是完整原生{native_sector_bytes}B块"
            ));
        }
        if backed != live {
            return Err(format!("LBA{lba} 实盘已偏离原生EDPB备份，拒绝后续提交"));
        }
        count += 1;
    }
    Ok(count)
}

/// Verify a native EDPB v4 snapshot against the same physical device using
/// only read handles, with *no* permission to restore or provision.
///
/// LBA0–12, LCE and every raw extent present in the verified EDPB are checked
/// byte-for-byte using their original 1024/2048/4096B native geometry. This
/// only proves that the current media still matches the captured evidence:
/// it never upgrades evidence-only artifacts to restorable artifacts.
pub fn verify_native_backup_against_disk_readonly(
    runner: &dyn CmdRunner,
    disk: u32,
    backup_path: &Path,
) -> Result<NativeBackupDiskMatch, String> {
    use std::collections::BTreeSet;

    let mut backup = EvidenceSource::open_backup(backup_path).map_err(|e| e.to_string())?;
    let manifest = backup
        .backup_manifest()
        .ok_or("来源必须为完整可校验的EDPB备份")?
        .clone();
    let sector = manifest.geometry.logical_sector_size;
    if manifest.schema != "edpb.manifest.v4"
        || !matches!(sector, 1024 | 2048 | 4096)
        || manifest.geometry.total_sectors.is_none()
        || manifest.geometry.capacity_bytes
            != manifest
                .geometry
                .total_sectors
                .and_then(|n| n.checked_mul(u64::from(sector)))
    {
        return Err("原生写前校验要求1024/2048/4096B完整几何的EDPB v4备份".into());
    }
    let mut live = EvidenceSource::open_disk(runner, disk).map_err(|e| e.to_string())?;
    if backup.total_sectors() != live.total_sectors()
        || live.logical_sector_bytes() != sector
        || backup.native_protocol_image().is_none()
        || live.native_protocol_image().is_none()
        || backup.identity() != live.identity()
        || live.identity().size_bytes != manifest.geometry.capacity_bytes
    {
        return Err("实盘与EDPB设备身份、扇区规格或容量不一致".into());
    }

    // Enumerate only ranges already backed by verified *raw_sectors*
    // artifacts. Never infer data from holes or unowned partition bytes.
    let mut lbas = BTreeSet::new();
    let mut lce_present = false;
    for artifact in manifest
        .artifacts
        .iter()
        .filter(|a| a.kind == "raw_sectors")
    {
        for id in &artifact.source_extent_ids {
            let extent = manifest
                .extents
                .iter()
                .find(|e| &e.id == id)
                .ok_or("EDPB 原生原始范围引用不存在")?;
            lce_present |= extent.id == "extent.lba7_compatibility";
            for delta in 0..extent.sector_count {
                if lbas.len() >= 512 {
                    return Err("原生EDPB预检超过512块上限".into());
                }
                let lba = extent
                    .start_lba
                    .checked_add(delta)
                    .filter(|lba| *lba < live.total_sectors())
                    .ok_or("EDPB 原生证据范围超过实盘容量")?;
                lbas.insert(lba);
            }
        }
    }
    if !lce_present || !(0..13).all(|lba| lbas.contains(&lba)) {
        return Err("EDPB 缺少完整LBA0-12原生协议或完整LCE".into());
    }
    let count = compare_native_backup_blocks(
        lbas,
        sector as usize,
        |lba| SectorReader::read_native_sector(&mut backup, lba),
        |lba| SectorReader::read_native_sector(&mut live, lba),
    )?;
    Ok(NativeBackupDiskMatch {
        disk,
        logical_sector_bytes: sector,
        total_sectors: live.total_sectors(),
        verified_native_blocks: count,
    })
}

/// Read a source's entire native EDP protocol and LCE through the same
/// read-only sector abstraction, with an independently captured protocol
/// snapshot as a second source-consistency check. This authenticates the
/// *consistency* of reads against the provided evidence, not the hardware
/// identity or cryptographic content of LCE.
pub fn verified_native_source_replay<R: SectorReader + ?Sized>(
    reader: &mut R,
    plan: &crate::provision::NativeEdpLayoutPlan,
    source_protocol: &crate::protocol::image::NativeProtocolImage,
    device_id: &str,
    source_total_sectors: u64,
) -> Result<Vec<crate::filesystem::NativeFilesystemWrite>, String> {
    if !crate::domain::hardware::native_sector_capability(reader.logical_sector_bytes())
        .is_some_and(|capability| capability.fat_exfat_format)
        || reader.logical_sector_bytes() != plan.logical_sector_bytes
        || source_protocol.logical_sector_bytes() != plan.logical_sector_bytes
        || source_total_sectors != plan.total_sectors
    {
        return Err("来源与重放计划的原生扇区大小或容量不一致".into());
    }
    if device_id.is_empty() {
        return Err("来源缺少设备身份，拒绝生成重放计划".into());
    }
    plan.verified_source_replay_from_reader(device_id, |lba| {
        let block = reader
            .read_native_sector(lba)
            .map_err(|error| format!("来源原生LBA{lba}只读失败: {error}"))?;
        if lba < METADATA_SECTOR_COUNT as u64
            && source_protocol.block(lba as usize) != Some(block.as_slice())
        {
            return Err(format!("来源协议LBA{lba}与已采集原生快照不一致"));
        }
        Ok(block)
    })
}

impl SectorReader for EvidenceSource {
    fn logical_sector_bytes(&self) -> u32 {
        match &self.reader {
            EvidenceReader::Disk(reader) => reader.logical_sector_bytes(),
            EvidenceReader::Backup(reader) => reader.logical_sector_bytes(),
        }
    }

    fn read_native_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        match &mut self.reader {
            EvidenceReader::Disk(reader) => SectorReader::read_native_sector(reader, lba),
            EvidenceReader::Backup(reader) => reader.read_native_sector(lba),
        }
    }

    fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        match &mut self.reader {
            EvidenceReader::Disk(reader) => SectorReader::read_sector(reader, lba),
            EvidenceReader::Backup(reader) => reader.read_sector(lba),
        }
    }
}

#[cfg(test)]
mod native_backup_match_tests {
    use super::*;

    #[test]
    fn native_backup_comparator_covers_each_sector_width_and_unknown_tail() {
        for sector in [1024usize, 2048, 4096] {
            let lbas = [0, 1, 12, 100, 101, 102];
            let fresh = |lba: u64| -> io::Result<Vec<u8>> {
                let mut buffer = vec![lba as u8; sector];
                buffer[sector - 1] = (lba as u8).wrapping_add(1);
                Ok(buffer)
            };
            assert_eq!(
                compare_native_backup_blocks(lbas, sector, fresh, fresh),
                Ok(6)
            );
            let mutated = |lba: u64| -> io::Result<Vec<u8>> {
                let mut buffer = fresh(lba)?;
                if lba == 102 {
                    buffer[sector - 1] ^= 1;
                }
                Ok(buffer)
            };
            assert!(compare_native_backup_blocks(lbas, sector, fresh, mutated)
                .unwrap_err()
                .contains("LBA102"));
            let truncated = |_: u64| -> io::Result<Vec<u8>> { Ok(vec![0; 512]) };
            assert!(compare_native_backup_blocks(lbas, sector, fresh, truncated)
                .unwrap_err()
                .contains(&format!("原生{sector}B")));
        }
    }

    #[test]
    fn independent_4096b_backup_vs_disk_rejects_tamper_and_truncated_blocks() {
        let lbas = [0, 1, 12, 62_476_561];
        let read = |lba: u64| -> io::Result<Vec<u8>> { Ok(vec![(lba & 0xff) as u8; 4096]) };
        assert_eq!(compare_native_backup_blocks(lbas, 4096, read, read), Ok(4));
        let corrupted = |lba: u64| -> io::Result<Vec<u8>> {
            let mut block = vec![(lba & 0xff) as u8; 4096];
            if lba == 62_476_561 {
                block[4095] ^= 0x80;
            }
            Ok(block)
        };
        assert!(compare_native_backup_blocks(lbas, 4096, read, corrupted)
            .unwrap_err()
            .contains("LBA62476561"));
        let short = |lba: u64| -> io::Result<Vec<u8>> { Ok(vec![(lba & 0xff) as u8; 512]) };
        assert!(compare_native_backup_blocks(lbas, 4096, short, read)
            .unwrap_err()
            .contains("完整原生4096B"));
    }
}
