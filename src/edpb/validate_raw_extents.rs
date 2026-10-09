//! Reject ambiguous native raw-evidence mapping in read-only EDPB v4.
//! EDPB v3 retains its existing restorable extent rules.
use super::*;

pub(super) fn validate_unique_native_raw_extents(manifest: &Manifest) -> Result<(), String> {
    let mut ranges = Vec::new();
    for artifact in manifest
        .artifacts
        .iter()
        .filter(|a| a.kind == "raw_sectors")
    {
        let [extent_id] = artifact.source_extent_ids.as_slice() else {
            return Err("4Kn raw evidence must reference exactly one source extent".into());
        };
        let extent = manifest
            .extents
            .iter()
            .find(|e| &e.id == extent_id)
            .ok_or("4Kn raw source extent missing")?;
        if extent.sector_count == 0 {
            return Err("4Kn raw evidence extent cannot be empty".into());
        }
        let end = extent
            .start_lba
            .checked_add(extent.sector_count)
            .ok_or("4Kn source extent range overflow")?;
        if manifest
            .geometry
            .total_sectors
            .is_some_and(|total| end > total)
        {
            return Err("4Kn raw evidence extent exceeds source device geometry".into());
        }
        ranges.push((extent.start_lba, end));
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err("4Kn raw evidence source extents overlap".into());
    }
    Ok(())
}
