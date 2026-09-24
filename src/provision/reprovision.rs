//! Sector-based, device-independent defaults for a target provisioning mode.

use crate::protocol::edpf::EdpPartitionType;
use crate::{
    common::SECTOR,
    crypto::{a6b0_full, crc32_bare, xor_rolling},
    protocol::edpf::{EdpfEntry64, EdpfEntry96},
};

use super::{
    OfficialFilesystemFormat, OfficialPartitionMode, PartitionRole, DEFAULT_MODE0_BOOT_SECTORS,
    OFFICIAL_PARTITION_START_SECTOR, WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES,
};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PartitionAction {
    PreserveExact,
    Rebuild,
}

/// Geometry alone is necessary but not sufficient for actual preservation. The
/// writer must additionally require verified per-partition key material.
pub fn decide_partition_action(
    source: Option<&ExistingPartition>,
    target: &TargetPartitionGeometry,
) -> PartitionAction {
    match source {
        Some(source)
            if source.role == target.role
                && source.partition_type == target.partition_type
                && source.start_lba == target.start_lba
                && source.sector_count == target.sector_count
                && source.physically_encrypted == target.physically_encrypted
                && source.filesystem.is_some()
                && source.filesystem == target.filesystem
                && source.role != PartitionRole::CompatibilityReserve =>
        {
            PartitionAction::PreserveExact
        }
        _ => PartitionAction::Rebuild,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvisionPrefill {
    pub mode: OfficialPartitionMode,
    pub boot: Option<CapacityInput>,
    pub share: Option<CapacityInput>,
    pub encrypt: Option<CapacityInput>,
    /// Start positions are independent of capacity edits. Existing data partitions
    /// remain anchored until the user explicitly changes their own geometry.
    pub boot_start_lba: Option<u64>,
    pub share_start_lba: Option<u64>,
    pub encrypt_start_lba: Option<u64>,
    pub usable_end_lba: u64,
}

impl ProvisionPrefill {
    pub fn target_partitions(
        &self,
        sector_size: u64,
    ) -> Result<Vec<TargetPartitionGeometry>, String> {
        if sector_size != 512 {
            return Err("only 512-byte sector targets are supported".into());
        }
        let mut out = Vec::new();
        let mut push =
            |role, partition_type, start_lba, capacity: CapacityInput, encrypted, filesystem| {
                out.push(TargetPartitionGeometry {
                    role,
                    partition_type,
                    start_lba,
                    sector_count: capacity.sectors(),
                    physically_encrypted: encrypted,
                    filesystem,
                });
            };
        match self.mode {
            OfficialPartitionMode::DefaultThreePartition => {
                push(
                    PartitionRole::Boot,
                    EdpPartitionType::Boot,
                    self.boot_start_lba.ok_or("missing boot start")?,
                    self.boot.ok_or("missing boot capacity")?,
                    false,
                    Some(OfficialFilesystemFormat::Fat16),
                );
                push(
                    PartitionRole::Share,
                    EdpPartitionType::Share,
                    self.share_start_lba.ok_or("missing share start")?,
                    self.share.ok_or("missing share capacity")?,
                    true,
                    Some(OfficialFilesystemFormat::ExFat),
                );
                push(
                    PartitionRole::Encrypt,
                    EdpPartitionType::Encrypt,
                    self.encrypt_start_lba.ok_or("missing encrypt start")?,
                    self.encrypt.ok_or("missing encrypt capacity")?,
                    true,
                    Some(OfficialFilesystemFormat::ExFat),
                );
            }
            OfficialPartitionMode::BootShareCombined => {
                push(
                    PartitionRole::BootShareCombined,
                    EdpPartitionType::Share,
                    self.share_start_lba.ok_or("missing combined start")?,
                    self.share.ok_or("missing combined capacity")?,
                    false,
                    Some(OfficialFilesystemFormat::ExFat),
                );
                push(
                    PartitionRole::Encrypt,
                    EdpPartitionType::Encrypt,
                    self.encrypt_start_lba.ok_or("missing encrypt start")?,
                    self.encrypt.ok_or("missing encrypt capacity")?,
                    true,
                    Some(OfficialFilesystemFormat::ExFat),
                );
            }
            OfficialPartitionMode::WholeDiskEncrypted => {
                push(
                    PartitionRole::CompatibilityReserve,
                    EdpPartitionType::Boot,
                    self.boot_start_lba.ok_or("missing reserve start")?,
                    self.boot.ok_or("missing reserve capacity")?,
                    false,
                    None,
                );
                push(
                    PartitionRole::Encrypt,
                    EdpPartitionType::Encrypt,
                    self.encrypt_start_lba.ok_or("missing encrypt start")?,
                    self.encrypt.ok_or("missing encrypt capacity")?,
                    true,
                    Some(OfficialFilesystemFormat::ExFat),
                );
            }
            OfficialPartitionMode::IntranetExtranetDualPartition => {
                push(
                    PartitionRole::Boot,
                    EdpPartitionType::Boot,
                    self.boot_start_lba.ok_or("missing boot start")?,
                    self.boot.ok_or("missing boot capacity")?,
                    false,
                    Some(OfficialFilesystemFormat::Fat16),
                );
                push(
                    PartitionRole::Share,
                    EdpPartitionType::Share,
                    self.share_start_lba.ok_or("missing share start")?,
                    self.share.ok_or("missing share capacity")?,
                    true,
                    Some(OfficialFilesystemFormat::ExFat),
                );
            }
        }
        validate_target_geometry(&out, self.usable_end_lba)?;
        Ok(out)
    }
}

pub fn validate_target_geometry(
    parts: &[TargetPartitionGeometry],
    usable_end_lba: u64,
) -> Result<u64, String> {
    let mut ordered = parts.to_vec();
    ordered.sort_by_key(|part| part.start_lba);
    let mut cursor = OFFICIAL_PARTITION_START_SECTOR;
    let mut gaps = 0u64;
    for part in ordered {
        if part.sector_count == 0 {
            return Err(format!("zero-sized partition at LBA{}", part.start_lba));
        }
        if part.start_lba < cursor {
            return Err(format!(
                "partition overlap at LBA{} by {} sectors",
                part.start_lba,
                cursor - part.start_lba
            ));
        }
        gaps = gaps
            .checked_add(part.start_lba - cursor)
            .ok_or("gap count overflows")?;
        cursor = part.end_lba()?;
        if cursor > usable_end_lba {
            return Err(format!(
                "partition exceeds usable LBA boundary by {} sectors",
                cursor - usable_end_lba
            ));
        }
    }
    gaps.checked_add(usable_end_lba.saturating_sub(cursor))
        .ok_or_else(|| "gap count overflows".into())
}

fn exact(value: u64, source: CapacitySource) -> Result<CapacityInput, String> {
    CapacityInput::from_exact(value, source)
}
fn source_capacity(source: Option<&ExistingPartition>) -> Result<Option<CapacityInput>, String> {
    source
        .map(|part| exact(part.sector_count, CapacitySource::ExistingPartition))
        .transpose()
}

/// Compute editable defaults. Existing same-semantics partitions retain their
/// own starts, even when a preceding newly-built partition leaves a gap.
pub fn prefill_for_target_mode(
    source: Option<&ExistingProvisionProfile>,
    mode: OfficialPartitionMode,
    usable_end_lba: u64,
    sector_size: u64,
) -> Result<ProvisionPrefill, String> {
    if sector_size != 512 {
        return Err("only 512-byte sector targets are supported".into());
    }
    let first = OFFICIAL_PARTITION_START_SECTOR;
    if usable_end_lba <= first {
        return Err("no usable partition sectors".into());
    }
    let boot_old = source.and_then(|s| s.partition(PartitionRole::Boot));
    let share_old = source.and_then(|s| s.partition(PartitionRole::Share));
    let combined_old = source.and_then(|s| s.partition(PartitionRole::BootShareCombined));
    let encrypt_old = source.and_then(|s| s.partition(PartitionRole::Encrypt));
    let boot = match mode {
        OfficialPartitionMode::DefaultThreePartition
        | OfficialPartitionMode::IntranetExtranetDualPartition => {
            source_capacity(boot_old)?.or(Some(exact(
                DEFAULT_MODE0_BOOT_SECTORS,
                CapacitySource::SystemDefault,
            )?))
        }
        OfficialPartitionMode::WholeDiskEncrypted => Some(exact(
            WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES / sector_size,
            CapacitySource::SystemDefault,
        )?),
        OfficialPartitionMode::BootShareCombined => None,
    };
    let boot_start = boot.map(|_| boot_old.map_or(first, |part| part.start_lba));
    let boot_end = boot_start
        .zip(boot)
        .map(|(start, size)| {
            start
                .checked_add(size.sectors())
                .ok_or("boot end overflows")
        })
        .transpose()?
        .unwrap_or(first);
    let encrypt = if matches!(mode, OfficialPartitionMode::IntranetExtranetDualPartition) {
        None
    } else {
        source_capacity(encrypt_old)?.or(Some(exact(2_097_152, CapacitySource::SystemDefault)?))
    };
    let encrypt_start = if encrypt.is_none() {
        None
    } else if let Some(old) = encrypt_old {
        Some(old.start_lba)
    } else if matches!(mode, OfficialPartitionMode::WholeDiskEncrypted) {
        Some(boot_end)
    } else {
        None
    };
    let share_role_source = if matches!(mode, OfficialPartitionMode::BootShareCombined) {
        combined_old
    } else {
        share_old
    };
    let share_start = if matches!(mode, OfficialPartitionMode::WholeDiskEncrypted) {
        None
    } else {
        Some(share_role_source.map_or(
            if matches!(mode, OfficialPartitionMode::BootShareCombined) {
                first
            } else {
                boot_end
            },
            |part| part.start_lba,
        ))
    };
    let share = if matches!(mode, OfficialPartitionMode::WholeDiskEncrypted) {
        None
    } else if let Some(old) = share_role_source {
        if mode == OfficialPartitionMode::DefaultThreePartition && encrypt_old.is_none() {
            let space_after =
                usable_end_lba.saturating_sub(old.start_lba.saturating_add(old.sector_count));
            let new_encrypt = encrypt.ok_or("missing encrypt capacity")?.sectors();
            if space_after < new_encrypt {
                Some(exact(
                    usable_end_lba
                        .checked_sub(old.start_lba)
                        .and_then(|v| v.checked_sub(new_encrypt))
                        .ok_or("no room for new encrypt partition")?,
                    CapacitySource::ExistingBoundary,
                )?)
            } else {
                source_capacity(Some(old))?
            }
        } else {
            source_capacity(Some(old))?
        }
    } else {
        let boundary = encrypt_start.unwrap_or_else(|| {
            usable_end_lba.saturating_sub(encrypt.map_or(0, CapacityInput::sectors))
        });
        let start = share_start.ok_or("missing share start")?;
        let available = boundary
            .checked_sub(start)
            .ok_or("no room before anchored encrypt partition")?;
        if source.is_some() {
            Some(exact(available, CapacitySource::ExistingBoundary)?)
        } else {
            Some(CapacityInput::from_quick(
                available / 2048,
                QuickCapacityUnit::MiB,
                CapacitySource::SystemDefault,
            )?)
        }
    };
    let encrypt_start = encrypt_start.or_else(|| {
        if encrypt.is_some() {
            share_start
                .zip(share)
                .and_then(|(start, size)| start.checked_add(size.sectors()))
        } else {
            None
        }
    });
    let encrypt =
        if matches!(mode, OfficialPartitionMode::WholeDiskEncrypted) && encrypt_old.is_none() {
            Some(exact(
                usable_end_lba
                    .checked_sub(encrypt_start.ok_or("missing encrypt start")?)
                    .ok_or("no room for encrypt partition")?,
                CapacitySource::SystemDefault,
            )?)
        } else {
            encrypt
        };
    let result = ProvisionPrefill {
        mode,
        boot,
        share,
        encrypt,
        boot_start_lba: boot_start,
        share_start_lba: share_start,
        encrypt_start_lba: encrypt_start,
        usable_end_lba,
    };
    result.target_partitions(sector_size)?;
    Ok(result)
}

/// Decoded, partition-owned protocol evidence. The two entries deliberately
/// remain separate: LBA7's later entries point at the compatibility extent,
/// while LBA12 holds the actual data geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExistingPartitionRecord {
    pub lba7: EdpfEntry64,
    pub lba12: EdpfEntry96,
}

impl ExistingPartitionRecord {
    pub fn lba7_key_material(self) -> super::LegacyLba7KeyMaterial {
        super::LegacyLba7KeyMaterial {
            user_key_crc: self.lba7.user_key_crc,
            file_key_crc: self.lba7.file_key_crc,
            wrapped_file_key: self.lba7.encrypted_file_key,
        }
    }

    pub fn lba12_key_material(self) -> Result<super::ProvisionKeyMaterial, String> {
        let encrypt_mode = super::FileKeyWrapMode::from_raw(self.lba12.encrypt_mode)
            .ok_or("existing partition uses unsupported FileKey wrap mode")?;
        Ok(super::ProvisionKeyMaterial {
            user_key_crc: self.lba12.user_key_crc,
            file_key_crc: self.lba12.file_key_crc,
            wrapped_file_key: self.lba12.encrypted_file_key,
            encrypt_mode,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedExistingProvision {
    pub profile: ExistingProvisionProfile,
    pub records: Vec<ExistingPartitionRecord>,
    pub device_id: String,
    pub total_sectors: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DiskProvisionKind {
    #[default]
    Plain,
    Mode0,
    Mode1,
    Mode2,
    Mode3,
}

impl DiskProvisionKind {
    pub const fn from_mode(mode: OfficialPartitionMode) -> Self {
        match mode {
            OfficialPartitionMode::DefaultThreePartition => Self::Mode0,
            OfficialPartitionMode::BootShareCombined => Self::Mode1,
            OfficialPartitionMode::WholeDiskEncrypted => Self::Mode2,
            OfficialPartitionMode::IntranetExtranetDualPartition => Self::Mode3,
        }
    }

    pub const fn full_name(self) -> &'static str {
        match self {
            Self::Plain => "普通盘",
            Self::Mode0 => "模式0 · 缺省三分区",
            Self::Mode1 => "模式1 · 启动区和交换区二合一",
            Self::Mode2 => "模式2 · 整盘加密",
            Self::Mode3 => "模式3 · 内外网通用双分区",
        }
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

    pub fn from_metadata(image: &[u8], device_id: &str) -> Self {
        let Some(lba7) = image.get(7 * SECTOR..8 * SECTOR) else {
            return Self::Plain;
        };
        let Some(lba12) = image.get(12 * SECTOR..13 * SECTOR) else {
            return Self::Plain;
        };
        Self::from_sectors(lba7, lba12, device_id)
    }

    pub fn from_sectors(lba7: &[u8], lba12: &[u8], device_id: &str) -> Self {
        if lba7.len() != SECTOR || lba12.len() != SECTOR || device_id.is_empty() {
            return Self::Plain;
        }
        let crc = crc32_bare(device_id.as_bytes());
        let decoded7 = xor_rolling(lba7, (crc & 0xffff) ^ (crc >> 16));
        let decoded12 = a6b0_full(lba12, &crc.to_le_bytes(), 0);
        if decoded7.get(..4) != Some(b"EDPF") || decoded12.get(..4) != Some(b"EDPF") {
            return Self::Plain;
        }
        let count = u32::from_le_bytes(decoded12[8..12].try_into().unwrap()) as usize;
        if !(2..=3).contains(&count) {
            return Self::Plain;
        }
        let mut types = Vec::with_capacity(count);
        for index in 0..count {
            let Ok(e7) = EdpfEntry64::parse(
                decoded7[index * 0x40..(index + 1) * 0x40]
                    .try_into()
                    .unwrap(),
            ) else {
                return Self::Plain;
            };
            let Ok(e12) = EdpfEntry96::parse(
                decoded12[index * 0x60..(index + 1) * 0x60]
                    .try_into()
                    .unwrap(),
            ) else {
                return Self::Plain;
            };
            if e7.partition_count as usize != count
                || e12.partition_count as usize != count
                || e7.partition_type != e12.partition_type
                || e7.sector_size != 512
                || e12.sector_size != 512
                || e12.partition_size == 0
                || !e12.partition_size.is_multiple_of(512)
            {
                return Self::Plain;
            }
            if index == 0
                && (e7.start_sector != e12.start_sector || e7.partition_size != e12.partition_size)
            {
                return Self::Plain;
            }
            types.push(e12.partition_type);
        }
        OfficialPartitionMode::from_partition_types(&types)
            .map(Self::from_mode)
            .unwrap_or(Self::Plain)
    }
}

impl ParsedExistingProvision {
    pub fn record(&self, role: PartitionRole) -> Option<&ExistingPartitionRecord> {
        self.profile
            .partitions
            .iter()
            .position(|part| part.role == role)
            .map(|index| &self.records[index])
    }
}

/// Classify and decode a complete metadata image without guessing unknown
/// formats. `None` means no paired, valid EDPF magic; a partially recognizable
/// or inconsistent registration is an error and must not be treated as plain.
pub fn parse_existing_provision(
    image: &super::ProvisionImage,
    device_id: &str,
    total_sectors: u64,
) -> Result<Option<ParsedExistingProvision>, String> {
    if device_id.is_empty() {
        return Err("device_id is required to decode existing EDPF".into());
    }
    let bytes = image.as_bytes();
    let crc = crc32_bare(device_id.as_bytes());
    let raw7 = &bytes[7 * SECTOR..8 * SECTOR];
    let raw12 = &bytes[12 * SECTOR..13 * SECTOR];
    let plain7 = xor_rolling(raw7, (crc & 0xffff) ^ (crc >> 16));
    let plain12 = a6b0_full(raw12, &crc.to_le_bytes(), 0);
    let magic7 = plain7.get(..4) == Some(b"EDPF");
    let magic12 = plain12.get(..4) == Some(b"EDPF");
    if !magic7 && !magic12 {
        return Ok(None);
    }
    if !magic7 || !magic12 {
        return Err("LBA7/LBA12 EDPF registration disagrees".into());
    }
    let count = u32::from_le_bytes(plain12[8..12].try_into().unwrap()) as usize;
    if !(2..=3).contains(&count) {
        return Err(format!("unsupported EDPF partition count {count}"));
    }
    let mut records = Vec::with_capacity(count);
    let mut types = Vec::with_capacity(count);
    for index in 0..count {
        let lba7 = EdpfEntry64::parse(plain7[index * 0x40..(index + 1) * 0x40].try_into().unwrap())
            .map_err(|error| format!("LBA7 entry{index}: {error}"))?;
        let lba12 = EdpfEntry96::parse(
            plain12[index * 0x60..(index + 1) * 0x60]
                .try_into()
                .unwrap(),
        )
        .map_err(|error| format!("LBA12 entry{index}: {error}"))?;
        if lba7.partition_count as usize != count
            || lba12.partition_count as usize != count
            || lba7.partition_type != lba12.partition_type
            || lba7.need_encrypt != lba12.need_encrypt
            || lba7.sector_size != SECTOR as u64
            || lba12.sector_size != SECTOR as u64
        {
            return Err(format!(
                "LBA7/LBA12 entry{index} has inconsistent type, count, flags, or sector size"
            ));
        }
        if lba12.partition_size == 0 || !lba12.partition_size.is_multiple_of(SECTOR as u64) {
            return Err(format!(
                "LBA12 entry{index} size is empty or not sector aligned"
            ));
        }
        let sectors = lba12.partition_size / SECTOR as u64;
        if lba12.start_sector < OFFICIAL_PARTITION_START_SECTOR
            || lba12
                .start_sector
                .checked_add(sectors)
                .is_none_or(|end| end > total_sectors)
        {
            return Err(format!("LBA12 entry{index} is outside target disk"));
        }
        if index == 0
            && (lba7.start_sector != lba12.start_sector
                || lba7.partition_size != lba12.partition_size)
        {
            return Err("first LBA7/LBA12 partition geometry disagrees".into());
        }
        types.push(lba12.partition_type);
        records.push(ExistingPartitionRecord { lba7, lba12 });
    }
    let mode = OfficialPartitionMode::from_partition_types(&types)
        .ok_or("EDPF partition types do not match an official mode")?;
    if plain7[count * 0x40..0xc0].iter().any(|byte| *byte != 0)
        || plain12[count * 0x60..0x120].iter().any(|byte| *byte != 0)
    {
        return Err("unused EDPF slots are nonzero".into());
    }
    let mut partitions = Vec::with_capacity(count);
    for (index, record) in records.iter().enumerate() {
        let role = match (mode, index) {
            (OfficialPartitionMode::WholeDiskEncrypted, 0) => PartitionRole::CompatibilityReserve,
            (OfficialPartitionMode::BootShareCombined, 0) => PartitionRole::BootShareCombined,
            (_, 0) => PartitionRole::Boot,
            (OfficialPartitionMode::IntranetExtranetDualPartition, 1) => PartitionRole::Share,
            (OfficialPartitionMode::DefaultThreePartition, 1) => PartitionRole::Share,
            _ => PartitionRole::Encrypt,
        };
        let partition_type = EdpPartitionType::from_raw(record.lba12.partition_type)
            .ok_or("unknown EDPF partition type")?;
        let physically_encrypted = match role {
            PartitionRole::Boot
            | PartitionRole::BootShareCombined
            | PartitionRole::CompatibilityReserve => false,
            PartitionRole::Share | PartitionRole::Encrypt => true,
        };
        partitions.push(ExistingPartition {
            role,
            partition_type,
            start_lba: record.lba12.start_sector,
            sector_count: record.lba12.partition_size / SECTOR as u64,
            physically_encrypted,
            filesystem: None,
        });
    }
    let geometry: Vec<TargetPartitionGeometry> =
        partitions.iter().map(|part| part.as_target()).collect();
    validate_target_geometry(&geometry, total_sectors)?;
    Ok(Some(ParsedExistingProvision {
        profile: ExistingProvisionProfile {
            source_mode: mode,
            partitions,
        },
        records,
        device_id: device_id.to_string(),
        total_sectors,
    }))
}
