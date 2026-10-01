#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionResultPartition {
    pub role: Option<crate::provision::PartitionRole>,
    pub filesystem: Option<crate::filesystem::FilesystemKind>,
    pub start_lba: u64,
    pub size_bytes: u64,
    pub selected_for_format: bool,
    pub disposition: Option<crate::provision::RegionDisposition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionResultSnapshot {
    pub disk: u32,
    pub target: crate::provision::ProvisionTarget,
    pub total_bytes: u64,
    pub partitions: Vec<ProvisionResultPartition>,
}

impl ProvisionResultSnapshot {
    pub fn from_prepared(
        prepared: &crate::application::provision::PreparedProvision,
        total_bytes: u64,
    ) -> Self {
        let partitions = match prepared {
            crate::application::provision::PreparedProvision::Official(official) => official
                .format_targets
                .iter()
                .map(|item| {
                    let role = item.target.role;
                    let disposition = official.target_plan.as_ref().and_then(|plan| {
                        plan.partitions
                            .iter()
                            .find(|part| part.geometry.role == role)
                            .map(|part| part.disposition)
                    });
                    ProvisionResultPartition {
                        role: Some(role),
                        filesystem: item.filesystem.or(item.target.filesystem),
                        start_lba: item.target.geometry.start_sector,
                        size_bytes: item.target.geometry.size_bytes,
                        selected_for_format: item.selected,
                        disposition,
                    }
                })
                .collect(),
            crate::application::provision::PreparedProvision::Plain(plain) => plain
                .plan
                .partitions
                .iter()
                .map(|item| ProvisionResultPartition {
                    role: None,
                    filesystem: Some(item.filesystem),
                    start_lba: item.start_lba,
                    size_bytes: item
                        .sector_count
                        .saturating_mul(crate::common::SECTOR as u64),
                    selected_for_format: true,
                    disposition: None,
                })
                .collect(),
        };

        Self {
            disk: prepared.disk(),
            target: prepared.target(),
            total_bytes,
            partitions,
        }
    }
}
