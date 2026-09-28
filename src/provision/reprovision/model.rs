use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapacityInputMode {
    Quick,
    Exact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuickCapacityUnit {
    MiB,
    GiB,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapacitySource {
    SystemDefault,
    ExistingPartition,
    ExistingBoundary,
    UserEdited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapacityInput {
    sectors: u64,
    mode: CapacityInputMode,
    source: CapacitySource,
}

impl CapacityInput {
    pub fn from_quick(
        value: u64,
        unit: QuickCapacityUnit,
        source: CapacitySource,
    ) -> Result<Self, String> {
        let sectors_per_unit = match unit {
            QuickCapacityUnit::MiB => 2048,
            QuickCapacityUnit::GiB => 2_097_152,
        };
        let sectors = value
            .checked_mul(sectors_per_unit)
            .ok_or("capacity exceeds sector address range")?;
        if sectors == 0 {
            return Err("partition capacity must be non-zero".into());
        }
        Ok(Self {
            sectors,
            mode: CapacityInputMode::Quick,
            source,
        })
    }

    pub fn from_exact(sectors: u64, source: CapacitySource) -> Result<Self, String> {
        if sectors == 0 {
            return Err("partition capacity must be non-zero".into());
        }
        Ok(Self {
            sectors,
            mode: CapacityInputMode::Exact,
            source,
        })
    }

    pub fn from_quick_sectors(sectors: u64, source: CapacitySource) -> Result<Self, String> {
        if sectors == 0 {
            return Err("partition capacity must be non-zero".into());
        }
        Ok(Self {
            sectors,
            mode: CapacityInputMode::Quick,
            source,
        })
    }

    pub const fn sectors(self) -> u64 {
        self.sectors
    }
    pub const fn mode(self) -> CapacityInputMode {
        self.mode
    }
    pub const fn source(self) -> CapacitySource {
        self.source
    }
    pub fn whole_mib(self) -> Option<u64> {
        self.sectors
            .is_multiple_of(2048)
            .then_some(self.sectors / 2048)
    }
    pub const fn to_exact(self) -> Self {
        Self {
            mode: CapacityInputMode::Exact,
            ..self
        }
    }

    /// Changing the presentation alone must never round the canonical sector value.
    pub fn to_quick(self, unit: QuickCapacityUnit) -> Result<Self, String> {
        let divisor = match unit {
            QuickCapacityUnit::MiB => 2048,
            QuickCapacityUnit::GiB => 2_097_152,
        };
        if !self.sectors.is_multiple_of(divisor) {
            return Err(
                "exact capacity is not a whole quick unit; explicit rounded value required".into(),
            );
        }
        Ok(Self {
            mode: CapacityInputMode::Quick,
            ..self
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExistingPartition {
    pub role: PartitionRole,
    pub partition_type: EdpPartitionType,
    pub start_lba: u64,
    pub sector_count: u64,
    pub physically_encrypted: bool,
    pub filesystem: Option<OfficialFilesystemFormat>,
}

impl ExistingPartition {
    pub const fn as_target(self) -> TargetPartitionGeometry {
        TargetPartitionGeometry {
            role: self.role,
            partition_type: self.partition_type,
            start_lba: self.start_lba,
            sector_count: self.sector_count,
            physically_encrypted: self.physically_encrypted,
            filesystem: self.filesystem,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExistingProvisionProfile {
    pub source_mode: OfficialPartitionMode,
    pub partitions: Vec<ExistingPartition>,
}

impl ExistingProvisionProfile {
    pub fn partition(&self, role: PartitionRole) -> Option<&ExistingPartition> {
        self.partitions.iter().find(|part| part.role == role)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetPartitionGeometry {
    pub role: PartitionRole,
    pub partition_type: EdpPartitionType,
    pub start_lba: u64,
    pub sector_count: u64,
    pub physically_encrypted: bool,
    pub filesystem: Option<OfficialFilesystemFormat>,
}

impl TargetPartitionGeometry {
    pub fn end_lba(self) -> Result<u64, String> {
        self.start_lba
            .checked_add(self.sector_count)
            .ok_or_else(|| "partition end LBA overflows".into())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProvisionTarget {
    #[default]
    Plain,
    Official(OfficialPartitionMode),
}

impl ProvisionTarget {
    pub const OFFICIAL: [Self; 4] = [
        Self::Official(OfficialPartitionMode::DefaultThreePartition),
        Self::Official(OfficialPartitionMode::BootShareCombined),
        Self::Official(OfficialPartitionMode::WholeDiskEncrypted),
        Self::Official(OfficialPartitionMode::IntranetExtranetDualPartition),
    ];

    pub const fn official_mode(self) -> Option<OfficialPartitionMode> {
        match self {
            Self::Plain => None,
            Self::Official(mode) => Some(mode),
        }
    }

    pub const fn from_mode_number(mode: u8) -> Option<Self> {
        match mode {
            0 => Some(Self::OFFICIAL[0]),
            1 => Some(Self::OFFICIAL[1]),
            2 => Some(Self::OFFICIAL[2]),
            3 => Some(Self::OFFICIAL[3]),
            _ => None,
        }
    }

    pub const fn mode_number(self) -> Option<u8> {
        match self {
            Self::Plain => None,
            Self::Official(OfficialPartitionMode::DefaultThreePartition) => Some(0),
            Self::Official(OfficialPartitionMode::BootShareCombined) => Some(1),
            Self::Official(OfficialPartitionMode::WholeDiskEncrypted) => Some(2),
            Self::Official(OfficialPartitionMode::IntranetExtranetDualPartition) => Some(3),
        }
    }

    pub const fn full_name(self) -> &'static str {
        match self {
            Self::Plain => "普通盘",
            Self::Official(OfficialPartitionMode::DefaultThreePartition) => "模式0 · 缺省三分区",
            Self::Official(OfficialPartitionMode::BootShareCombined) => {
                "模式1 · 启动区和交换区二合一"
            }
            Self::Official(OfficialPartitionMode::WholeDiskEncrypted) => "模式2 · 整盘加密",
            Self::Official(OfficialPartitionMode::IntranetExtranetDualPartition) => {
                "模式3 · 内外网通用双分区"
            }
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Plain => "标准 MBR 普通盘",
            Self::Official(OfficialPartitionMode::DefaultThreePartition) => {
                "启动区 + 交换区 + 保密区"
            }
            Self::Official(OfficialPartitionMode::BootShareCombined) => {
                "启动/交换二合一区 + 保密区"
            }
            Self::Official(OfficialPartitionMode::WholeDiskEncrypted) => {
                "兼容保留区 + 保密区（整盘加密）"
            }
            Self::Official(OfficialPartitionMode::IntranetExtranetDualPartition) => {
                "启动区 + 交换区（内外网双分区）"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DiskProvisionKind {
    #[default]
    Plain,
    Mode0,
    Mode1,
    Mode2,
    Mode3,
}

impl DiskProvisionKind {
    pub const fn target(self) -> ProvisionTarget {
        match self {
            Self::Plain => ProvisionTarget::Plain,
            Self::Mode0 => ProvisionTarget::OFFICIAL[0],
            Self::Mode1 => ProvisionTarget::OFFICIAL[1],
            Self::Mode2 => ProvisionTarget::OFFICIAL[2],
            Self::Mode3 => ProvisionTarget::OFFICIAL[3],
        }
    }

    pub const fn official_mode(self) -> Option<OfficialPartitionMode> {
        self.target().official_mode()
    }
    pub const fn from_mode(mode: OfficialPartitionMode) -> Self {
        match mode {
            OfficialPartitionMode::DefaultThreePartition => Self::Mode0,
            OfficialPartitionMode::BootShareCombined => Self::Mode1,
            OfficialPartitionMode::WholeDiskEncrypted => Self::Mode2,
            OfficialPartitionMode::IntranetExtranetDualPartition => Self::Mode3,
        }
    }

    pub const fn full_name(self) -> &'static str {
        self.target().full_name()
    }

    pub const fn short_name(self) -> &'static str {
        match self {
            Self::Plain => "普通盘",
            Self::Mode0 => "mode0 · 缺省三分区",
            Self::Mode1 => "mode1 · 二合一",
            Self::Mode2 => "mode2 · 整盘加密",
            Self::Mode3 => "mode3 · 内外网双分区",
        }
    }

    /// Detect an official EDP mode from LBA7/LBA12 metadata.
    ///
    /// This detector deliberately never returns Plain: failure to recognize a
    /// valid official EDP profile is None. Plain media must be confirmed by
    /// the independent physical partition-table classifier.
    pub fn from_metadata(image: &[u8], device_id: &str) -> Option<Self> {
        let lba7 = image.get(7 * SECTOR..8 * SECTOR)?;
        let lba12 = image.get(12 * SECTOR..13 * SECTOR)?;
        Self::from_sectors(lba7, lba12, device_id)
    }

    /// Detect an official EDP mode from exactly one LBA7 and one LBA12 sector.
    ///
    /// Returns None for malformed, conflicting, unsupported, or non-EDP
    /// metadata. Plain is not an EDP decode result.
    pub fn from_sectors(lba7: &[u8], lba12: &[u8], device_id: &str) -> Option<Self> {
        if lba7.len() != SECTOR || lba12.len() != SECTOR || device_id.is_empty() {
            return None;
        }
        let crc = crc32_bare(device_id.as_bytes());
        let decoded7 = xor_rolling(lba7, (crc & 0xffff) ^ (crc >> 16));
        let decoded12 = a6b0_full(lba12, &crc.to_le_bytes(), 0);
        if decoded7.get(..4) != Some(b"EDPF") || decoded12.get(..4) != Some(b"EDPF") {
            return None;
        }
        let count = u32::from_le_bytes(decoded12[8..12].try_into().unwrap()) as usize;
        if !(2..=3).contains(&count) {
            return None;
        }
        let mut types = Vec::with_capacity(count);
        for index in 0..count {
            let e7 = EdpfEntry64::parse(
                decoded7[index * 0x40..(index + 1) * 0x40]
                    .try_into()
                    .unwrap(),
            )
            .ok()?;
            let e12 = EdpfEntry96::parse(
                decoded12[index * 0x60..(index + 1) * 0x60]
                    .try_into()
                    .unwrap(),
            )
            .ok()?;
            if e7.partition_count as usize != count
                || e12.partition_count as usize != count
                || e7.partition_type != e12.partition_type
                || e7.sector_size != 512
                || e12.sector_size != 512
                || e12.partition_size == 0
                || !e12.partition_size.is_multiple_of(512)
            {
                return None;
            }
            if index == 0
                && (e7.start_sector != e12.start_sector || e7.partition_size != e12.partition_size)
            {
                return None;
            }
            types.push(e12.partition_type);
        }
        OfficialPartitionMode::from_partition_types(&types).map(Self::from_mode)
    }
}
