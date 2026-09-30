use super::*;

fn default_protocol_request() -> AdvancedInspectRequest {
    AdvancedInspectRequest {
        mode: AdvancedInspectMode::Decode,
        lbas: (0..METADATA_SECTOR_COUNT as u64).collect(),
        export_dir: None,
        device_id_override: None,
        fail_soft_decode: false,
    }
}

pub fn load_backup_inspect(path: &Path) -> Result<AdvancedInspectWorkspace, InspectError> {
    load_backup_advanced_inspect(path, &default_protocol_request())
}

pub fn load_disk_inspect(
    runner: &dyn CmdRunner,
    disk: u32,
) -> Result<AdvancedInspectWorkspace, InspectError> {
    load_disk_advanced_inspect(runner, disk, &default_protocol_request())
}

pub fn load_backup_advanced_inspect(
    path: &Path,
    request: &AdvancedInspectRequest,
) -> Result<AdvancedInspectWorkspace, InspectError> {
    run_evidence_source(EvidenceSource::open_backup(path)?, request)
}

pub fn load_disk_advanced_inspect(
    runner: &dyn CmdRunner,
    disk: u32,
    request: &AdvancedInspectRequest,
) -> Result<AdvancedInspectWorkspace, InspectError> {
    run_evidence_source(EvidenceSource::open_disk(runner, disk)?, request)
}
