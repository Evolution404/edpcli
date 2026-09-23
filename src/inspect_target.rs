//! 任意物理扇区的区域识别与只读解码策略。
//!
//! 这里不负责 CLI 展示。所有 decode 都必须来自已经验证的协议/数据区算法；
//! 无法确认的区域返回明确错误，绝不把 RAW 静默冒充为 decoded。

use crate::backup_deep::keys;
use crate::backup_metadata::{
    parse_lba7_compatibility_geometry, parse_partition_geometry, Lba7CompatibilityGeometry,
    PartitionGeometry, DEVICE_TAIL_WINDOW_SECTORS, TAIL_END4_MIRROR_OFFSET_SECTORS,
    TAIL_METADATA_MIRROR_OFFSET_SECTORS, TAIL_METADATA_MIRROR_SECTORS,
};
use crate::common::{METADATA_LAST_LBA, SECTOR};
use crate::crypto::a6b0_full_offset;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SectorRegion {
    Protocol {
        lba: u64,
    },
    Lce {
        start_lba: u64,
        index: u64,
    },
    Partition {
        index: usize,
        partition_type: u32,
        relative_lba: u64,
        need_encrypt: u32,
        encrypt_mode: u8,
    },
    TailMetadataMirror {
        index: u64,
    },
    RestoreNodeEnd4,
    DeviceTailWindow {
        relative_lba: u64,
    },
    Unknown,
}

