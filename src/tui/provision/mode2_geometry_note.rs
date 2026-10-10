use super::*;

pub(super) fn note_for(
    mode: crate::provision::OfficialPartitionMode,
    unallocated_sectors: u64,
    preserves_existing_encrypt_geometry: bool,
) -> Option<String> {
    (mode == crate::provision::OfficialPartitionMode::WholeDiskEncrypted
        && unallocated_sectors > 0
        && preserves_existing_encrypt_geometry)
        .then(|| {
            format!(
                "保留现有保密区几何；空闲 {} 未自动并入。如需占满，请调整保密区起点/容量。",
                AppState::format_sector_size(unallocated_sectors)
            )
        })
}

pub(super) fn editor_note(state: &AppState) -> Option<String> {
    let target_mode = state.provision_target_mode()?;
    if target_mode != crate::provision::OfficialPartitionMode::WholeDiskEncrypted {
        return None;
    }
    let (resolved, source) = state.provision_resolved_prefill().ok()?;
    let source_encrypt = source
        .as_ref()?
        .partition(crate::provision::PartitionRole::Encrypt)?;
    let target_encrypt = resolved.encrypt?;
    let preserves_existing = resolved.encrypt_start_lba == Some(source_encrypt.start_lba)
        && target_encrypt.sectors() == source_encrypt.sector_count;
    let parts = resolved
        .draft_partitions(u64::from(resolved.logical_sector_bytes))
        .ok()?;
    let unallocated =
        crate::provision::validate_target_geometry(&parts, resolved.usable_end_lba).ok()?;
    note_for(target_mode, unallocated, preserves_existing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_requires_mode2_free_space_and_preserved_encrypt_geometry() {
        use crate::provision::OfficialPartitionMode as Mode;
        assert!(note_for(Mode::WholeDiskEncrypted, 4096, true).is_some());
        assert!(note_for(Mode::WholeDiskEncrypted, 0, true).is_none());
        assert!(note_for(Mode::WholeDiskEncrypted, 4096, false).is_none());
        assert!(note_for(Mode::DefaultThreePartition, 4096, true).is_none());
    }
}
