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
                    ProvisionResultPartition::from_format(item, official.target_plan.as_ref())
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

impl ProvisionResultPartition {
    fn from_format(
        item: &crate::application::provision::PlannedPartitionFormat,
        target_plan: Option<&crate::provision::TargetProvisionPlan>,
    ) -> Self {
        let role = item.target.role;
        let planned = target_plan.and_then(|plan| {
            plan.partitions.iter().find(|part| {
                part.geometry.role == role
                    && part.geometry.start_lba == item.target.geometry.start_sector
                    && part.geometry.sector_count == item.target.geometry.sector_count()
            })
        });
        let filesystem = if item.selected {
            item.filesystem.or(item.target.filesystem)
        } else {
            planned
                .filter(|part| part.disposition.preserves_extent())
                .and_then(|part| part.geometry.filesystem)
        };
        Self {
            role: Some(role),
            filesystem,
            start_lba: item.target.geometry.start_sector,
            size_bytes: item.target.geometry.size_bytes,
            selected_for_format: item.selected,
            disposition: planned.map(|part| part.disposition),
        }
    }
}

#[cfg(test)]
#[path = "result_model_tests.rs"]
mod tests;
