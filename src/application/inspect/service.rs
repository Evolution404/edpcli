use super::*;

pub fn load_backup_advanced_inspect(
    path: &Path,
    request: &AdvancedInspectRequest,
) -> Result<AdvancedInspectWorkspace, InspectError> {
    let evidence = EvidenceSource::open_backup(path)?;
    let is_default_protocol_sweep = request.lbas.len() == METADATA_SECTOR_COUNT
        && request
            .lbas
            .iter()
            .copied()
            .eq(0..METADATA_SECTOR_COUNT as u64);
    if !is_default_protocol_sweep {
        return run_evidence_source(evidence, request);
    }

    // Metadata-only Plain backups are intentionally sparse: they capture only
    // partition-table metadata, not a fabricated LBA0-12 protocol image. The
    // browser must therefore build its whole-disk topology from the verified
    // Manifest/context while decoding only sectors that were actually captured.
    let mut available = request.clone();
    available.lbas = evidence.available_requested_lbas(&request.lbas);
    run_evidence_source(evidence, &available)
}

pub fn load_disk_advanced_inspect(
    runner: &dyn CmdRunner,
    disk: u32,
    request: &AdvancedInspectRequest,
) -> Result<AdvancedInspectWorkspace, InspectError> {
    run_evidence_source(EvidenceSource::open_disk(runner, disk)?, request)
}
