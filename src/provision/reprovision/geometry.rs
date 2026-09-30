use super::*;

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
