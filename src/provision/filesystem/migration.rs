use std::collections::BTreeSet;

use super::{build_empty_exfat, build_empty_fat16, SparseFilesystemImage, SECTOR_SIZE};
use crate::filesystem::{
    exfat_boot_checksum, exfat_geometry, exfat_upcase_table, fat_chain, put_stream, put_u16,
    put_u32, put_u64, upcase_mapping,
};
use crate::provision::{MigrationStagedEntry, OfficialFilesystemFormat};

#[derive(Clone, Debug)]
struct TreeEntry<'a> {
    staged: &'a MigrationStagedEntry,
    parent: String,
    name: String,
    first_cluster: u32,
    cluster_count: u32,
}

fn split_path(path: &str, directory: bool) -> Result<(String, String), String> {
    if !path.starts_with('/') || path == "/" {
        return Err(format!("invalid migration path {path:?}"));
    }
    let trimmed = path.trim_end_matches('/');
    let (parent, name) = trimmed
        .rsplit_once('/')
        .ok_or_else(|| format!("invalid migration path {path:?}"))?;
    if name.is_empty() || name == "." || name == ".." {
        return Err(format!("invalid migration path component {path:?}"));
    }
    let parent = if parent.is_empty() {
        "/".to_string()
    } else {
        format!("{parent}/")
    };
    let canonical = if directory {
        format!("{trimmed}/")
    } else {
        trimmed.to_string()
    };
    if canonical != path {
        return Err(format!("migration path is not canonical: {path:?}"));
    }
    Ok((parent, name.to_string()))
}

fn validate_tree(entries: &[MigrationStagedEntry]) -> Result<(), String> {
    let dirs = entries
        .iter()
        .filter(|entry| entry.is_directory)
        .map(|entry| entry.path.as_str())
        .collect::<BTreeSet<_>>();
    let mut paths = BTreeSet::new();
    for entry in entries {
        let (parent, _) = split_path(&entry.path, entry.is_directory)?;
        if parent != "/" && !dirs.contains(parent.as_str()) {
            return Err(format!(
                "migration entry {:?} has missing parent directory {parent:?}",
                entry.path
            ));
        }
        let key = entry.path.trim_end_matches('/').to_lowercase();
        if !paths.insert(key) {
            return Err(format!("duplicate migration path {:?}", entry.path));
        }
    }
    Ok(())
}

fn children<'a>(entries: &'a [TreeEntry<'a>], parent: &str) -> Vec<&'a TreeEntry<'a>> {
    let mut out = entries
        .iter()
        .filter(|entry| entry.parent == parent)
        .collect::<Vec<_>>();
    out.sort_by(|left, right| left.staged.path.cmp(&right.staged.path));
    out
}

fn short_alias(index: usize, directory: bool) -> [u8; 11] {
    let mut out = [b' '; 11];
    let base = format!("K6{:06X}", index & 0x00ff_ffff);
    out[..8].copy_from_slice(base.as_bytes());
    if !directory {
        out[8..].copy_from_slice(b"DAT");
    }
    out
}

fn short_checksum(short: &[u8; 11]) -> u8 {
    short
        .iter()
        .fold(0u8, |sum, &byte| sum.rotate_right(1).wrapping_add(byte))
}

