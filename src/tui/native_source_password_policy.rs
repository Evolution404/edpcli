//! UI policy for READ-ONLY source-key verification, independent from physical
//! provisioning write authorization. Application enforces geometry again.

pub(crate) const fn supports_source_key_verification(bytes: Option<u32>) -> bool {
    match bytes {
        Some(sector) => match crate::domain::hardware::native_sector_capability(sector) {
            Some(capability) => capability.fat_exfat_format,
            None => false,
        },
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_source_geometry_does_not_imply_physical_write_permission() {
        for accepted in [512, 1024, 2048, 4096] {
            assert!(supports_source_key_verification(Some(accepted)));
        }
        for invalid in [None, Some(0), Some(768), Some(8192)] {
            assert!(!supports_source_key_verification(invalid));
        }
    }
}
