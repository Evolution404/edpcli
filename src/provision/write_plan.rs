//! Strict byte plan shared by provisioning preview, image export and real writes.

use std::collections::BTreeMap;

use crate::common::SECTOR;

use super::{
    build_lce_ciphertext, generate_official_image, OfficialProvisionPlan, ProvisionEntropy,
    ProvisionImage, ProvisionSpec,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfficialProvisionWriteImage {
    pub metadata: ProvisionImage,
    pub total_sectors: u64,
    pub patch: BTreeMap<u32, Vec<u8>>,
}

impl OfficialProvisionWriteImage {
    pub fn touched_sector_count(&self) -> usize {
        self.patch.len()
    }

    pub fn highest_touched_lba(&self) -> Option<u32> {
        self.patch.keys().next_back().copied()
    }
}

fn insert_sector(
    patch: &mut BTreeMap<u32, Vec<u8>>,
    lba: u64,
    data: &[u8],
    total_sectors: u64,
    owner: &str,
) -> Result<(), String> {
    if lba >= total_sectors {
        return Err(format!(
            "{owner} LBA{lba} lies beyond target {total_sectors}"
        ));
    }
    let lba32 = u32::try_from(lba).map_err(|_| format!("{owner} LBA{lba} exceeds u32"))?;
    if data.len() != SECTOR {
        return Err(format!(
            "{owner} LBA{lba} has {} bytes, expected {SECTOR}",
            data.len()
        ));
    }
    if patch.insert(lba32, data.to_vec()).is_some() {
        return Err(format!(
            "{owner} overlaps an existing planned write at LBA{lba}"
        ));
    }
    Ok(())
}

pub fn build_official_provision_write_image(
    spec: &ProvisionSpec,
    entropy: &ProvisionEntropy,
    plan: &OfficialProvisionPlan,
    _file_key: &[u8; 16],
    _volume_label: &str,
    _volume_serials: &[u32],
) -> Result<OfficialProvisionWriteImage, String> {
    build_official_provision_protocol_image(spec, entropy, plan)
}

/// The physical provision phase writes only protocol metadata and LCE. Filesystems
/// are a separately authorized, independently reported post-provision phase.
pub fn build_official_provision_protocol_image(
    spec: &ProvisionSpec,
    entropy: &ProvisionEntropy,
    plan: &OfficialProvisionPlan,
) -> Result<OfficialProvisionWriteImage, String> {
    let metadata = generate_official_image(spec, entropy, plan)?;
    let total_sectors = spec.target().total_sectors();
    if total_sectors > u32::MAX as u64 {
        return Err(
            "current first-party MBR provisioning is limited to u32 sector addresses".into(),
        );
    }

    if total_sectors == 0 {
        return Err("target has zero sectors".into());
    }

    let lce_start = plan.lba7_compatibility_extent.start_lba;
    let lce_end = lce_start
        .checked_add(plan.lba7_compatibility_extent.size_sectors)
        .ok_or("LCE range overflow")?;
    if lce_end > total_sectors {
        return Err(format!(
            "LCE range LBA{lce_start}..{} exceeds target LBA{}",
            lce_end - 1,
            total_sectors - 1
        ));
    }
    for partition in plan.logical_partitions(SECTOR as u64)? {
        let end = partition.end_sector_exclusive();
        if end > total_sectors {
            return Err(format!(
                "{} partition LBA{}..{} exceeds target LBA{}",
                partition.partition_type.role(),
                partition.start_sector,
                end.saturating_sub(1),
                total_sectors - 1
            ));
        }
        if partition.start_sector < lce_end && lce_start < end {
            return Err(format!(
                "{} partition LBA{}..{} overlaps LCE LBA{}..{}",
                partition.partition_type.role(),
                partition.start_sector,
                end - 1,
                lce_start,
                lce_end - 1
            ));
        }
    }

    let mut patch = BTreeMap::new();

    let lce = build_lce_ciphertext(plan.lba7_compatibility_extent)?;
    for (index, sector) in lce.as_chunks::<SECTOR>().0.iter().enumerate() {
        let lba = plan
            .lba7_compatibility_extent
            .start_lba
            .checked_add(index as u64)
            .ok_or("LCE LBA overflow")?;
        insert_sector(&mut patch, lba, sector, total_sectors, "LCE")?;
    }

    for lba in 0..13u64 {
        let start = lba as usize * SECTOR;
        insert_sector(
            &mut patch,
            lba,
            &metadata.as_bytes()[start..start + SECTOR],
            total_sectors,
            "metadata",
        )?;
    }

    Ok(OfficialProvisionWriteImage {
        metadata,
        total_sectors,
        patch,
    })
}
