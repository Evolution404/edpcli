//! UI-neutral geometry facade for provisioning frontends.
//!
//! Frontends should not depend on protocol implementation modules to derive
//! compatibility extents. Protocol-specific geometry stays behind this service
//! boundary so CLI/TUI code consumes only provisioning geometry.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProvisionCompatibilityExtent {
    pub start_lba: u64,
    pub sector_count: u64,
}

/// Resolve the verified first-party USB compatibility extent for provisioning.
///
/// Sector size and CHS translation details are protocol implementation details;
/// callers provide only the observed whole-disk capacity in sectors.
pub fn verified_usb_compatibility_extent(
    total_sectors: u64,
) -> Option<ProvisionCompatibilityExtent> {
    let layout =
        crate::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity(
            total_sectors,
            crate::common::SECTOR as u32,
        )?;
    Some(ProvisionCompatibilityExtent {
        start_lba: layout.start_lba,
        sector_count: layout.size_sectors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facade_preserves_protocol_geometry_without_exposing_protocol_type() {
        let total_sectors = 976_773_168;
        let expected =
            crate::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity(
                total_sectors,
                crate::common::SECTOR as u32,
            )
            .expect("verified USB geometry");
        let actual =
            verified_usb_compatibility_extent(total_sectors).expect("application geometry facade");

        assert_eq!(actual.start_lba, expected.start_lba);
        assert_eq!(actual.sector_count, expected.size_sectors);
    }
}
