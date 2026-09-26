//! Sector-based, device-independent defaults for a target provisioning mode.

use crate::protocol::edpf::EdpPartitionType;
use crate::{
    common::SECTOR,
    crypto::{a6b0_full, crc32_bare, xor_rolling},
    protocol::edpf::{EdpfEntry64, EdpfEntry96, PassInfo},
};

use super::{
    OfficialFilesystemFormat, OfficialPartitionMode, PartitionRole, PassInfoPolicy,
    DEFAULT_MODE0_BOOT_SECTORS, OFFICIAL_PARTITION_START_SECTOR,
    WHOLE_DISK_ENCRYPTED_COMPAT_BOOT_BYTES,
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TargetGeometryOverrides {
    pub boot: Option<CapacityInput>,
    pub share: Option<CapacityInput>,
    pub encrypt: Option<CapacityInput>,
    pub boot_start_lba: Option<u64>,
    pub share_start_lba: Option<u64>,
    pub encrypt_start_lba: Option<u64>,
}

/// Apply user geometry edits without disturbing source-backed anchors that the
/// user did not edit. Plain/unanchored targets remain compact; registered
/// targets are allowed to leave gaps and fail closed on overlap.
pub fn apply_target_geometry_overrides(
    mut prefill: ProvisionPrefill,
    source: Option<&ExistingProvisionProfile>,
    overrides: TargetGeometryOverrides,
) -> Result<ProvisionPrefill, String> {
    if let Some(value) = overrides.boot {
        if prefill.boot.is_some() {
            prefill.boot = Some(value);
        }
    }
    if let Some(value) = overrides.share {
        if prefill.share.is_some() {
            prefill.share = Some(value);
        }
    }
    if let Some(value) = overrides.encrypt {
        if prefill.encrypt.is_some() {
            prefill.encrypt = Some(value);
        }
    }
    if let Some(start) = overrides.boot_start_lba {
        if prefill.boot.is_some() {
            prefill.boot_start_lba = Some(start);
        }
    }
    if let Some(start) = overrides.share_start_lba {
        if prefill.share.is_some() {
            prefill.share_start_lba = Some(start);
        }
    }
    if let Some(start) = overrides.encrypt_start_lba {
        if prefill.encrypt.is_some() {
            prefill.encrypt_start_lba = Some(start);
        }
    }

    if overrides.share_start_lba.is_none()
        && source
            .and_then(|source| source.partition(PartitionRole::Share))
            .is_none()
        && prefill.mode != OfficialPartitionMode::BootShareCombined
        && prefill.share.is_some()
    {
        prefill.share_start_lba = prefill
            .boot_start_lba
            .zip(prefill.boot)
            .and_then(|(start, size)| start.checked_add(size.sectors()));
    }
    if overrides.encrypt_start_lba.is_none()
        && source
            .and_then(|source| source.partition(PartitionRole::Encrypt))
            .is_none()
        && prefill.encrypt.is_some()
    {
        prefill.encrypt_start_lba = if prefill.mode == OfficialPartitionMode::WholeDiskEncrypted {
            prefill
                .boot_start_lba
                .zip(prefill.boot)
                .and_then(|(start, size)| start.checked_add(size.sectors()))
        } else {
            prefill
                .share_start_lba
                .zip(prefill.share)
                .and_then(|(start, size)| start.checked_add(size.sectors()))
        };
    }

    prefill.target_partitions(512)?;
    Ok(prefill)
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
    let compatible_encrypt_old = encrypt_old.filter(|old| match mode {
        OfficialPartitionMode::DefaultThreePartition => old.start_lba > boot_end,
        OfficialPartitionMode::BootShareCombined => old.start_lba > first,
        OfficialPartitionMode::WholeDiskEncrypted => old.start_lba >= boot_end,
        OfficialPartitionMode::IntranetExtranetDualPartition => false,
    });
    let encrypt = if matches!(mode, OfficialPartitionMode::IntranetExtranetDualPartition) {
        None
    } else {
        source_capacity(compatible_encrypt_old)?.or(Some(CapacityInput::from_quick(
            1024,
            QuickCapacityUnit::MiB,
            CapacitySource::SystemDefault,
        )?))
    };
    let encrypt_start = if encrypt.is_none() {
        None
    } else if let Some(old) = compatible_encrypt_old {
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
        if mode == OfficialPartitionMode::DefaultThreePartition && compatible_encrypt_old.is_none()
        {
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
    let encrypt = if matches!(mode, OfficialPartitionMode::WholeDiskEncrypted)
        && compatible_encrypt_old.is_none()
    {
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

    pub fn verified_sm4_file_key(self, password: &[u8]) -> Result<[u8; 16], String> {
        if self.lba12.need_encrypt == 0
            || self.lba12.encrypt_mode != super::FileKeyWrapMode::Sm4.raw()
        {
            return Err("existing partition does not use the verified SM4 key profile".into());
        }
        if self.lba12.user_key_crc != crc32_bare(password) {
            return Err("password does not match existing partition key record".into());
        }
        let effective_password: &[u8] = if password == b"0000aaaa" {
            b"LtSWi[2f)j"
        } else {
            password
        };
        let digest = super::keys::md5_digest(effective_password);
        let key =
            crate::backup_deep::keys::sm4_decrypt_block(&self.lba12.encrypted_file_key, &digest);
        if crc32_bare(&key) != self.lba12.file_key_crc {
            return Err("existing FileKeyCRC does not verify".into());
        }
        Ok(key)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedExistingProvision {
    pub profile: ExistingProvisionProfile,
    pub records: Vec<ExistingPartitionRecord>,
    pub device_id: String,
    pub total_sectors: u64,
    pub pass_info_policy: Option<PassInfoPolicy>,
    pub force_change_password: Option<bool>,
}

fn reliable_pass_info_policy(plain7: &[u8], plain12: &[u8]) -> Option<PassInfoPolicy> {
    fn decode(stored: &[u8]) -> Option<PassInfoPolicy> {
        let stored: &[u8; 14] = stored.try_into().ok()?;
        let pass = PassInfo::decode_stored(stored);
        if !matches!(pass.version, 0x0064 | 0x0206) {
            return None;
        }
        let force_change_password = match (pass.force_change_share, pass.force_change_encrypt) {
            (0, 0) => false,
            (1, 1) => true,
            _ => return None,
        };
        let cancel_password_complexity_check = match pass.no_usb_check_password_safe {
            0 => false,
            1 => true,
            _ => return None,
        };
        Some(PassInfoPolicy {
            force_change_password,
            cancel_password_complexity_check,
            max_share_password_errors: pass.max_share_password_errors,
            max_encrypt_password_errors: pass.max_encrypt_password_errors,
        })
    }

    let from7 = decode(plain7.get(0xc0..0xce)?)?;
    let from12 = decode(plain12.get(0x120..0x12e)?)?;
    (from7 == from12).then_some(from7)
}

pub fn pass_info_policy_from_sectors(
    lba7: &[u8],
    lba12: &[u8],
    device_id: &str,
) -> Option<PassInfoPolicy> {
    if lba7.len() != SECTOR || lba12.len() != SECTOR || device_id.is_empty() {
        return None;
    }
    let crc = crc32_bare(device_id.as_bytes());
    let plain7 = xor_rolling(lba7, (crc & 0xffff) ^ (crc >> 16));
    let plain12 = a6b0_full(lba12, &crc.to_le_bytes(), 0);
    if plain7.get(..4) != Some(b"EDPF") || plain12.get(..4) != Some(b"EDPF") {
        return None;
    }
    reliable_pass_info_policy(&plain7, &plain12)
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

    pub fn confirm_filesystem(
        &mut self,
        role: PartitionRole,
        format: OfficialFilesystemFormat,
    ) -> Result<(), String> {
        let part = self
            .profile
            .partitions
            .iter_mut()
            .find(|part| part.role == role)
            .ok_or("source has no partition with requested role")?;
        part.filesystem = Some(format);
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetPartitionPlan {
    pub geometry: TargetPartitionGeometry,
    pub action: PartitionAction,
    pub reason: String,
    /// Only present when a verified source key record belongs to this exact
    /// target geometry. The writer re-encodes it for the target slot.
    pub preserved_record: Option<ExistingPartitionRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetProvisionPlan {
    pub mode: OfficialPartitionMode,
    pub partitions: Vec<TargetPartitionPlan>,
    pub unallocated_sectors: u64,
}

impl TargetProvisionPlan {
    pub fn build(
        source: Option<&ParsedExistingProvision>,
        mode: OfficialPartitionMode,
        targets: &[TargetPartitionGeometry],
        usable_end_lba: u64,
        key_domains: &super::KeyDomainSecrets,
    ) -> Result<Self, String> {
        if targets.len() != mode.partition_types().len() {
            return Err("target partition count does not match official mode".into());
        }
        let gap = validate_target_geometry(targets, usable_end_lba)?;
        let mut partitions = Vec::with_capacity(targets.len());
        for (index, target) in targets.iter().enumerate() {
            if target.partition_type != mode.partition_types()[index] {
                return Err(format!(
                    "target partition type at slot {index} does not match official mode"
                ));
            }
            let mut action = PartitionAction::Rebuild;
            let mut reason = "无兼容且已验证的来源分区；原数据不能原样保留".to_string();
            let mut preserved_record = None;
            if let Some(source) = source {
                if let Some((source_index, old)) = source
                    .profile
                    .partitions
                    .iter()
                    .enumerate()
                    .find(|(_, old)| old.role == target.role)
                {
                    if decide_partition_action(Some(old), target) == PartitionAction::PreserveExact
                    {
                        let record = source.records[source_index];
                        let key_ok = if record.lba12.need_encrypt == 0 {
                            true
                        } else {
                            key_domains
                                .source_password(target.role)
                                .is_some_and(|password| {
                                    record.verified_sm4_file_key(password).is_ok()
                                })
                        };
                        if key_ok {
                            action = PartitionAction::PreserveExact;
                            reason =
                                "语义、精确几何、文件系统和密钥记录均已验证；数据区禁止写入".into();
                            preserved_record = Some(record);
                        } else {
                            reason = "原密码或 FileKey 无法验证；原数据不能原样保留".into();
                        }
                    } else {
                        reason =
                            "语义、位置、大小、物理加密或文件系统与来源不一致；原数据不能原样保留"
                                .into();
                    }
                }
            }
            partitions.push(TargetPartitionPlan {
                geometry: *target,
                action,
                reason,
                preserved_record,
            });
        }
        Ok(Self {
            mode,
            partitions,
            unallocated_sectors: gap,
        })
    }

    pub fn preserved_extents(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        self.partitions
            .iter()
            .filter(|part| part.action == PartitionAction::PreserveExact)
            .map(|part| (part.geometry.start_lba, part.geometry.sector_count))
    }

    pub fn has_preserved_partitions(&self) -> bool {
        self.partitions
            .iter()
            .any(|part| part.action == PartitionAction::PreserveExact)
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
    let pass_info_policy = reliable_pass_info_policy(&plain7, &plain12);
    let force_change_password = pass_info_policy.map(|policy| policy.force_change_password);
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
        pass_info_policy,
        force_change_password,
    }))
}
