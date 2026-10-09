//! UI policy for READ-ONLY source-key verification, independent from physical
//! provisioning write authorization. Application enforces geometry again.

pub(crate) const fn supports_source_key_verification(bytes: Option<u32>) -> bool {
    matches!(bytes, Some(512 | 4096))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_source_geometry_does_not_imply_physical_write_permission() {
        assert!(supports_source_key_verification(Some(512)));
        assert!(supports_source_key_verification(Some(4096)));
        for invalid in [None, Some(0), Some(1024), Some(2048), Some(8192)] {
            assert!(!supports_source_key_verification(invalid));
        }
    }
}
