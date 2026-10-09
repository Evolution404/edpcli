//! Read-only EDP native geometry planning. No protocol synthesis or disk I/O.
use super::{
    official_partition_semantics, visible_mbr_partition_type, OfficialPartitionMode,
    OfficialPartitionSemantics, PartitionRole, TargetPartitionGeometry,
    OFFICIAL_PARTITION_START_SECTOR,
};
use crate::filesystem::{FilesystemKind, NativeFilesystemWrite};
use crate::protocol::image::NativeProtocolImage;
use crate::protocol::lba7_compat::LBA7_COMPAT_EXTENT_TOTAL_SIZE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeEdpWriteCapability {
    ReadOnlySourceGeometry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeEdpExtent {
    pub start_lba: u64,
    pub sector_count: u64,
}

impl NativeEdpExtent {
    pub fn end_exclusive(self) -> Option<u64> {
        self.start_lba.checked_add(self.sector_count)
    }
    pub fn byte_count(self, native_sector_bytes: u32) -> Option<u64> {
        self.sector_count
            .checked_mul(u64::from(native_sector_bytes))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeEdpPartition {
    pub geometry: TargetPartitionGeometry,
    pub semantics: OfficialPartitionSemantics,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeEdpLayoutPlan {
    pub mode: OfficialPartitionMode,
    pub logical_sector_bytes: u32,
    pub total_sectors: u64,
    pub protocol: NativeEdpExtent,
    pub reserved: NativeEdpExtent,
    pub lce: NativeEdpExtent,
    pub partitions: Vec<NativeEdpPartition>,
    pub visible_mbr_type: u8,
    pub write_capability: NativeEdpWriteCapability,
}

fn extent(start: u64, count: u64, total: u64, label: &str) -> Result<NativeEdpExtent, String> {
    let result = NativeEdpExtent {
        start_lba: start,
        sector_count: count,
    };
    if count == 0 || result.end_exclusive().is_none_or(|end| end > total) {
        return Err(format!("{label} 范围为空、溢出或超出原生LBA容量"));
    }
    Ok(result)
}
fn overlaps(a: NativeEdpExtent, b: NativeEdpExtent) -> bool {
    a.start_lba < b.end_exclusive().unwrap() && b.start_lba < a.end_exclusive().unwrap()
}

impl NativeEdpLayoutPlan {
    /// Accepts separately confirmed source geometry only. All modes remain
    /// read-only. The legacy 512B protocol projection must never be multiplied
    /// into native logical LBA locations or assumed to be a physical writer.
    pub fn from_confirmed_geometry(
        mode: OfficialPartitionMode,
        total_sectors: u64,
        logical_sector_bytes: u32,
        partitions: &[TargetPartitionGeometry],
        lce_start_lba: u64,
        lce_sector_count: u64,
    ) -> Result<Self, String> {
        if !matches!(logical_sector_bytes, 512 | 4096)
            || total_sectors <= OFFICIAL_PARTITION_START_SECTOR
            || total_sectors > u32::MAX as u64
            || total_sectors
                .checked_mul(u64::from(logical_sector_bytes))
                .is_none()
        {
            return Err("EDP 未认证原生扇区大小或容量越界".into());
        }
        if partitions.len() != mode.partition_types().len() {
            return Err("EDP 来源分区数量与官方模式不一致".into());
        }
        let protocol = extent(0, 13, total_sectors, "EDP主协议")?;
        let reserved = extent(
            13,
            OFFICIAL_PARTITION_START_SECTOR - 13,
            total_sectors,
            "EDP保留",
        )?;
        let lce = extent(lce_start_lba, lce_sector_count, total_sectors, "LCE")?;
        let expected_lce =
            (LBA7_COMPAT_EXTENT_TOTAL_SIZE as u64).div_ceil(u64::from(logical_sector_bytes));
        if lce_sector_count != expected_lce || lce_start_lba < OFFICIAL_PARTITION_START_SECTOR {
            return Err("LCE 占用长度或原生范围无效".into());
        }
        let mut regions = vec![protocol, reserved, lce];
        let mut validated = Vec::with_capacity(partitions.len());
        for (index, part) in partitions.iter().copied().enumerate() {
            let expected = mode.partition_types()[index];
            if part.partition_type != expected {
                return Err(format!("mode{} slot{index} 分区类型不匹配", mode as u8));
            }
            let semantics = official_partition_semantics(mode, index, expected)?;
            if part.role != semantics.role
                || part.physically_encrypted != semantics.physically_encrypted()
            {
                return Err(format!(
                    "mode{} slot{index} 分区角色或物理加密标志不匹配",
                    mode as u8
                ));
            }
            if (part.role == PartitionRole::CompatibilityReserve && part.filesystem.is_some())
                || (part.role != PartitionRole::CompatibilityReserve && part.filesystem.is_none())
            {
                return Err(format!("mode{} slot{index} 文件系统角色不匹配", mode as u8));
            }
            let current = extent(part.start_lba, part.sector_count, total_sectors, "EDP分区")?;
            if current.start_lba < OFFICIAL_PARTITION_START_SECTOR
                || regions.iter().any(|previous| overlaps(current, *previous))
            {
                return Err(format!(
                    "mode{} slot{index} 原生分区重叠或侵入保留区",
                    mode as u8
                ));
            }
            regions.push(current);
            validated.push(NativeEdpPartition {
                geometry: part,
                semantics,
            });
        }
        let front_fs = partitions[0].filesystem.unwrap_or(FilesystemKind::Fat16);
        Ok(Self {
            mode,
            logical_sector_bytes,
            total_sectors,
            protocol,
            reserved,
            lce,
            partitions: validated,
            visible_mbr_type: visible_mbr_partition_type(mode, front_fs),
            write_capability: NativeEdpWriteCapability::ReadOnlySourceGeometry,
        })
    }

    pub const fn protocol_projection_bytes(&self) -> usize {
        13 * 512
    }
    pub fn native_protocol_bytes(&self) -> Option<u64> {
        self.protocol.byte_count(self.logical_sector_bytes)
    }
    pub fn lce_payload_bytes(&self) -> usize {
        LBA7_COMPAT_EXTENT_TOTAL_SIZE
    }
    pub fn lce_native_bytes(&self) -> Option<u64> {
        self.lce.byte_count(self.logical_sector_bytes)
    }
    pub const fn may_write(&self) -> bool {
        false
    }

    /// Require the source EDPF consumer to authenticate both protocol blocks
    /// against the same device identity, then bind **every** source partition
    /// to independently confirmed native geometry before staging a virtual
    /// source replay. Unlike the legacy MBR-only preflight below, this rejects
    /// stale LBA12 secondary partition geometry even when LBA0 looks valid.
    ///
    /// This operation is wholly in memory; it preserves opaque native tails
    /// verbatim and DOES NOT certify newly generated 4Kn protocol or enable
    /// physical writes.
    pub fn verified_source_replay_native_blocks(
        &self,
        protocol: &NativeProtocolImage,
        device_id: &str,
        source_lce: &[Vec<u8>],
    ) -> Result<Vec<NativeFilesystemWrite>, String> {
        if device_id.is_empty() {
            return Err("来源device_id为空，无法认证协议重放".into());
        }
        if protocol.logical_sector_bytes() != self.logical_sector_bytes {
            return Err("来源协议原生扇区宽度与确认布局不匹配".into());
        }
        let parsed =
            super::parse_existing_provision_native(protocol, device_id, self.total_sectors)?
                .ok_or("来源没有可确认的成对LBA7/LBA12 EDPF记录")?;
        if parsed.profile.source_mode != self.mode
            || parsed.profile.partitions.len() != self.partitions.len()
            || parsed.records.len() != self.partitions.len()
        {
            return Err("来源EDPF制盘模式或分区数量与确认布局不匹配".into());
        }
        for (slot, (source, target)) in parsed
            .profile
            .partitions
            .iter()
            .zip(&self.partitions)
            .enumerate()
        {
            let target = &target.geometry;
            if source.role != target.role
                || source.partition_type != target.partition_type
                || source.start_lba != target.start_lba
                || source.sector_count != target.sector_count
                || source.physically_encrypted != target.physically_encrypted
            {
                return Err(format!("来源EDPF slot{slot}与确认的全部原生分区几何不一致"));
            }
        }
        self.source_replay_native_blocks(protocol, source_lce)
    }

    /// Stage an exact existing-source protocol+LCE replay into a *virtual*
    /// image. This is not protocol generation or an authorized disk write.
    /// All native opaque tails are copied unchanged, including LBA11.
    /// The MBR preflight checks only the visible first partition; cryptographic
    /// LBA7/LBA12 trust still requires an independent source verification.
    pub fn source_replay_native_blocks(
        &self,
        protocol: &NativeProtocolImage,
        source_lce: &[Vec<u8>],
    ) -> Result<Vec<NativeFilesystemWrite>, String> {
        if protocol.logical_sector_bytes() != self.logical_sector_bytes
            || source_lce.len() != self.lce.sector_count as usize
            || source_lce
                .iter()
                .any(|block| block.len() != self.logical_sector_bytes as usize)
        {
            return Err("来源协议/LCE 原生逻辑扇区几何不匹配".into());
        }
        let mbr = protocol.block(0).ok_or("来源缺少 LBA0 原生块")?;
        if mbr[510..512] != [0x55, 0xaa] || mbr[446 + 4] != self.visible_mbr_type {
            return Err("来源 LBA0 MBR 签名或分区类型不匹配".into());
        }
        let start = u32::from_le_bytes(mbr[454..458].try_into().unwrap()) as u64;
        let count = u32::from_le_bytes(mbr[458..462].try_into().unwrap()) as u64;
        if start != self.partitions[0].geometry.start_lba
            || count != self.partitions[0].geometry.sector_count
        {
            return Err("来源 LBA0 MBR 首分区原生起点/容量与已确认布局不一致".into());
        }
        let mut output = Vec::with_capacity(13 + source_lce.len());
        for lba in 1..13 {
            output.push(NativeFilesystemWrite {
                relative_lba: lba as u64,
                data: protocol.block(lba).unwrap().to_vec(),
            });
        }
        for (index, block) in source_lce.iter().enumerate() {
            output.push(NativeFilesystemWrite {
                relative_lba: self.lce.start_lba + index as u64,
                data: block.clone(),
            });
        }
        // Physical transaction semantics are NOT implied by output order;
        // the virtual image can still test an LBA0-last commit.
        output.push(NativeFilesystemWrite {
            relative_lba: 0,
            data: mbr.to_vec(),
        });
        Ok(output)
    }
}
