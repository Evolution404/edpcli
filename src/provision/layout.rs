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
pub const DEFAULT_MODE0_BOOT_SECTORS: u64 = 20_417;
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

pub const fn visible_mbr_partition_type(
    mode: OfficialPartitionMode,
    front_filesystem: OfficialFilesystemFormat,
) -> u8 {
    if matches!(mode, OfficialPartitionMode::WholeDiskEncrypted) {
        return official_mbr_partition_type(mode);
    }
    match front_filesystem {
        OfficialFilesystemFormat::Fat16 => 0x0e,
        OfficialFilesystemFormat::Fat32 => 0x0c,
        OfficialFilesystemFormat::ExFat | OfficialFilesystemFormat::Ntfs => 0x07,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OfficialPartitionFilesystems {
    pub boot: OfficialFilesystemFormat,
    pub share: OfficialFilesystemFormat,
    pub encrypt: OfficialFilesystemFormat,
}

impl OfficialPartitionFilesystems {
    pub const fn defaults() -> Self {
        Self {
            boot: OfficialFilesystemFormat::Fat16,
            share: OfficialFilesystemFormat::ExFat,
            encrypt: OfficialFilesystemFormat::ExFat,
        }
    }

    pub const fn all(format: OfficialFilesystemFormat) -> Self {
        Self {
            boot: format,
            share: format,
            encrypt: format,
        }
    }

    pub const fn for_role(self, role: PartitionRole) -> Option<OfficialFilesystemFormat> {
        match role {
            PartitionRole::Boot => Some(self.boot),
            PartitionRole::Share | PartitionRole::BootShareCombined => Some(self.share),
            PartitionRole::Encrypt => Some(self.encrypt),
            PartitionRole::CompatibilityReserve => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OfficialPartitionSizes {
    pub boot_mib: u64,
    pub boot_sectors: Option<u64>,
    pub share_mib: u64,
    pub share_sectors: Option<u64>,
    pub encrypt_mib: u64,
    pub encrypt_sectors: Option<u64>,
}

impl OfficialPartitionSizes {
    pub const fn new(boot_mib: u64, share_mib: u64, encrypt_mib: u64) -> Self {
        Self {
            boot_mib,
            boot_sectors: None,
            share_mib,
            share_sectors: None,
            encrypt_mib,
            encrypt_sectors: None,
        }
    }

    pub const fn with_boot_sectors(mut self, boot_sectors: u64) -> Self {
        self.boot_sectors = Some(boot_sectors);
        self
    }

    pub const fn with_share_sectors(mut self, share_sectors: u64) -> Self {
        self.share_sectors = Some(share_sectors);
        self
    }

    pub const fn with_encrypt_sectors(mut self, encrypt_sectors: u64) -> Self {
        self.encrypt_sectors = Some(encrypt_sectors);
        self
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
    /// Compatibility setting for the explicit legacy exFAT builder. New disk
    /// provisioning uses `filesystems` for every partition and for the MBR.
    pub filesystem_format: OfficialFilesystemFormat,
    pub filesystems: OfficialPartitionFilesystems,
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
        let mut plan = Self::new_with_filesystem(
            mode,
            sizes,
            OfficialFilesystemFormat::ExFat,
            lba7_compatibility_extent,
            lba7_key_material,
            lba12_key_material,
        )?;
        plan.filesystems = OfficialPartitionFilesystems::defaults();
        Ok(plan)
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
            filesystems: OfficialPartitionFilesystems::all(filesystem_format),
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

    pub fn format_targets(self) -> Result<Vec<PartitionFormatTarget>, String> {
        official_format_targets_with_filesystems(self.mode, self.sizes, 512, self.filesystems)
    }

    pub fn with_filesystems(mut self, filesystems: OfficialPartitionFilesystems) -> Self {
        self.filesystems = filesystems;
        self
    }

    pub fn visible_mbr_partition_type(self) -> Result<u8, String> {
        let front = self
            .format_targets()?
            .into_iter()
            .next()
            .ok_or("empty partition layout")?;
        front
            .visible_mbr_type
            .ok_or("front partition has no visible MBR type".into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PartitionRole {
    Boot,
    Share,
    Encrypt,
    BootShareCombined,
    CompatibilityReserve,
}

impl PartitionRole {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Boot => "启动区",
            Self::Share => "交换区",
            Self::Encrypt => "保密区",
            Self::BootShareCombined => "启动/交换区",
            Self::CompatibilityReserve => "0x7E00兼容保留区",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PartitionFormatTarget {
    pub role: PartitionRole,
    pub geometry: OfficialPartitionGeometry,
    pub physically_encrypted: bool,
    pub format_capable: bool,
    pub filesystem: Option<OfficialFilesystemFormat>,
    pub visible_mbr_type: Option<u8>,
}

pub const fn physical_partition_encryption(
    mode: OfficialPartitionMode,
    index: usize,
    partition_type: EdpPartitionType,
) -> bool {
    match partition_type {
        EdpPartitionType::Boot => false,
        EdpPartitionType::Share
            if matches!(mode, OfficialPartitionMode::BootShareCombined) && index == 0 =>
        {
            false
        }
        EdpPartitionType::Share | EdpPartitionType::Encrypt => true,
    }
}

pub fn official_format_targets(
    mode: OfficialPartitionMode,
    sizes: OfficialPartitionSizes,
    sector_size: u64,
) -> Result<Vec<PartitionFormatTarget>, String> {
    official_format_targets_with_filesystems(
        mode,
        sizes,
        sector_size,
        OfficialPartitionFilesystems::defaults(),
    )
}

pub fn official_format_targets_with_filesystems(
    mode: OfficialPartitionMode,
    sizes: OfficialPartitionSizes,
    sector_size: u64,
    filesystems: OfficialPartitionFilesystems,
) -> Result<Vec<PartitionFormatTarget>, String> {
    let geometries = build_official_partition_layout(mode, sizes, sector_size)?;
    Ok(geometries
        .into_iter()
        .enumerate()
        .map(|(index, geometry)| {
            let role = match (mode, index, geometry.partition_type) {
                (OfficialPartitionMode::WholeDiskEncrypted, 0, EdpPartitionType::Boot) => {
                    PartitionRole::CompatibilityReserve
                }
                (OfficialPartitionMode::BootShareCombined, 0, EdpPartitionType::Share) => {
                    PartitionRole::BootShareCombined
                }
                (_, _, EdpPartitionType::Boot) => PartitionRole::Boot,
                (_, _, EdpPartitionType::Share) => PartitionRole::Share,
                (_, _, EdpPartitionType::Encrypt) => PartitionRole::Encrypt,
            };
            PartitionFormatTarget {
                role,
                geometry,
                physically_encrypted: physical_partition_encryption(
                    mode,
                    index,
                    geometry.partition_type,
                ),
                format_capable: role != PartitionRole::CompatibilityReserve,
                filesystem: filesystems.for_role(role),
                visible_mbr_type: if index == 0 {
                    Some(match filesystems.for_role(role) {
                        Some(format) => visible_mbr_partition_type(mode, format),
                        None => official_mbr_partition_type(mode),
                    })
                } else {
                    None
                },
            }
        })
        .collect())
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

    let boot = match sizes.boot_sectors {
        Some(sectors) => sectors
            .checked_mul(sector_size)
            .ok_or("boot partition sector count overflows bytes")?,
        None => mib_bytes(sizes.boot_mib)?,
    };
    let share = match sizes.share_sectors {
        Some(sectors) => sectors
            .checked_mul(sector_size)
            .ok_or("share partition sector count overflows bytes")?,
        None => mib_bytes(sizes.share_mib)?,
    };
    let encrypt = match sizes.encrypt_sectors {
        Some(sectors) => sectors
            .checked_mul(sector_size)
            .ok_or("encrypt partition sector count overflows bytes")?,
        None => mib_bytes(sizes.encrypt_mib)?,
    };
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
            let encrypted = if sizes.encrypt_sectors.is_some() {
                encrypt
            } else {
                encrypt
                    .checked_sub(WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES)
                    .ok_or("whole-disk encrypted size is smaller than 0x7E00 compatibility entry")?
            };
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
