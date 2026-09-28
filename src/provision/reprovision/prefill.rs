use super::*;

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