fn fat_lfn_entries(name: &str, short: &[u8; 11]) -> Result<Vec<[u8; 32]>, String> {
    let mut units = name.encode_utf16().collect::<Vec<_>>();
    if units.is_empty() || units.len() > 255 {
        return Err(format!("FAT long filename length is unsupported: {name:?}"));
    }
    units.push(0);
    while !units.len().is_multiple_of(13) {
        units.push(0xffff);
    }
    let count = units.len() / 13;
    if count > 20 {
        return Err(format!(
            "FAT long filename requires too many entries: {name:?}"
        ));
    }
    let checksum = short_checksum(short);
    let offsets = [1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
    let mut out = Vec::with_capacity(count);
    for ord in (1..=count).rev() {
        let mut entry = [0xffu8; 32];
        entry[0] = ord as u8 | if ord == count { 0x40 } else { 0 };
        entry[11] = 0x0f;
        entry[12] = 0;
        entry[13] = checksum;
        entry[26..28].copy_from_slice(&0u16.to_le_bytes());
        for (slot, unit) in offsets
            .iter()
            .copied()
            .zip(units[(ord - 1) * 13..ord * 13].iter().copied())
        {
            entry[slot..slot + 2].copy_from_slice(&unit.to_le_bytes());
        }
        out.push(entry);
    }
    Ok(out)
}

fn fat_short_entry(
    short: [u8; 11],
    attributes: u8,
    first_cluster: u32,
    logical_size: u64,
) -> Result<[u8; 32], String> {
    if first_cluster > u16::MAX as u32 {
        return Err("FAT16 cluster exceeds u16".into());
    }
    let size = u32::try_from(logical_size).map_err(|_| "FAT16 file exceeds u32 bytes")?;
    let mut entry = [0u8; 32];
    entry[..11].copy_from_slice(&short);
    entry[11] = attributes;
    entry[26..28].copy_from_slice(&(first_cluster as u16).to_le_bytes());
    entry[28..32].copy_from_slice(&size.to_le_bytes());
    Ok(entry)
}

fn append_fat_named_entry(
    directory: &mut Vec<u8>,
    alias_index: usize,
    entry: &TreeEntry<'_>,
) -> Result<(), String> {
    let short = short_alias(alias_index, entry.staged.is_directory);
    for lfn in fat_lfn_entries(&entry.name, &short)? {
        directory.extend_from_slice(&lfn);
    }
    let mut attributes = (entry.staged.attributes as u8) & 0x27;
    if entry.staged.is_directory {
        attributes |= 0x10;
    } else {
        attributes &= !0x10;
    }
    directory.extend_from_slice(&fat_short_entry(
        short,
        attributes,
        entry.first_cluster,
        entry.staged.data.len() as u64,
    )?);
    Ok(())
}

fn set_fat16_value(fat: &mut [u8], cluster: u32, value: u16) -> Result<(), String> {
    let at = usize::try_from(cluster)
        .ok()
        .and_then(|cluster| cluster.checked_mul(2))
        .ok_or("FAT16 offset overflow")?;
    if at + 2 > fat.len() {
        return Err("FAT16 allocation exceeds FAT".into());
    }
    fat[at..at + 2].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn allocate_contiguous(next: &mut u32, count: u32, limit: u32) -> Result<u32, String> {
    if count == 0 {
        return Ok(0);
    }
    let first = *next;
    let end = first
        .checked_add(count)
        .ok_or("filesystem cluster allocation overflow")?;
    if first < 2 || end > limit.saturating_add(2) {
        return Err("migration files do not fit target filesystem cluster heap".into());
    }
    *next = end;
    Ok(first)
}

fn write_cluster_bytes(
    image: &mut SparseFilesystemImage,
    data_start: u64,
    sectors_per_cluster: u64,
    first_cluster: u32,
    cluster_count: u32,
    bytes: &[u8],
) -> Result<(), String> {
    if cluster_count == 0 {
        return if bytes.is_empty() {
            Ok(())
        } else {
            Err("non-empty payload has no allocated cluster".into())
        };
    }
    let capacity = cluster_count as u64 * sectors_per_cluster * SECTOR_SIZE as u64;
    if bytes.len() as u64 > capacity {
        return Err("payload exceeds allocated target clusters".into());
    }
    let first_lba = data_start
        .checked_add((first_cluster as u64 - 2) * sectors_per_cluster)
        .ok_or("target cluster LBA overflow")?;
    for sector_index in 0..cluster_count as u64 * sectors_per_cluster {
        let mut sector = [0u8; SECTOR_SIZE];
        let start = sector_index as usize * SECTOR_SIZE;
        if start < bytes.len() {
            let end = (start + SECTOR_SIZE).min(bytes.len());
            sector[..end - start].copy_from_slice(&bytes[start..end]);
        }
        image.sectors.insert(first_lba + sector_index, sector);
    }
    Ok(())
}

fn build_migrated_fat16(
    partition_offset: u64,
    volume_sectors: u64,
    volume_serial: u32,
    volume_label: &str,
    staged: &[MigrationStagedEntry],
) -> Result<SparseFilesystemImage, String> {
    validate_tree(staged)?;
    let mut image = build_empty_fat16(
        partition_offset,
        volume_sectors,
        volume_serial,
        volume_label,
    )?;
    let boot = image
        .sector_or_zero(0)
        .ok_or("FAT16 target has no boot sector")?;
    let sectors_per_cluster = boot[13] as u64;
    let reserved = u16::from_le_bytes(boot[14..16].try_into().unwrap()) as u64;
    let copies = boot[16] as u64;
    let root_entries = u16::from_le_bytes(boot[17..19].try_into().unwrap()) as u64;
    let fat_sectors = u16::from_le_bytes(boot[22..24].try_into().unwrap()) as u64;
    let root_sectors = (root_entries * 32).div_ceil(SECTOR_SIZE as u64);
    let root_start = reserved + copies * fat_sectors;
    let data_start = root_start + root_sectors;
    if data_start >= volume_sectors || sectors_per_cluster == 0 {
        return Err("invalid FAT16 target geometry".into());
    }
    let cluster_limit = u32::try_from((volume_sectors - data_start) / sectors_per_cluster)
        .map_err(|_| "FAT16 cluster count overflow")?;

    let cluster_bytes = sectors_per_cluster * SECTOR_SIZE as u64;
    let mut next_cluster = 2u32;
    let mut tree = Vec::with_capacity(staged.len());
    for entry in staged.iter().filter(|entry| entry.is_directory) {
        let (parent, name) = split_path(&entry.path, true)?;
        let child_count = staged
            .iter()
            .filter(|candidate| {
                split_path(&candidate.path, candidate.is_directory)
                    .map(|(candidate_parent, _)| candidate_parent == entry.path)
                    .unwrap_or(false)
            })
            .count();
        let bytes_needed = 64u64
            .checked_add((child_count as u64).saturating_mul(21 * 32))
            .ok_or("FAT16 directory size overflow")?;
        let clusters = u32::try_from(bytes_needed.div_ceil(cluster_bytes).max(1))
            .map_err(|_| "FAT16 directory cluster count overflow")?;
        let first_cluster = allocate_contiguous(&mut next_cluster, clusters, cluster_limit)?;
        tree.push(TreeEntry {
            staged: entry,
            parent,
            name,
            first_cluster,
            cluster_count: clusters,
        });
    }
    for entry in staged.iter().filter(|entry| !entry.is_directory) {
        let (parent, name) = split_path(&entry.path, false)?;
        let clusters = if entry.data.is_empty() {
            0
        } else {
            u32::try_from((entry.data.len() as u64).div_ceil(cluster_bytes))
                .map_err(|_| "FAT16 file cluster count overflow")?
        };
        let first_cluster = allocate_contiguous(&mut next_cluster, clusters, cluster_limit)?;
        tree.push(TreeEntry {
            staged: entry,
            parent,
            name,
            first_cluster,
            cluster_count: clusters,
        });
    }

    let fat_bytes_len =
        usize::try_from(fat_sectors * SECTOR_SIZE as u64).map_err(|_| "FAT16 FAT is too large")?;
    let mut fat = vec![0u8; fat_bytes_len];
    fat[..4].copy_from_slice(&[0xf8, 0xff, 0xff, 0xff]);
    for entry in &tree {
        for offset in 0..entry.cluster_count {
            let cluster = entry.first_cluster + offset;
            let value = if offset + 1 == entry.cluster_count {
                0xffff
            } else {
                u16::try_from(cluster + 1).map_err(|_| "FAT16 next cluster overflow")?
            };
            set_fat16_value(&mut fat, cluster, value)?;
        }
    }
    for copy in 0..copies {
        for sector_index in 0..fat_sectors {
            let mut sector = [0u8; SECTOR_SIZE];
            let start = sector_index as usize * SECTOR_SIZE;
            sector.copy_from_slice(&fat[start..start + SECTOR_SIZE]);
            image
                .sectors
                .insert(reserved + copy * fat_sectors + sector_index, sector);
        }
    }

    let mut alias_index = 1usize;
    let mut root = Vec::new();
    let existing_label = image
        .sector_or_zero(root_start)
        .ok_or("FAT16 root directory missing")?;
    root.extend_from_slice(&existing_label[..32]);
    for entry in children(&tree, "/") {
        append_fat_named_entry(&mut root, alias_index, entry)?;
        alias_index += 1;
    }
    let root_capacity = root_sectors * SECTOR_SIZE as u64;
    if root.len() as u64 > root_capacity {
        return Err("migration root directory exceeds FAT16 fixed root capacity".into());
    }
    root.resize(root_capacity as usize, 0);
    for sector_index in 0..root_sectors {
        let start = sector_index as usize * SECTOR_SIZE;
        let mut sector = [0u8; SECTOR_SIZE];
        sector.copy_from_slice(&root[start..start + SECTOR_SIZE]);
        image.sectors.insert(root_start + sector_index, sector);
    }

    for directory in tree.iter().filter(|entry| entry.staged.is_directory) {
        let mut bytes = Vec::new();
        let dot = fat_short_entry(*b".          ", 0x10, directory.first_cluster, 0)?;
        bytes.extend_from_slice(&dot);
        let parent_cluster = if directory.parent == "/" {
            0
        } else {
            tree.iter()
                .find(|entry| entry.staged.is_directory && entry.staged.path == directory.parent)
                .ok_or_else(|| format!("missing FAT16 parent {:?}", directory.parent))?
                .first_cluster
        };
        let dotdot = fat_short_entry(*b"..         ", 0x10, parent_cluster, 0)?;
        bytes.extend_from_slice(&dotdot);
        for child in children(&tree, &directory.staged.path) {
            append_fat_named_entry(&mut bytes, alias_index, child)?;
            alias_index += 1;
        }
        let capacity = directory.cluster_count as u64 * sectors_per_cluster * SECTOR_SIZE as u64;
        if bytes.len() as u64 > capacity {
            return Err(format!(
                "migration directory {:?} exceeds allocated FAT16 clusters",
                directory.staged.path
            ));
        }
        write_cluster_bytes(
            &mut image,
            data_start,
            sectors_per_cluster,
            directory.first_cluster,
            directory.cluster_count,
            &bytes,
        )?;
    }

    for file in tree.iter().filter(|entry| !entry.staged.is_directory) {
        write_cluster_bytes(
            &mut image,
            data_start,
            sectors_per_cluster,
            file.first_cluster,
            file.cluster_count,
            &file.staged.data,
        )?;
    }

    Ok(image)
}

fn exfat_name_hash(name: &[u16]) -> u16 {
    name.iter().fold(0u16, |hash, unit| {
        let upper = upcase_mapping(*unit);
        upper.to_le_bytes().into_iter().fold(hash, |sum, byte| {
            sum.rotate_right(1).wrapping_add(byte as u16)
        })
    })
}

fn exfat_entry_set(
    entry: &TreeEntry<'_>,
    data_length: u64,
    first_cluster: u32,
) -> Result<Vec<u8>, String> {
    let units = entry.name.encode_utf16().collect::<Vec<_>>();
    if units.is_empty() || units.len() > 255 {
        return Err(format!(
            "exFAT filename length is unsupported: {:?}",
            entry.name
        ));
    }
    let filename_entries = units.len().div_ceil(15);
    let secondary_count = 1 + filename_entries;
    let mut bytes = vec![0u8; (secondary_count + 1) * 32];
    bytes[0] = 0x85;
    bytes[1] = secondary_count as u8;
    let mut attributes = (entry.staged.attributes as u16) & 0x0037;
    if entry.staged.is_directory {
        attributes |= 0x10;
    } else {
        attributes &= !0x10;
    }
    put_u16(&mut bytes, 4, attributes);

    let stream = &mut bytes[32..64];
    stream[0] = 0xc0;
    stream[1] = 0x03;
    stream[3] = units.len() as u8;
    put_u16(stream, 4, exfat_name_hash(&units));
    put_u64(stream, 8, data_length);
    put_u32(stream, 20, first_cluster);
    put_u64(stream, 24, data_length);

    for (index, chunk) in units.chunks(15).enumerate() {
        let filename = &mut bytes[(2 + index) * 32..(3 + index) * 32];
        filename[0] = 0xc1;
        for (slot, unit) in chunk.iter().copied().enumerate() {
            put_u16(filename, 2 + slot * 2, unit);
        }
    }
    let checksum = {
        let mut checksum = 0u16;
        for (index, byte) in bytes.iter().copied().enumerate() {
            if matches!(index, 2 | 3) {
                continue;
            }
            checksum = checksum.rotate_right(1).wrapping_add(byte as u16);
        }
        checksum
    };
    put_u16(&mut bytes, 2, checksum);
    Ok(bytes)
}

fn build_migrated_exfat(
    partition_offset: u64,
    volume_sectors: u64,
    volume_serial: u32,
    volume_label: &str,
    staged: &[MigrationStagedEntry],
) -> Result<SparseFilesystemImage, String> {
    validate_tree(staged)?;
    let mut image = build_empty_exfat(
        partition_offset,
        volume_sectors,
        volume_serial,
        volume_label,
    )?;
    let mut boot = image
        .sector_or_zero(0)
        .ok_or("exFAT target has no boot sector")?;
    let cluster_shift = boot[109];
    let sectors_per_cluster = 1u64
        .checked_shl(cluster_shift.into())
        .ok_or("invalid exFAT cluster shift")?;
    let (fat_length, heap_offset, cluster_count) = exfat_geometry(volume_sectors, cluster_shift)?;
    let cluster_bytes = sectors_per_cluster * SECTOR_SIZE as u64;
    let bitmap_len = (cluster_count as u64).div_ceil(8);
    let bitmap_clusters = u32::try_from(bitmap_len.div_ceil(cluster_bytes))
        .map_err(|_| "exFAT bitmap cluster count overflow")?;
    let upcase = exfat_upcase_table();
    let upcase_clusters = u32::try_from((upcase.len() as u64).div_ceil(cluster_bytes))
        .map_err(|_| "exFAT upcase cluster count overflow")?;
    let root_cluster = 2u32;
    let bitmap_cluster = 3u32;
    let upcase_cluster = bitmap_cluster
        .checked_add(bitmap_clusters)
        .ok_or("exFAT metadata cluster overflow")?;
    let system_end = upcase_cluster
        .checked_add(upcase_clusters)
        .ok_or("exFAT metadata cluster overflow")?;

    // Root needs its system entries plus every direct child entry set.
    let root_system_bytes = if volume_label.encode_utf16().next().is_some() {
        96usize
    } else {
        64usize
    };
    let mut root_needed = root_system_bytes as u64;
    for entry in staged {
        let (parent, name) = split_path(&entry.path, entry.is_directory)?;
        if parent == "/" {
            let units = name.encode_utf16().count();
            root_needed = root_needed
                .checked_add(((2 + units.div_ceil(15)) * 32) as u64)
                .ok_or("exFAT root size overflow")?;
        }
    }
    root_needed = root_needed
        .checked_add(32)
        .ok_or("exFAT root size overflow")?;
    let root_clusters = u32::try_from(root_needed.div_ceil(cluster_bytes).max(1))
        .map_err(|_| "exFAT root cluster count overflow")?;
    let root_extra = root_clusters.saturating_sub(1);

    let mut next_cluster = system_end;
    let root_extra_first = allocate_contiguous(&mut next_cluster, root_extra, cluster_count)?;
    let mut tree = Vec::with_capacity(staged.len());
    for entry in staged.iter().filter(|entry| entry.is_directory) {
        let (parent, name) = split_path(&entry.path, true)?;
        let child_bytes = staged
            .iter()
            .filter_map(|candidate| {
                let (candidate_parent, child_name) =
                    split_path(&candidate.path, candidate.is_directory).ok()?;
                (candidate_parent == entry.path)
                    .then_some(((2 + child_name.encode_utf16().count().div_ceil(15)) * 32) as u64)
            })
            .try_fold(32u64, |sum, value| sum.checked_add(value))
            .ok_or("exFAT directory size overflow")?;
        let clusters = u32::try_from(child_bytes.div_ceil(cluster_bytes).max(1))
            .map_err(|_| "exFAT directory cluster count overflow")?;
        let first_cluster = allocate_contiguous(&mut next_cluster, clusters, cluster_count)?;
        tree.push(TreeEntry {
            staged: entry,
            parent,
            name,
            first_cluster,
            cluster_count: clusters,
        });
    }
    for entry in staged.iter().filter(|entry| !entry.is_directory) {
        let (parent, name) = split_path(&entry.path, false)?;
        let clusters = if entry.data.is_empty() {
            0
        } else {
            u32::try_from((entry.data.len() as u64).div_ceil(cluster_bytes))
                .map_err(|_| "exFAT file cluster count overflow")?
        };
        let first_cluster = allocate_contiguous(&mut next_cluster, clusters, cluster_count)?;
        tree.push(TreeEntry {
            staged: entry,
            parent,
            name,
            first_cluster,
            cluster_count: clusters,
        });
    }
    let allocated_end = next_cluster;
    if allocated_end > cluster_count.saturating_add(2) {
        return Err("migration files exceed exFAT cluster heap".into());
    }

    // Rebuild FAT from the canonical empty image and extend the root chain.
    let fat_bytes_len = usize::try_from(
        fat_length
            .checked_mul(SECTOR_SIZE as u64)
            .ok_or("exFAT FAT byte length overflow")?,
    )
    .map_err(|_| "exFAT FAT is too large")?;
    let mut fat = vec![0u8; fat_bytes_len];
    put_u32(&mut fat, 0, 0xffff_fff8);
    put_u32(&mut fat, 4, 0xffff_ffff);
    fat_chain(&mut fat, bitmap_cluster, bitmap_clusters).map_err(|error| error.to_string())?;
    fat_chain(&mut fat, upcase_cluster, upcase_clusters).map_err(|error| error.to_string())?;
    if root_extra == 0 {
        put_u32(&mut fat, root_cluster as usize * 4, 0xffff_ffff);
    } else {
        put_u32(&mut fat, root_cluster as usize * 4, root_extra_first);
        fat_chain(&mut fat, root_extra_first, root_extra).map_err(|error| error.to_string())?;
    }
    const FAT_OFFSET: u64 = 24;
    for (index, chunk) in fat.as_chunks::<SECTOR_SIZE>().0.iter().enumerate() {
        let mut sector = [0u8; SECTOR_SIZE];
        sector.copy_from_slice(chunk);
        image.sectors.insert(FAT_OFFSET + index as u64, sector);
    }

    let mut bitmap = vec![0u8; bitmap_len as usize];
    for cluster in 2..allocated_end {
        let bit = (cluster - 2) as usize;
        if bit < bitmap.len() * 8 {
            bitmap[bit / 8] |= 1 << (bit % 8);
        }
    }
    put_stream(
        &mut image.sectors,
        heap_offset,
        sectors_per_cluster,
        bitmap_cluster,
        bitmap_clusters,
        &bitmap,
    )
    .map_err(|error| error.to_string())?;

    let mut root = vec![0u8; root_clusters as usize * cluster_bytes as usize];
    let old_root_lba = heap_offset + (root_cluster as u64 - 2) * sectors_per_cluster;
    for sector_index in 0..sectors_per_cluster {
        let sector = image
            .sector_or_zero(old_root_lba + sector_index)
            .ok_or("exFAT root sector missing")?;
        let start = sector_index as usize * SECTOR_SIZE;
        root[start..start + SECTOR_SIZE].copy_from_slice(&sector);
    }
    let mut root_offset = root_system_bytes;
    for entry in children(&tree, "/") {
        let data_length = if entry.staged.is_directory {
            entry.cluster_count as u64 * cluster_bytes
        } else {
            entry.staged.data.len() as u64
        };
        let set = exfat_entry_set(entry, data_length, entry.first_cluster)?;
        if root_offset + set.len() + 32 > root.len() {
            return Err("exFAT root directory allocation is insufficient".into());
        }
        root[root_offset..root_offset + set.len()].copy_from_slice(&set);
        root_offset += set.len();
    }
    // Leave an explicit end marker.
    root[root_offset] = 0;
    write_cluster_bytes(
        &mut image,
        heap_offset,
        sectors_per_cluster,
        root_cluster,
        1,
        &root[..cluster_bytes as usize],
    )?;
    if root_extra > 0 {
        write_cluster_bytes(
            &mut image,
            heap_offset,
            sectors_per_cluster,
            root_extra_first,
            root_extra,
            &root[cluster_bytes as usize..],
        )?;
    }

    for directory in tree.iter().filter(|entry| entry.staged.is_directory) {
        let mut bytes = Vec::new();
        for child in children(&tree, &directory.staged.path) {
            let data_length = if child.staged.is_directory {
                child.cluster_count as u64 * cluster_bytes
            } else {
                child.staged.data.len() as u64
            };
            bytes.extend_from_slice(&exfat_entry_set(child, data_length, child.first_cluster)?);
        }
        bytes.extend_from_slice(&[0u8; 32]);
        write_cluster_bytes(
            &mut image,
            heap_offset,
            sectors_per_cluster,
            directory.first_cluster,
            directory.cluster_count,
            &bytes,
        )?;
    }
    for file in tree.iter().filter(|entry| !entry.staged.is_directory) {
        write_cluster_bytes(
            &mut image,
            heap_offset,
            sectors_per_cluster,
            file.first_cluster,
            file.cluster_count,
            &file.staged.data,
        )?;
    }

    // Update PercentInUse in both boot regions and recompute their checksums.
    let allocated_clusters = allocated_end.saturating_sub(2) as u64;
    boot[112] = u8::try_from((allocated_clusters * 100).div_ceil(cluster_count as u64))
        .unwrap_or(100)
        .min(100);
    let mut main_boot = [[0u8; SECTOR_SIZE]; 12];
    for (index, sector) in main_boot.iter_mut().enumerate().take(11) {
        *sector = image
            .sector_or_zero(index as u64)
            .ok_or("exFAT boot sector missing")?;
    }
    main_boot[0] = boot;
    let checksum = exfat_boot_checksum(&main_boot);
    for chunk in main_boot[11].as_chunks_mut::<4>().0 {
        chunk.copy_from_slice(&checksum.to_le_bytes());
    }
    for (index, sector) in main_boot.iter().enumerate() {
        image.sectors.insert(index as u64, *sector);
        image.sectors.insert(index as u64 + 12, *sector);
    }
    Ok(image)
}

pub fn build_migrated_filesystem(
    filesystem: OfficialFilesystemFormat,
    partition_offset: u64,
    volume_sectors: u64,
    volume_serial: u32,
    volume_label: &str,
    staged: &[MigrationStagedEntry],
) -> Result<SparseFilesystemImage, String> {
    match filesystem {
        OfficialFilesystemFormat::Fat16 => build_migrated_fat16(
            partition_offset,
            volume_sectors,
            volume_serial,
            volume_label,
            staged,
        ),
        OfficialFilesystemFormat::ExFat => build_migrated_exfat(
            partition_offset,
            volume_sectors,
            volume_serial,
            volume_label,
            staged,
        ),
        OfficialFilesystemFormat::Fat32 | OfficialFilesystemFormat::Ntfs => Err(format!(
            "K6 first-party migration writer does not implement {}",
            filesystem.config_token()
        )),
    }
}
