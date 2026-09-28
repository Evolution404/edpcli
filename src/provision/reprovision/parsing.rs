use super::*;

/// Decoded, partition-owned protocol evidence. The two entries deliberately
/// remain separate: LBA7's later entries point at the compatibility extent,
/// while LBA12 holds the actual data geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExistingPartitionRecord {
    pub lba7: EdpfEntry64,
    pub lba12: EdpfEntry96,
}

impl ExistingPartitionRecord {
    pub fn lba7_key_material(self) -> super::super::LegacyLba7KeyMaterial {
        super::super::LegacyLba7KeyMaterial {
            user_key_crc: self.lba7.user_key_crc,
            file_key_crc: self.lba7.file_key_crc,
            wrapped_file_key: self.lba7.encrypted_file_key,
        }
    }

    pub fn lba12_key_material(self) -> Result<super::super::ProvisionKeyMaterial, String> {
        let encrypt_mode = super::super::FileKeyWrapMode::from_raw(self.lba12.encrypt_mode)
            .ok_or("existing partition uses unsupported FileKey wrap mode")?;
        Ok(super::super::ProvisionKeyMaterial {
            user_key_crc: self.lba12.user_key_crc,
            file_key_crc: self.lba12.file_key_crc,
            wrapped_file_key: self.lba12.encrypted_file_key,
            encrypt_mode,
        })
    }

    pub fn verified_sm4_file_key(self, password: &[u8]) -> Result<[u8; 16], String> {
        if self.lba12.need_encrypt == 0
            || self.lba12.encrypt_mode != super::super::FileKeyWrapMode::Sm4.raw()
        {
            return Err("existing partition does not use the verified SM4 key profile".into());
        }
        self.verified_file_key(Some(password))
            .map_err(|error| error.to_string())
    }

    pub fn verified_file_key(
        self,
        password: Option<&[u8]>,
    ) -> Result<[u8; 16], super::super::ExistingFileKeyError> {
        use super::super::ExistingFileKeyError;
        if self.lba12.need_encrypt == 0 {
            return Err(ExistingFileKeyError::MalformedKeyRecord);
        }
        let mode = super::super::FileKeyWrapMode::from_raw(self.lba12.encrypt_mode)
            .ok_or(ExistingFileKeyError::UnsupportedEncryptMode)?;
        super::super::keys::unwrap_file_key(
            password,
            super::super::ProvisionKeyMaterial {
                user_key_crc: self.lba12.user_key_crc,
                file_key_crc: self.lba12.file_key_crc,
                wrapped_file_key: self.lba12.encrypted_file_key,
                encrypt_mode: mode,
            },
        )
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

/// Classify and decode a complete metadata image without guessing unknown
/// formats. `None` means no paired, valid EDPF magic; a partially recognizable
/// or inconsistent registration is an error and must not be treated as plain.
pub fn parse_existing_provision(
    image: &super::super::ProvisionImage,
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

/// Replace one encrypted partition's key records without regenerating its
/// geometry, PassInfo, or any other protocol bytes.
pub fn rekey_existing_partition_image(
    image: &super::super::ProvisionImage,
    device_id: &str,
    total_sectors: u64,
    partition_index: u32,
    legacy: super::super::LegacyLba7KeyMaterial,
    current: super::super::ProvisionKeyMaterial,
) -> Result<super::super::ProvisionImage, String> {
    if current.encrypt_mode != super::super::FileKeyWrapMode::Sm4 {
        return Err("reinitialize requires the verified SM4 data profile".into());
    }
    let parsed = parse_existing_provision(image, device_id, total_sectors)?
        .ok_or("target has no EDP partition records")?;
    let index = partition_index
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
        .ok_or("invalid EDP partition index")?;
    let record = parsed.records.get(index).ok_or("EDP partition not found")?;
    if record.lba12.need_encrypt == 0 || !parsed.profile.partitions[index].physically_encrypted {
        return Err("selected partition is not an encrypted data partition".into());
    }
    let crc = crc32_bare(device_id.as_bytes());
    let mut bytes = image.as_bytes().to_vec();
    let mut plain7 = xor_rolling(&bytes[7 * SECTOR..8 * SECTOR], (crc & 0xffff) ^ (crc >> 16));
    let mut plain12 = a6b0_full(&bytes[12 * SECTOR..13 * SECTOR], &crc.to_le_bytes(), 0);
    let base7 = index * 0x40;
    let base12 = index * 0x60;
    plain7[base7 + 0x30..base7 + 0x40].copy_from_slice(&legacy.packed16());
    plain12[base12 + 0x30..base12 + 0x48].copy_from_slice(&current.packed24());
    plain12[base12 + 0x58] = current.encrypt_mode.raw();
    bytes[7 * SECTOR..8 * SECTOR]
        .copy_from_slice(&xor_rolling(&plain7, (crc & 0xffff) ^ (crc >> 16)));
    bytes[12 * SECTOR..13 * SECTOR].copy_from_slice(&crate::crypto::a7f0_full(
        &plain12,
        &crc.to_le_bytes(),
        0,
    ));
    let updated = super::super::ProvisionImage::from_bytes(bytes)?;
    let verified = parse_existing_provision(&updated, device_id, total_sectors)?
        .ok_or("rekeyed EDP image has no partition records")?;
    if verified.profile != parsed.profile || verified.records.len() != parsed.records.len() {
        return Err("rekey changed partition geometry or role".into());
    }
    for (slot, (before, after)) in parsed.records.iter().zip(&verified.records).enumerate() {
        if slot != index && before != after {
            return Err("rekey changed another partition record".into());
        }
    }
    Ok(updated)
}