impl SectorRegion {
    pub fn label(&self) -> String {
        match self {
            Self::Protocol { lba } => format!("EDP 主协议区 LBA{lba}"),
            Self::Lce { index, .. } => format!("LCE +{index}/6"),
            Self::Partition {
                index,
                partition_type,
                relative_lba,
                ..
            } => format!("分区[{index}] type{partition_type} +{relative_lba}"),
            Self::TailMetadataMirror { index } => format!("盘尾历史 9 扇区镜像 +{index}/9"),
            Self::RestoreNodeEnd4 => "盘尾 end-4 restore-node".into(),
            Self::DeviceTailWindow { relative_lba } => {
                format!("盘尾 2048 扇区取证窗口 +{relative_lba}")
            }
            Self::Unknown => "未知物理扇区".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilesystemBootKind {
    Fat16,
    Fat32,
    Exfat,
    Ntfs,
}

impl FilesystemBootKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fat16 => "FAT16",
            Self::Fat32 => "FAT32",
            Self::Exfat => "exFAT",
            Self::Ntfs => "NTFS",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhysicalDataState {
    PlaintextFilesystem { filesystem: FilesystemBootKind },
    EncryptedMode2 { filesystem: FilesystemBootKind },
    Unknown { reason: String },
}

impl PhysicalDataState {
    pub fn label(&self) -> String {
        match self {
            Self::PlaintextFilesystem { filesystem } => {
                format!("物理明文文件系统 ({})", filesystem.label())
            }
            Self::EncryptedMode2 { filesystem } => {
                format!("SM4 mode2 密文（解密后为 {}）", filesystem.label())
            }
            Self::Unknown { reason } => format!("未知（{reason}）"),
        }
    }

    pub fn decode_strategy(&self) -> String {
        match self {
            Self::PlaintextFilesystem { filesystem } => format!(
                "raw 已通过严格 {} boot-sector 校验；不执行 SM4，decode=raw",
                filesystem.label()
            ),
            Self::EncryptedMode2 { filesystem } => format!(
                "FileKeyCRC 验证通过后执行 SM4-ECB；分区起始扇区解密后通过严格 {} boot-sector 校验",
                filesystem.label()
            ),
            Self::Unknown { reason } => format!("fail-closed：{reason}"),
        }
    }

    pub fn filesystem(&self) -> Option<FilesystemBootKind> {
        match self {
            Self::PlaintextFilesystem { filesystem } | Self::EncryptedMode2 { filesystem } => {
                Some(*filesystem)
            }
            Self::Unknown { .. } => None,
        }
    }
}

fn u16le(raw: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(raw[offset..offset + 2].try_into().unwrap())
}

fn u32le(raw: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap())
}

fn u64le(raw: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(raw[offset..offset + 8].try_into().unwrap())
}

fn valid_boot_jump(boot: &[u8]) -> bool {
    (boot[0] == 0xeb && boot[2] == 0x90) || boot[0] == 0xe9
}

fn valid_spc(spc: u32) -> bool {
    spc != 0 && spc.is_power_of_two() && spc <= 128
}

fn total_fits_partition(total: u64, partition: &PartitionGeometry) -> bool {
    total != 0 && total <= partition.sector_count
}

fn detect_filesystem_boot(
    partition: &PartitionGeometry,
    boot: &[u8],
) -> Option<FilesystemBootKind> {
    if boot.len() != SECTOR || boot[510..512] != [0x55, 0xaa] || !valid_boot_jump(boot) {
        return None;
    }

    if boot.get(3..11) == Some(b"NTFS    ") {
        let bps = u16le(boot, 11) as u32;
        let spc = boot[13] as u32;
        let total = u64le(boot, 40);
        let clusters = total.checked_div(spc as u64)?;
        let mft = u64le(boot, 48);
        let mft_mirror = u64le(boot, 56);
        if bps == SECTOR as u32
            && valid_spc(spc)
            && total_fits_partition(total, partition)
            && boot[14..21].iter().all(|&byte| byte == 0)
            && boot[21] >= 0xf0
            && clusters > 0
            && mft < clusters
            && mft_mirror < clusters
        {
            return Some(FilesystemBootKind::Ntfs);
        }
        return None;
    }

    if boot.get(3..11) == Some(b"EXFAT   ") {
        let volume_length = u64le(boot, 72);
        let fat_offset = u32le(boot, 80) as u64;
        let fat_length = u32le(boot, 84) as u64;
        let heap_offset = u32le(boot, 88) as u64;
        let cluster_count = u32le(boot, 92) as u64;
        let root_cluster = u32le(boot, 96) as u64;
        let bps_shift = boot[108];
        let spc_shift = boot[109];
        let fats = boot[110] as u64;
        let percent_in_use = boot[112];
        let spc = 1u64.checked_shl(spc_shift as u32)?;
        let fat_end = fat_offset.checked_add(fat_length.checked_mul(fats)?)?;
        let heap_end = heap_offset.checked_add(cluster_count.checked_mul(spc)?)?;
        if boot[11..64].iter().all(|&byte| byte == 0)
            && bps_shift == 9
            && spc_shift < 26
            && matches!(fats, 1 | 2)
            && (percent_in_use <= 100 || percent_in_use == 0xff)
            && total_fits_partition(volume_length, partition)
            && fat_offset >= 24
            && fat_length > 0
            && heap_offset >= fat_end
            && cluster_count > 0
            && root_cluster >= 2
            && root_cluster < cluster_count + 2
            && heap_end <= volume_length
        {
            return Some(FilesystemBootKind::Exfat);
        }
        return None;
    }

    let bps = u16le(boot, 11) as u32;
    let spc = boot[13] as u32;
    let reserved = u16le(boot, 14) as u64;
    let fats = boot[16] as u64;
    let root_entries = u16le(boot, 17) as u64;
    let total16 = u16le(boot, 19) as u64;
    let media = boot[21];
    let fat16 = u16le(boot, 22) as u64;
    let total32 = u32le(boot, 32) as u64;
    let total = if total16 != 0 { total16 } else { total32 };
    if bps != SECTOR as u32
        || !valid_spc(spc)
        || reserved == 0
        || !matches!(fats, 1 | 2)
        || media < 0xf0
        || !total_fits_partition(total, partition)
    {
        return None;
    }
    let root_dir_sectors = root_entries.checked_mul(32)?.div_ceil(SECTOR as u64);
    let fat_size = if fat16 != 0 {
        fat16
    } else {
        u32le(boot, 36) as u64
    };
    if fat_size == 0 {
        return None;
    }
    let metadata_sectors = reserved
        .checked_add(fats.checked_mul(fat_size)?)?
        .checked_add(root_dir_sectors)?;
    let data_sectors = total.checked_sub(metadata_sectors)?;
    let cluster_count = data_sectors / spc as u64;

    if cluster_count >= 65_525 {
        let root_cluster = u32le(boot, 44) as u64;
        if fat16 == 0 && root_entries == 0 && root_cluster >= 2 && root_cluster < cluster_count + 2
        {
            return Some(FilesystemBootKind::Fat32);
        }
        return None;
    }
    if cluster_count >= 4_085 && fat16 != 0 && root_entries != 0 {
        return Some(FilesystemBootKind::Fat16);
    }
    None
}

#[derive(Clone, Debug)]
pub struct InspectDiskContext {
    pub protocol_image: Vec<u8>,
    pub device_id: Option<String>,
    pub total_sectors: u64,
    pub partitions: Vec<PartitionGeometry>,
    pub lce: Option<Lba7CompatibilityGeometry>,
    pub context_issues: Vec<String>,
}

impl InspectDiskContext {
    pub fn new(protocol_image: Vec<u8>, device_id: Option<String>, total_sectors: u64) -> Self {
        let mut partitions = Vec::new();
        let mut lce = None;
        let mut context_issues = Vec::new();
        if let Some(did) = device_id.as_deref() {
            match parse_partition_geometry(&protocol_image, did, total_sectors) {
                Ok(value) => partitions = value,
                Err(error) => context_issues.push(format!("LBA12 分区几何不可用: {error}")),
            }
            match parse_lba7_compatibility_geometry(&protocol_image, did, total_sectors) {
                Ok(value) => lce = Some(value),
                Err(error) => context_issues.push(format!("LCE 几何不可用: {error}")),
            }
        } else {
            context_issues.push("缺少 device_id，无法解析 LBA7/LBA12 区域几何".into());
        }
        Self {
            protocol_image,
            device_id,
            total_sectors,
            partitions,
            lce,
            context_issues,
        }
    }

    pub fn validate_lba(&self, lba: u64) -> Result<(), String> {
        if lba >= self.total_sectors {
            return Err(format!(
                "LBA{lba} 越界：设备共有 {} 个扇区，合法范围 0..{}",
                self.total_sectors,
                self.total_sectors.saturating_sub(1)
            ));
        }
        Ok(())
    }

    pub fn regions(&self, lba: u64) -> Vec<SectorRegion> {
        let mut out = Vec::new();
        if lba <= u64::from(METADATA_LAST_LBA) {
            out.push(SectorRegion::Protocol { lba });
        }

        if let Some(lce) = &self.lce {
            if lba >= lce.start_lba && lba < lce.start_lba + lce.sector_count {
                out.push(SectorRegion::Lce {
                    start_lba: lce.start_lba,
                    index: lba - lce.start_lba,
                });
            }
        }

        for partition in &self.partitions {
            if lba >= partition.start_sector
                && lba
                    < partition
                        .start_sector
                        .saturating_add(partition.sector_count)
            {
                out.push(SectorRegion::Partition {
                    index: partition.index,
                    partition_type: partition.partition_type,
                    relative_lba: lba - partition.start_sector,
                    need_encrypt: partition.need_encrypt,
                    encrypt_mode: partition.encrypt_mode,
                });
            }
        }

        if self.total_sectors >= TAIL_METADATA_MIRROR_OFFSET_SECTORS {
            let start = self.total_sectors - TAIL_METADATA_MIRROR_OFFSET_SECTORS;
            if lba >= start && lba < start + TAIL_METADATA_MIRROR_SECTORS {
                out.push(SectorRegion::TailMetadataMirror { index: lba - start });
            }
        }

        if self.total_sectors > TAIL_END4_MIRROR_OFFSET_SECTORS
            && lba == self.total_sectors - TAIL_END4_MIRROR_OFFSET_SECTORS
        {
            out.push(SectorRegion::RestoreNodeEnd4);
        }

        let tail_count = self.total_sectors.min(DEVICE_TAIL_WINDOW_SECTORS);
        let tail_start = self.total_sectors - tail_count;
        if lba >= tail_start {
            out.push(SectorRegion::DeviceTailWindow {
                relative_lba: lba - tail_start,
            });
        }

        if out.is_empty() {
            out.push(SectorRegion::Unknown);
        }
        out
    }

    pub fn partition_for_lba(&self, lba: u64) -> Option<&PartitionGeometry> {
        self.partitions.iter().find(|partition| {
            lba >= partition.start_sector
                && lba
                    < partition
                        .start_sector
                        .saturating_add(partition.sector_count)
        })
    }

    pub fn partition_mbr_exposure(&self, partition: &PartitionGeometry) -> String {
        let Some(mbr) = self.protocol_image.get(..SECTOR) else {
            return "未知（缺少 LBA0）".into();
        };
        if mbr[510..512] != [0x55, 0xaa] {
            return "否（LBA0 无有效 MBR 签名）".into();
        }
        for slot in 0..4usize {
            let offset = 0x1be + slot * 16;
            let partition_type = mbr[offset + 4];
            let start = u32le(mbr, offset + 8) as u64;
            let sectors = u32le(mbr, offset + 12) as u64;
            if partition_type != 0 && start == partition.start_sector && sectors != 0 {
                return format!(
                    "是（MBR P{} type=0x{partition_type:02X} start={start} sectors={sectors}）",
                    slot + 1
                );
            }
        }
        "否（MBR 没有直接指向该分区起始 LBA）".into()
    }

    pub fn partition_file_key_crc_status(&self, partition: &PartitionGeometry) -> String {
        if partition.need_encrypt == 0 {
            return "不适用（need_encrypt=0）".into();
        }
        if partition.encrypt_mode != 2 {
            return format!("未验证（encrypt_mode={}）", partition.encrypt_mode);
        }
        let Some(did) = self.device_id.as_deref() else {
            return "未验证（缺少 device_id）".into();
        };
        match keys::default_file_key(&self.protocol_image, did, partition.index) {
            Ok(_) => "PASS".into(),
            Err(error) if error.contains("FileKeyCRC") => format!("FAIL（{error}）"),
            Err(error) => format!("未验证（{error}）"),
        }
    }

    pub fn partition_physical_state(
        &self,
        partition: &PartitionGeometry,
        raw_boot: &[u8],
    ) -> PhysicalDataState {
        if let Some(filesystem) = detect_filesystem_boot(partition, raw_boot) {
            return PhysicalDataState::PlaintextFilesystem { filesystem };
        }

        if partition.need_encrypt == 0 {
            return PhysicalDataState::Unknown {
                reason: "raw 未通过严格文件系统 boot-sector 校验；need_encrypt=0 也不能证明当前物理数据可直接作为明文".into(),
            };
        }
        if partition.encrypt_mode != 2 {
            return PhysicalDataState::Unknown {
                reason: format!(
                    "raw 未通过严格文件系统 boot-sector 校验，且 encrypt_mode={} 没有已验证解码器",
                    partition.encrypt_mode
                ),
            };
        }
        let Some(did) = self.device_id.as_deref() else {
            return PhysicalDataState::Unknown {
                reason:
                    "raw 未通过严格文件系统 boot-sector 校验，且缺少 device_id，无法验证 FileKey"
                        .into(),
            };
        };
        let key = match keys::default_file_key(&self.protocol_image, did, partition.index) {
            Ok(key) => key,
            Err(error) => {
                return PhysicalDataState::Unknown {
                    reason: format!("raw 未识别为明文文件系统；FileKey 校验失败: {error}"),
                };
            }
        };
        let decoded = match keys::decrypt_mode2(raw_boot, &key) {
            Ok(decoded) => decoded,
            Err(error) => {
                return PhysicalDataState::Unknown {
                    reason: format!("SM4 mode2 解密失败: {error}"),
                };
            }
        };
        if let Some(filesystem) = detect_filesystem_boot(partition, &decoded) {
            PhysicalDataState::EncryptedMode2 { filesystem }
        } else {
            PhysicalDataState::Unknown {
                reason:
                    "raw 与 FileKeyCRC 验证后的 SM4 mode2 结果均未通过严格文件系统 boot-sector 校验"
                        .into(),
            }
        }
    }

    pub fn decode_non_protocol_with_boot(
        &self,
        lba: u64,
        raw: &[u8],
        partition_boot_raw: Option<&[u8]>,
    ) -> Result<(Vec<u8>, String), String> {
        if raw.len() != SECTOR {
            return Err(format!("LBA{lba} 长度 {}B，不是 512B", raw.len()));
        }

        if let Some(lce) = &self.lce {
            if lba >= lce.start_lba && lba < lce.start_lba + lce.sector_count {
                let physical_offset = lba
                    .checked_mul(SECTOR as u64)
                    .ok_or_else(|| "LCE 物理字节偏移溢出".to_string())?;
                let plain = a6b0_full_offset(raw, &[0u8; 8], physical_offset);
                return Ok((
                    plain,
                    format!(
                        "LCE：EDPSECDISK zero8 + 64 位物理字节偏移 tweak=0x{physical_offset:X}"
                    ),
                ));
            }
        }

        if let Some(partition) = self.partition_for_lba(lba) {
            let boot = if lba == partition.start_sector {
                raw
            } else {
                partition_boot_raw.ok_or_else(|| {
                    format!(
                        "分区[{}] type{} 缺少起始扇区证据，无法判断物理数据是明文还是 mode2 密文；decode fail-closed",
                        partition.index, partition.partition_type
                    )
                })?
            };
            let state = self.partition_physical_state(partition, boot);
            match state {
                PhysicalDataState::PlaintextFilesystem { filesystem } => Ok((
                    raw.to_vec(),
                    format!(
                        "分区[{}] type{}：物理盘面已为有效 {} 明文文件系统，未执行 SM4；MBR直接暴露={}",
                        partition.index,
                        partition.partition_type,
                        filesystem.label(),
                        self.partition_mbr_exposure(partition)
                    ),
                )),
                PhysicalDataState::EncryptedMode2 { filesystem } => {
                    let did = self
                        .device_id
                        .as_deref()
                        .ok_or_else(|| "缺少 device_id，无法解封数据区 FileKey".to_string())?;
                    let key = keys::default_file_key(&self.protocol_image, did, partition.index)
                        .map_err(|error| {
                            format!(
                                "分区[{}] type{} 无法取得已验证 FileKey: {error}",
                                partition.index, partition.partition_type
                            )
                        })?;
                    let plain = keys::decrypt_mode2(raw, &key)?;
                    Ok((
                        plain,
                        format!(
                            "分区[{}] type{}：SM4-ECB，FileKeyCRC=PASS；起始扇区解密后识别为 {}；MBR直接暴露={}",
                            partition.index,
                            partition.partition_type,
                            filesystem.label(),
                            self.partition_mbr_exposure(partition)
                        ),
                    ))
                }
                PhysicalDataState::Unknown { reason } => Err(format!(
                    "分区[{}] type{} 物理数据状态无法确认：{reason}；decode fail-closed",
                    partition.index, partition.partition_type
                )),
            }
        } else {
            Err(format!(
                "LBA{lba} 不属于已验证可解码区域；raw 可读，decode 拒绝猜测"
            ))
        }
    }

    pub fn decode_non_protocol(&self, lba: u64, raw: &[u8]) -> Result<(Vec<u8>, String), String> {
        let boot = self
            .partition_for_lba(lba)
            .and_then(|partition| (partition.start_sector == lba).then_some(raw));
        self.decode_non_protocol_with_boot(lba, raw, boot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_context_still_classifies_protocol_and_tail() {
        let context = InspectDiskContext::new(vec![0; 13 * SECTOR], None, 4096);
        assert_eq!(
            context.regions(7).first(),
            Some(&SectorRegion::Protocol { lba: 7 })
        );
        assert!(context
            .regions(4095)
            .iter()
            .any(|region| matches!(region, SectorRegion::DeviceTailWindow { .. })));
        assert!(context.validate_lba(4096).is_err());
    }
}
