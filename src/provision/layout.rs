//! First-party partition-mode and logical geometry model.
//!
//! This module is deliberately pure.  It models the partition list emitted by
//! the current first-party label-tool family; it does not create filesystems or
//! touch a device.

use crate::protocol::{
    edpf::EdpPartitionType, lba7::Lba7PartitionMode, lba7_compat::Lba7CompatibilityExtentLayout,
};

use super::{LegacyLba7KeyMaterial, OfficialFilesystemFormat, ProvisionKeyMaterial};

pub type OfficialPartitionMode = Lba7PartitionMode;

pub const OFFICIAL_PARTITION_START_SECTOR: u64 = 63;
pub const WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES: u64 = 0x7e00;
const MIB: u64 = 1024 * 1024;

/// MBR partition-type byte selected by the current first-party writer for the
/// four official partition modes.  This is the direct result of the producer's
/// partition-position selector, not a filesystem guess.
pub const fn official_mbr_partition_type(mode: OfficialPartitionMode) -> u8 {
    match mode {
        OfficialPartitionMode::DefaultThreePartition => 0x0e,
        OfficialPartitionMode::BootShareCombined => 0x07,
        OfficialPartitionMode::WholeDiskEncrypted => 0x0b,
        OfficialPartitionMode::IntranetExtranetDualPartition => 0x0e,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OfficialPartitionSizes {
    pub boot_mib: u64,
    pub share_mib: u64,
    pub encrypt_mib: u64,
}

impl OfficialPartitionSizes {
    pub const fn new(boot_mib: u64, share_mib: u64, encrypt_mib: u64) -> Self {
        Self {
            boot_mib,
            share_mib,
            encrypt_mib,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OfficialPartitionGeometry {
    pub partition_type: EdpPartitionType,
    pub start_sector: u64,
    pub sector_size: u64,
    pub size_bytes: u64,
}

impl OfficialPartitionGeometry {
    pub fn sector_count(self) -> u64 {
        self.size_bytes / self.sector_size
    }

    pub fn end_sector_exclusive(self) -> u64 {
        self.start_sector + self.sector_count()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OfficialProvisionPlan {
    pub mode: OfficialPartitionMode,
    pub sizes: OfficialPartitionSizes,
    pub filesystem_format: OfficialFilesystemFormat,
    pub lba7_compatibility_extent: Lba7CompatibilityExtentLayout,
    pub lba7_key_material: LegacyLba7KeyMaterial,
    pub lba12_key_material: ProvisionKeyMaterial,
}

impl OfficialProvisionPlan {
    pub fn new(
        mode: OfficialPartitionMode,
        sizes: OfficialPartitionSizes,
        lba7_compatibility_extent: Lba7CompatibilityExtentLayout,
        lba7_key_material: LegacyLba7KeyMaterial,
        lba12_key_material: ProvisionKeyMaterial,
    ) -> Result<Self, String> {
        Self::new_with_filesystem(
            mode,
            sizes,
            OfficialFilesystemFormat::ExFat,
            lba7_compatibility_extent,
            lba7_key_material,
            lba12_key_material,
        )
    }

    pub fn new_with_filesystem(
        mode: OfficialPartitionMode,
        sizes: OfficialPartitionSizes,
        filesystem_format: OfficialFilesystemFormat,
        lba7_compatibility_extent: Lba7CompatibilityExtentLayout,
        lba7_key_material: LegacyLba7KeyMaterial,
        lba12_key_material: ProvisionKeyMaterial,
    ) -> Result<Self, String> {
        if lba7_compatibility_extent.size_bytes == 0 || lba7_compatibility_extent.size_sectors == 0
        {
            return Err("LBA7 compatibility extent must be non-empty".into());
        }
        Ok(Self {
            mode,
            sizes,
            filesystem_format,
            lba7_compatibility_extent,
            lba7_key_material,
            lba12_key_material,
        })
    }

    pub fn logical_partitions(
        self,
        sector_size: u64,
    ) -> Result<Vec<OfficialPartitionGeometry>, String> {
        build_official_partition_layout(self.mode, self.sizes, sector_size)
    }
}

fn mib_bytes(value: u64) -> Result<u64, String> {
    value
        .checked_mul(MIB)
        .ok_or_else(|| format!("partition MiB value overflows bytes: {value}"))
}

/// Reproduce the logical partition sequence and size selection of the current
/// first-party writer.
///
/// The caller supplies the three UI/request MiB fields.  Modes ignore fields
/// that the official mode does not emit.  Mode 2 is special: the writer keeps
/// a type1 compatibility entry of exactly 0x7E00 bytes and deducts those bytes
/// from the requested encrypted size.
pub fn build_official_partition_layout(
    mode: OfficialPartitionMode,
    sizes: OfficialPartitionSizes,
    sector_size: u64,
) -> Result<Vec<OfficialPartitionGeometry>, String> {
    if sector_size == 0 {
        return Err("sector size must be non-zero".into());
    }

    let boot = mib_bytes(sizes.boot_mib)?;
    let share = mib_bytes(sizes.share_mib)?;
    let encrypt = mib_bytes(sizes.encrypt_mib)?;
    let logical: Vec<(EdpPartitionType, u64)> = match mode {
        OfficialPartitionMode::DefaultThreePartition => vec![
            (EdpPartitionType::Boot, boot),
            (EdpPartitionType::Share, share),
            (EdpPartitionType::Encrypt, encrypt),
        ],
        OfficialPartitionMode::BootShareCombined => vec![
            (EdpPartitionType::Share, share),
            (EdpPartitionType::Encrypt, encrypt),
        ],
        OfficialPartitionMode::WholeDiskEncrypted => {
            let encrypted = encrypt
                .checked_sub(WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES)
                .ok_or("whole-disk encrypted size is smaller than 0x7E00 compatibility entry")?;
            vec![
                (
                    EdpPartitionType::Boot,
                    WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES,
                ),
                (EdpPartitionType::Encrypt, encrypted),
            ]
        }
        OfficialPartitionMode::IntranetExtranetDualPartition => vec![
            (EdpPartitionType::Boot, boot),
            (EdpPartitionType::Share, share),
        ],
    };

    let mut start = OFFICIAL_PARTITION_START_SECTOR;
    let mut out = Vec::with_capacity(logical.len());
    for (partition_type, size_bytes) in logical {
        if size_bytes == 0 {
            return Err(format!(
                "official mode {} contains a zero-sized {} partition",
                mode as u8,
                partition_type.role()
            ));
        }
        if size_bytes % sector_size != 0 {
            return Err(format!(
                "{} partition size {size_bytes} is not aligned to sector size {sector_size}",
                partition_type.role()
            ));
        }
        let sectors = size_bytes / sector_size;
        let geometry = OfficialPartitionGeometry {
            partition_type,
            start_sector: start,
            sector_size,
            size_bytes,
        };
        start = start
            .checked_add(sectors)
            .ok_or("official partition layout overflows sector address space")?;
        out.push(geometry);
    }
    Ok(out)
}
