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

/// Target LCE geometry for the same source-aware native CLI/TUI writer.
/// A Plain source has no LCE record: never place a guessed tail at total-1.
/// This mirrors the protocol producer's translated CHS geometry (255x63).
pub fn native_compatibility_extent(
    total_sectors: u64,
    logical_sector_bytes: u32,
) -> Result<ProvisionCompatibilityExtent, String> {
    if !crate::domain::hardware::valid_native_sector_bytes(logical_sector_bytes) {
        return Err("unsupported native logical sector bytes".into());
    }
    use crate::protocol::lba7_compat::{
        locate_lba7_compatibility_extent_from_geometry, VERIFIED_USB_SECTORS_PER_TRACK,
        VERIFIED_USB_TRACKS_PER_CYLINDER,
    };
    let per_cylinder =
        u64::from(VERIFIED_USB_TRACKS_PER_CYLINDER) * u64::from(VERIFIED_USB_SECTORS_PER_TRACK);
    let cylinders = total_sectors / per_cylinder;
    let layout = locate_lba7_compatibility_extent_from_geometry(
        cylinders,
        VERIFIED_USB_TRACKS_PER_CYLINDER,
        VERIFIED_USB_SECTORS_PER_TRACK,
        logical_sector_bytes,
    )
    .ok_or("native compatibility LCE cannot fit verified CHS geometry")?;
    if layout.start_lba < crate::provision::OFFICIAL_PARTITION_START_SECTOR
        || layout
            .start_lba
            .checked_add(layout.size_sectors)
            .is_none_or(|end| end > total_sectors)
    {
        return Err("native compatibility LCE lies outside verified disk geometry".into());
    }
    Ok(ProvisionCompatibilityExtent {
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
