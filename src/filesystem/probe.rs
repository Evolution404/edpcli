use crate::common::SECTOR;
use crate::domain::geometry::{FilesystemKind, FilesystemProbe, PartitionGeometry};

fn u16le(raw: &[u8], offset: usize) -> Option<u16> {
    raw.get(offset..offset + 2)
        .and_then(|v| v.try_into().ok())
        .map(u16::from_le_bytes)
}

fn u32le(raw: &[u8], offset: usize) -> Option<u32> {
    raw.get(offset..offset + 4)
        .and_then(|v| v.try_into().ok())
        .map(u32::from_le_bytes)
}

fn u64le(raw: &[u8], offset: usize) -> Option<u64> {
    raw.get(offset..offset + 8)
        .and_then(|v| v.try_into().ok())
        .map(u64::from_le_bytes)
}

pub(crate) fn probe_filesystem(partition: &PartitionGeometry, prefix: &[u8]) -> FilesystemProbe {
    let mut probe = FilesystemProbe {
        partition_index: partition.index,
        partition_type: partition.partition_type,
        kind: FilesystemKind::Unknown,
        bytes_per_sector: None,
        sectors_per_cluster: None,
        key_lbas: Vec::new(),
        notes: Vec::new(),
    };
    if prefix.len() < SECTOR {
        probe
            .notes
            .push("partition prefix is shorter than one sector".into());
        return probe;
    }
    let boot = &prefix[..SECTOR];

    if boot.get(3..11) == Some(b"NTFS    ") {
        let Some(bps) = u16le(boot, 11).map(u32::from) else {
            return probe;
        };
        let spc = boot[13] as u32;
        if bps == SECTOR as u32 && spc != 0 {
            probe.kind = FilesystemKind::Ntfs;
            probe.bytes_per_sector = Some(bps);
            probe.sectors_per_cluster = Some(spc);
            if let Some(mft_lcn) = u64le(boot, 48) {
                if let Some(lba) = mft_lcn
                    .checked_mul(spc as u64)
                    .and_then(|rel| partition.start_sector.checked_add(rel))
                {
                    probe.key_lbas.push(lba);
                    probe.notes.push(format!("NTFS $MFT LCN={mft_lcn}"));
                }
            }
            if let Some(mftmirr_lcn) = u64le(boot, 56) {
                if let Some(lba) = mftmirr_lcn
                    .checked_mul(spc as u64)
                    .and_then(|rel| partition.start_sector.checked_add(rel))
                {
                    probe.key_lbas.push(lba);
                    probe.notes.push(format!("NTFS $MFTMirr LCN={mftmirr_lcn}"));
                }
            }
            probe.key_lbas.push(
                partition
                    .start_sector
                    .saturating_add(partition.sector_count.saturating_sub(1)),
            );
        }
        return probe;
    }

    if let Some(layout) = crate::filesystem::exfat_analysis_layout(boot, partition.sector_count) {
        probe.kind = FilesystemKind::Exfat;
        probe.bytes_per_sector = Some(layout.bytes_per_sector);
        probe.sectors_per_cluster = Some(layout.sectors_per_cluster);
        if let Some(lba) = partition.start_sector.checked_add(layout.fat_offset) {
            probe.key_lbas.push(lba);
        }
        probe
            .notes
            .push(format!("exFAT FAT offset={}", layout.fat_offset));
        if let Some(lba) = partition.start_sector.checked_add(layout.root_relative_lba) {
            probe.key_lbas.push(lba);
        }
        probe.notes.push(format!(
            "exFAT root relative LBA={}",
            layout.root_relative_lba
        ));
        if let Some(lba) = partition.start_sector.checked_add(12) {
            probe.key_lbas.push(lba);
        }
        return probe;
    }

    if boot.get(82..90) == Some(b"FAT32   ") {
        let bps = u16le(boot, 11).unwrap_or(0) as u32;
        let spc = boot[13] as u32;
        let reserved = u16le(boot, 14).unwrap_or(0) as u64;
        let fats = boot[16] as u64;
        let fat_size = u32le(boot, 36).unwrap_or(0) as u64;
        let root_cluster = u32le(boot, 44).unwrap_or(0) as u64;
        if bps == SECTOR as u32 && spc != 0 && fat_size != 0 {
            probe.kind = FilesystemKind::Fat32;
            probe.bytes_per_sector = Some(bps);
            probe.sectors_per_cluster = Some(spc);
            if let Some(lba) = partition.start_sector.checked_add(reserved) {
                probe.key_lbas.push(lba);
            }
            let data_start = reserved.saturating_add(fats.saturating_mul(fat_size));
            if root_cluster >= 2 {
                if let Some(lba) = (root_cluster - 2)
                    .checked_mul(spc as u64)
                    .and_then(|v| data_start.checked_add(v))
                    .and_then(|rel| partition.start_sector.checked_add(rel))
                {
                    probe.key_lbas.push(lba);
                }
            }
            if let Some(fsinfo) = u16le(boot, 48) {
                if let Some(lba) = partition.start_sector.checked_add(fsinfo as u64) {
                    probe.key_lbas.push(lba);
                }
            }
            if let Some(backup_boot) = u16le(boot, 50) {
                if backup_boot != 0 {
                    if let Some(lba) = partition.start_sector.checked_add(backup_boot as u64) {
                        probe.key_lbas.push(lba);
                    }
                }
            }
        }
    }
    probe
}
