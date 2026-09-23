//! Strict byte plan shared by provisioning preview, image export and real writes.

use std::collections::BTreeMap;

use crate::common::SECTOR;

use super::{
    build_lce_ciphertext, build_official_exfat_partitions, generate_official_image,
    OfficialProvisionPlan, ProvisionEntropy, ProvisionImage, ProvisionSpec,
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
    file_key: &[u8; 16],
    volume_label: &str,
    volume_serials: &[u32],
) -> Result<OfficialProvisionWriteImage, String> {
    let metadata = generate_official_image(spec, entropy, plan)?;
    let total_sectors = spec.target().total_sectors();
    if total_sectors > u32::MAX as u64 {
        return Err(
            "current first-party MBR provisioning is limited to u32 sector addresses".into(),
        );
    }

    let filesystems =
        build_official_exfat_partitions(plan, file_key, volume_label, volume_serials)?;
    let mut patch = BTreeMap::new();

    for fs in filesystems {
        for (&relative_lba, sector) in fs.image.sectors() {
            let absolute = fs
                .geometry
                .start_sector
                .checked_add(relative_lba)
                .ok_or("filesystem absolute LBA overflow")?;
            insert_sector(&mut patch, absolute, sector, total_sectors, "filesystem")?;
        }
    }

    let lce = build_lce_ciphertext(plan.lba7_compatibility_extent)?;
    for (index, sector) in lce.chunks_exact(SECTOR).enumerate() {
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
