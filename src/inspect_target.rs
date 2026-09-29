//! 任意物理扇区的区域识别与只读解码策略。
//!
//! 这里不负责 CLI 展示。所有 decode 都必须来自已经验证的协议/数据区算法；
//! 无法确认的区域返回明确错误，绝不把 RAW 静默冒充为 decoded。

use crate::backup_metadata::{
    parse_lba7_compatibility_geometry, parse_partition_geometry, Lba7CompatibilityGeometry,
    PartitionGeometry, TAIL_END4_MIRROR_OFFSET_SECTORS, TAIL_METADATA_MIRROR_OFFSET_SECTORS,
    TAIL_METADATA_MIRROR_SECTORS,
};
use crate::common::{METADATA_LAST_LBA, SECTOR};
use crate::crypto::a6b0_full_offset;
use crate::filesystem::FilesystemKind;
use crate::provision::{
    decrypt_mode2, default_file_key, default_file_key_checked, DefaultFileKeyError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SectorRegion {
    Protocol {
        lba: u64,
    },
    PartitionTable {
        label: String,
        start_lba: u64,
    },
    PlainPartition {
        index: usize,
        label: String,
        relative_lba: u64,
    },
    Unallocated,
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
    Unknown,
}

impl SectorRegion {
    pub fn label(&self) -> String {
        match self {
            Self::Protocol { lba } => format!("EDP 主协议区 LBA{lba}"),
            Self::PartitionTable { label, .. } => label.clone(),
            Self::PlainPartition {
                index,
                label,
                relative_lba,
            } => format!("普通分区 P{index} {label} +{relative_lba}"),
            Self::Unallocated => "未分配空间".into(),
            Self::Lce { index, .. } => format!("LCE +{index}/6"),
            Self::Partition {
                index,
                partition_type,
                relative_lba,
                ..
            } => format!("分区[{index}] type{partition_type} +{relative_lba}"),
            Self::TailMetadataMirror { index } => format!("盘尾历史 9 扇区镜像 +{index}/9"),
            Self::RestoreNodeEnd4 => "盘尾恢复节点（末端前第4扇区）".into(),
            Self::Unknown => "未知物理扇区".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhysicalDataState {
    PlaintextFilesystem { filesystem: FilesystemKind },
    EncryptedMode2 { filesystem: FilesystemKind },
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

    pub fn filesystem(&self) -> Option<FilesystemKind> {
        match self {
            Self::PlaintextFilesystem { filesystem } | Self::EncryptedMode2 { filesystem } => {
                Some(*filesystem)
            }
            Self::Unknown { .. } => None,
        }
    }
}

// 仅用于 MBR 固定字段解析；文件系统 boot-sector 解析统一由 filesystem registry 负责。
fn u32le(raw: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap())
}

fn detect_filesystem_boot(partition: &PartitionGeometry, boot: &[u8]) -> Option<FilesystemKind> {
    if boot.len() != SECTOR {
        return None;
    }
    let mut reader = crate::filesystem::BootSectorReader::new(boot, partition.sector_count);
    crate::filesystem::default_registry()
        .detect(&mut reader)
        .ok()
        .flatten()
        .map(|detected| detected.kind())
}

pub fn detect_plain_filesystem(sector_count: u64, boot: &[u8]) -> Option<FilesystemKind> {
    let partition = PartitionGeometry {
        index: 0,
        partition_type: 0,
        partition_count: 1,
        need_disturb: 0,
        need_encrypt: 0,
        start_sector: 0,
        sector_size: SECTOR as u64,
        partition_size: sector_count.saturating_mul(SECTOR as u64),
        sector_count,
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    };
    detect_filesystem_boot(&partition, boot)
}

#[derive(Clone, Debug)]
pub struct InspectDiskContext {
    pub protocol_image: Vec<u8>,
    pub device_id: Option<String>,
    pub total_sectors: u64,
    pub provision_kind: Option<crate::provision::DiskProvisionKind>,
    pub partition_table: Option<crate::partition_table::PartitionTableSnapshot>,
    pub partitions: Vec<PartitionGeometry>,
    pub lce: Option<Lba7CompatibilityGeometry>,
    pub context_issues: Vec<String>,
}

impl InspectDiskContext {
    pub fn new(protocol_image: Vec<u8>, device_id: Option<String>, total_sectors: u64) -> Self {
        let provision_kind = match device_id.as_deref() {
            Some(did) => crate::provision::DiskProvisionKind::from_metadata(&protocol_image, did),
            None => {
                let onlyid = protocol_image
                    .get(4 * SECTOR..5 * SECTOR)
                    .and_then(crate::diskio::lba4_label_id_from);
                (onlyid.is_none()
                    && crate::partition_table::confirmed_plain_protocol_prefix(
                        &protocol_image,
                        total_sectors,
                    ))
                .then_some(crate::provision::DiskProvisionKind::Plain)
            }
        };
        let partition_table = if provision_kind == Some(crate::provision::DiskProvisionKind::Plain)
        {
            crate::partition_table::read_partition_table(total_sectors, |lba| {
                let start = usize::try_from(lba)
                    .ok()
                    .and_then(|value| value.checked_mul(SECTOR))
                    .ok_or_else(|| format!("LBA{lba} 偏移溢出"))?;
                protocol_image
                    .get(start..start + SECTOR)
                    .map(|sector| sector.to_vec())
                    .ok_or_else(|| format!("LBA{lba} 不在当前 LBA0-12 上下文中"))
            })
            .ok()
        } else {
            None
        };
        Self::new_with_partition_table(
            protocol_image,
            device_id,
            total_sectors,
            provision_kind,
            partition_table,
            None,
        )
    }

    pub fn new_with_partition_table(
        protocol_image: Vec<u8>,
        device_id: Option<String>,
        total_sectors: u64,
        provision_kind: Option<crate::provision::DiskProvisionKind>,
        partition_table: Option<crate::partition_table::PartitionTableSnapshot>,
        partition_table_issue: Option<String>,
    ) -> Self {
        let mut partitions = Vec::new();
        let mut lce = None;
        let mut context_issues = Vec::new();

        if provision_kind == Some(crate::provision::DiskProvisionKind::Plain) {
            if let Some(issue) = partition_table_issue {
                context_issues.push(format!("普通盘分区表不可用: {issue}"));
            }
        } else if provision_kind.is_some() || device_id.is_some() {
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
                context_issues.push("EDP 盘缺少 device_id，无法解析 LBA7/LBA12 区域几何".into());
            }
        } else {
            context_issues.push("盘型未确认；仅提供 RAW/已验证分区表证据".into());
        }

        Self {
            protocol_image,
            device_id,
            total_sectors,
            provision_kind,
            partition_table,
            partitions,
            lce,
            context_issues,
        }
    }

    pub fn is_plain(&self) -> bool {
        self.provision_kind == Some(crate::provision::DiskProvisionKind::Plain)
    }

    pub fn has_edp_protocol(&self) -> bool {
        self.provision_kind
            .is_some_and(|kind| kind != crate::provision::DiskProvisionKind::Plain)
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

        if self.is_plain() {
            if let Some(table) = &self.partition_table {
                for extent in &table.table_extents {
                    if lba >= extent.start_lba
                        && lba < extent.start_lba.saturating_add(extent.sector_count)
                    {
                        out.push(SectorRegion::PartitionTable {
                            label: extent.label.clone(),
                            start_lba: extent.start_lba,
                        });
                    }
                }
                if let Some(partition) = table.partition_for_lba(lba) {
                    out.push(SectorRegion::PlainPartition {
                        index: partition.index,
                        label: partition
                            .filesystem
                            .clone()
                            .unwrap_or_else(|| partition.type_label()),
                        relative_lba: lba - partition.start_lba,
                    });
                }
            }
            if out.is_empty() {
                out.push(if self.partition_table.is_some() {
                    SectorRegion::Unallocated
                } else {
                    SectorRegion::Unknown
                });
            }
            return out;
        }

        if self.has_edp_protocol() && lba <= u64::from(METADATA_LAST_LBA) {
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

        if self.has_edp_protocol() && self.total_sectors >= TAIL_METADATA_MIRROR_OFFSET_SECTORS {
            let start = self.total_sectors - TAIL_METADATA_MIRROR_OFFSET_SECTORS;
            if lba >= start && lba < start + TAIL_METADATA_MIRROR_SECTORS {
                out.push(SectorRegion::TailMetadataMirror { index: lba - start });
            }
        }

        if self.has_edp_protocol()
            && self.total_sectors > TAIL_END4_MIRROR_OFFSET_SECTORS
            && lba == self.total_sectors - TAIL_END4_MIRROR_OFFSET_SECTORS
        {
            out.push(SectorRegion::RestoreNodeEnd4);
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

    pub fn plain_partition_for_lba(
        &self,
        lba: u64,
    ) -> Option<&crate::partition_table::PhysicalPartition> {
        self.partition_table
            .as_ref()
            .and_then(|table| table.partition_for_lba(lba))
    }

    pub fn partition_start_for_lba(&self, lba: u64) -> Option<u64> {
        if self.is_plain() {
            self.plain_partition_for_lba(lba)
                .map(|partition| partition.start_lba)
        } else {
            self.partition_for_lba(lba)
                .map(|partition| partition.start_sector)
        }
    }

    pub fn has_partition_for_lba(&self, lba: u64) -> bool {
        self.partition_start_for_lba(lba).is_some()
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
        match default_file_key_checked(&self.protocol_image, did, partition.index) {
            Ok(_) => "PASS".into(),
            Err(error @ DefaultFileKeyError::FileKeyCrcMismatch) => {
                format!("FAIL（{error}）")
            }
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
        let key = match default_file_key(&self.protocol_image, did, partition.index) {
            Ok(key) => key,
            Err(error) => {
                return PhysicalDataState::Unknown {
                    reason: format!("raw 未识别为明文文件系统；FileKey 校验失败: {error}"),
                };
            }
        };
        let decoded = match decrypt_mode2(raw_boot, &key) {
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

        if self.is_plain() {
            if let Some(partition) = self.plain_partition_for_lba(lba) {
                let boot = if lba == partition.start_lba {
                    raw
                } else {
                    partition_boot_raw.ok_or_else(|| {
                        format!(
                            "普通分区 P{} 缺少起始 LBA{} 证据，无法验证文件系统；decode fail-closed",
                            partition.index, partition.start_lba
                        )
                    })?
                };
                let filesystem = detect_plain_filesystem(partition.sector_count, boot)
                    .ok_or_else(|| {
                        format!(
                            "普通分区 P{} 起始扇区未通过 FAT/exFAT/NTFS 严格校验；decode fail-closed",
                            partition.index
                        )
                    })?;
                return Ok((
                    raw.to_vec(),
                    format!(
                        "普通分区 P{}：物理明文 {}，严格 boot-sector 校验通过；decode=raw",
                        partition.index,
                        filesystem.label()
                    ),
                ));
            }
            return Err(format!(
                "LBA{lba} 不属于普通盘已识别分区；raw 可读，decode 拒绝猜测"
            ));
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
                    let key = default_file_key(&self.protocol_image, did, partition.index)
                        .map_err(|error| {
                            format!(
                                "分区[{}] type{} 无法取得已验证 FileKey: {error}",
                                partition.index, partition.partition_type
                            )
                        })?;
                    let plain = decrypt_mode2(raw, &key)?;
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
            .partition_start_for_lba(lba)
            .and_then(|start| (start == lba).then_some(raw));
        self.decode_non_protocol_with_boot(lba, raw, boot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_context_does_not_invent_edp_protocol_or_tail() {
        let context = InspectDiskContext::new(vec![0; 13 * SECTOR], None, 4096);
        assert_eq!(context.provision_kind, None);
        assert_eq!(context.regions(7), vec![SectorRegion::Unknown]);
        assert_eq!(context.regions(4095), vec![SectorRegion::Unknown]);
        assert!(context.validate_lba(4096).is_err());
    }
}
