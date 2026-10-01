use super::PreparedProvision;

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
