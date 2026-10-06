use super::*;

impl PreparedProvision {
    pub const fn total_sectors(&self) -> u64 {
        match self {
            Self::Official(prepared) => prepared.write_image.total_sectors,
            Self::Plain(prepared) => prepared.plan.total_sectors,
        }
    }

    pub fn expected_onlyid(&self) -> Option<&str> {
        match self {
            Self::Official(prepared) => Some(prepared.expected_onlyid.as_str()),
            Self::Plain(_) => None,
        }
    }

    pub fn hardware_probe(&self) -> &crate::platform::HardwareProbe {
        match self {
            Self::Official(prepared) => &prepared.expected_probe,
            Self::Plain(prepared) => &prepared.expected_probe,
        }
    }

    pub fn lce_extent(&self) -> Option<(u64, u64)> {
        match self {
            Self::Official(prepared) => Some((
                prepared.lce_start_lba,
                prepared.plan.lba7_compatibility_extent.size_sectors,
            )),
            Self::Plain(_) => None,
        }
    }
}

impl super::PreparedNewProvision {
    pub fn disk(&self) -> &u32 {
        &self.disk
    }
    pub fn device_id(&self) -> &String {
        &self.device_id
    }
    pub fn source_kind(&self) -> &crate::provision::DiskProvisionKind {
        &self.source_kind
    }
    pub fn mode(&self) -> &OfficialPartitionMode {
        &self.mode
    }
    pub fn force_change_password(&self) -> &bool {
        &self.force_change_password
    }
    pub fn pass_info_policy(&self) -> &PassInfoPolicy {
        &self.pass_info_policy
    }
    pub fn lce_start_lba(&self) -> &u64 {
        &self.lce_start_lba
    }
    pub fn write_image(&self) -> &OfficialProvisionWriteImage {
        &self.write_image
    }
    pub fn format_targets(&self) -> &Vec<PlannedPartitionFormat> {
        &self.format_targets
    }
    pub fn target_plan(&self) -> &Option<TargetProvisionPlan> {
        &self.target_plan
    }
}

impl super::PreparedPlainProvision {
    pub fn disk(&self) -> &u32 {
        &self.disk
    }
    pub fn device_id(&self) -> &String {
        &self.device_id
    }
    pub fn plan(&self) -> &PlainProvisionPlan {
        &self.plan
    }
    pub fn write_plan(&self) -> &PlainProvisionWritePlan {
        &self.write_plan
    }
    pub fn source_kind(&self) -> &crate::provision::DiskProvisionKind {
        &self.source_kind
    }
    pub fn source_lce_start_lba(&self) -> &Option<u64> {
        &self.source_lce_start_lba
    }
}
