//! Unified read-only evidence source for physical disks and EDPB backups.
//!
//! Consumers receive the same protocol image, device identity, total-sector
//! geometry and sector-reading interface regardless of where the evidence came
//! from. This module never enters a write-capable state.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::common::{METADATA_IMAGE_LEN, METADATA_SECTOR_COUNT, SECTOR};
use crate::diskio::{self, FileDev};
use crate::edpb::Manifest;
use crate::identify::identify;
use crate::sysinfo::{self, CmdRunner};

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
                write!(formatter, "读取 EDPB LBA0-12 失败 {}: {message}", path.display())
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
                write!(formatter, "读取 disk{disk} 协议上下文 LBA0-12 失败: {message}")
            }
        }
    }
}

impl std::error::Error for EvidenceError {}

pub trait SectorReader {
    fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>>;

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
}

impl SectorReader for FileDev {
    fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        self.read_sector_u64(lba)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceIdentity {
    pub device_id: Option<String>,
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub size_bytes: Option<u64>,
    pub onlyid: Option<String>,
}

struct BackupSectorReader {
    path: PathBuf,
    manifest: Manifest,
    protocol: Vec<u8>,
    cache: BTreeMap<String, Vec<u8>>,
}

impl BackupSectorReader {
    fn read_artifact(&mut self, artifact_id: &str) -> io::Result<Option<Vec<u8>>> {
        if !self
            .manifest
            .artifacts
            .iter()
            .any(|artifact| artifact.id == artifact_id)
        {
            return Ok(None);
        }
        if !self.cache.contains_key(artifact_id) {
            let data =
                crate::edpb::read_artifact(&self.path, artifact_id).map_err(io::Error::other)?;
            self.cache.insert(artifact_id.to_string(), data);
        }
        Ok(self.cache.get(artifact_id).cloned())
    }
}

impl SectorReader for BackupSectorReader {
    fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        if lba < METADATA_SECTOR_COUNT as u64 {
            let start = usize::try_from(lba).unwrap() * SECTOR;
            return Ok(self.protocol[start..start + SECTOR].to_vec());
        }
        let found = self.manifest.extents.iter().find_map(|extent| {
            let end = extent.start_lba.checked_add(extent.sector_count)?;
            if lba < extent.start_lba || lba >= end {
                return None;
            }
            let artifact = self.manifest.artifacts.iter().find(|artifact| {
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
            .expect("manifest 中已确认的 Artifact 必须存在");
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
    identity: EvidenceIdentity,
    reader: EvidenceReader,
}

impl EvidenceSource {
    pub fn open_backup(path: &Path) -> Result<Self, EvidenceError> {
        let verified = crate::edpb::verify_file(path).map_err(|error| EvidenceError::BackupVerify {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        let protocol = crate::edpb::read_raw_protocol(path).map_err(|error| {
            EvidenceError::BackupProtocolRead {
                path: path.to_path_buf(),
                message: error.to_string(),
            }
        })?;
        if protocol.len() != METADATA_IMAGE_LEN {
            return Err(EvidenceError::BackupProtocolLength {
                actual: protocol.len(),
            });
        }
        let manifest = verified.manifest;
        let total_sectors = manifest
            .geometry
            .total_sectors
            .ok_or_else(|| EvidenceError::BackupMissingGeometry {
                path: path.to_path_buf(),
            })?;
        let identity = EvidenceIdentity {
            device_id: Some(manifest.device.device_id.clone()),
            vid: Some(manifest.device.vid.clone()),
            pid: Some(manifest.device.pid.clone()),
            size_bytes: manifest.geometry.capacity_bytes,
            onlyid: manifest.device.onlyid.clone(),
        };
        Ok(Self {
            source_label: path.display().to_string(),
            total_sectors,
            protocol: protocol.clone(),
            identity,
            reader: EvidenceReader::Backup(Box::new(BackupSectorReader {
                path: path.to_path_buf(),
                manifest,
                protocol,
                cache: BTreeMap::new(),
            })),
        })
    }

    pub fn open_disk(runner: &dyn CmdRunner, disk: u32) -> Result<Self, EvidenceError> {
        let target = TargetSession::<ReadOnly>::open_usb(runner, disk)
            .map_err(|error| EvidenceError::Target(error.msg))?;
        let total_sectors = target
            .total_sectors()
            .ok_or(EvidenceError::DiskMissingGeometry { disk })?;
        let path = diskio::raw_path(disk);
        let mut dev = FileDev::open_rdonly(&path).map_err(|error| EvidenceError::DiskOpen {
            disk,
            message: error.to_string(),
        })?;
        let protocol = dev
            .read_range(0, METADATA_SECTOR_COUNT)
            .map_err(|error| EvidenceError::DiskProtocolRead {
                disk,
                message: error.to_string(),
            })?;
        debug_assert_eq!(protocol.len(), METADATA_IMAGE_LEN);

        let raw7 = &protocol[7 * SECTOR..8 * SECTOR];
        let id = identify(runner, disk, raw7).device_id;
        let (vid, pid) = sysinfo::usb_vid_pid(runner, disk);
        let identity = EvidenceIdentity {
            device_id: id,
            vid: (vid != "xxxx").then_some(vid),
            pid: (pid != "xxxx").then_some(pid),
            size_bytes: total_sectors.checked_mul(SECTOR as u64),
            onlyid: diskio::lba4_label_id_from(&protocol[4 * SECTOR..5 * SECTOR]),
        };
        Ok(Self {
            source_label: format!("物理盘 disk{disk} ({path})"),
            total_sectors,
            protocol,
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

    pub fn identity(&self) -> &EvidenceIdentity {
        &self.identity
    }

    pub fn read_artifact(&mut self, artifact_id: &str) -> io::Result<Option<Vec<u8>>> {
        match &mut self.reader {
            EvidenceReader::Disk(_) => Ok(None),
            EvidenceReader::Backup(reader) => reader.read_artifact(artifact_id),
        }
    }
}

impl SectorReader for EvidenceSource {
    fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        match &mut self.reader {
            EvidenceReader::Disk(reader) => SectorReader::read_sector(reader, lba),
            EvidenceReader::Backup(reader) => reader.read_sector(lba),
        }
    }
}
