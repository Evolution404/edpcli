//! exFAT allocation metadata and directory inventory; ordinary file payloads
//! are never read.
use super::{FileEntry, PartitionReader};
use std::collections::{BTreeSet, VecDeque};

pub(super) struct Inventory {
    pub total: u64,
    pub free: u64,
    pub entries: Vec<FileEntry>,
}

fn u16le(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}
fn u32le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn u64le(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
fn read(r: &mut dyn PartitionReader, lba: u64, total: u64) -> Result<Vec<u8>, String> {
    if lba >= total {
        return Err("exFAT sector out of bounds".into());
    }
    let bytes = r
        .read_sector(lba)
        .map_err(|error| format!("read relative LBA {lba}: {error}"))?;
    if bytes.len() != 512 {
        return Err("truncated exFAT sector".into());
    }
    Ok(bytes)
}

fn rotate_checksum32(mut value: u32, byte: u8) -> u32 {
    value = value.rotate_right(1);
    value.wrapping_add(byte as u32)
}

fn boot_checksum(sectors: &[Vec<u8>]) -> u32 {
    let mut checksum = 0u32;
    for (sector_index, sector) in sectors.iter().take(11).enumerate() {
        for (offset, &byte) in sector.iter().enumerate() {
            if sector_index == 0 && matches!(offset, 106 | 107 | 112) {
                continue;
            }
            checksum = rotate_checksum32(checksum, byte);
        }
    }
    checksum
}

fn entry_set_checksum(entries: &[u8]) -> u16 {
    let mut checksum = 0u16;
    for (index, &byte) in entries.iter().enumerate() {
        if matches!(index, 2 | 3) {
            continue;
        }
        checksum = checksum.rotate_right(1).wrapping_add(byte as u16);
    }
    checksum
}

#[derive(Clone, Copy)]
struct Geometry {
    heap_offset: u64,
    cluster_count: u32,
    sectors_per_cluster: u64,
}
impl Geometry {
    fn cluster_bytes(self) -> u64 {
        self.sectors_per_cluster * 512
    }
    fn cluster_lba(self, cluster: u32) -> Result<u64, String> {
        if cluster < 2 || cluster >= self.cluster_count + 2 {
            return Err("exFAT cluster out of bounds".into());
        }
        self.heap_offset
            .checked_add((cluster as u64 - 2) * self.sectors_per_cluster)
            .ok_or_else(|| "exFAT cluster LBA overflow".into())
    }
}

struct Fat {
    values: Vec<u32>,
    count: u32,
}
impl Fat {
    fn chain(&self, first: u32, claimed: &mut BTreeSet<u32>) -> Result<Vec<u32>, String> {
        let mut out = Vec::new();
        let mut cluster = first;
        loop {
            if cluster < 2 || cluster >= self.count + 2 {
                return Err("exFAT FAT cluster out of bounds".into());
            }
            if !claimed.insert(cluster) {
                return Err("cyclic or cross-linked exFAT cluster chain".into());
            }
            out.push(cluster);
            let next = self.values[cluster as usize];
            if next >= 0xfffffff8 {
                return Ok(out);
            }
            if next < 2 || next == 0xfffffff7 || next >= 0xfffffff0 {
                return Err("invalid/free/bad exFAT FAT chain link".into());
            }
            cluster = next;
        }
    }
}

fn contiguous_clusters(
    first: u32,
    data_length: u64,
    geometry: Geometry,
    claimed: &mut BTreeSet<u32>,
) -> Result<Vec<u32>, String> {
    if data_length == 0 {
        if first != 0 {
            return Err("zero-length exFAT stream has a first cluster".into());
        }
        return Ok(Vec::new());
    }
    if first < 2 {
        return Err("non-empty exFAT stream has no first cluster".into());
    }
    let count = data_length.div_ceil(geometry.cluster_bytes());
    if count > geometry.cluster_count as u64 {
        return Err("exFAT contiguous stream exceeds cluster heap".into());
    }
    let end = (first as u64)
        .checked_add(count)
        .ok_or_else(|| "exFAT contiguous stream overflow".to_string())?;
    if end > geometry.cluster_count as u64 + 2 {
        return Err("exFAT contiguous stream leaves cluster heap".into());
    }
    let mut out = Vec::with_capacity(count as usize);
    for cluster in first..first + count as u32 {
        if !claimed.insert(cluster) {
            return Err("cross-linked exFAT contiguous stream".into());
        }
        out.push(cluster);
    }
    Ok(out)
}

fn stream_clusters(
    first: u32,
    data_length: u64,
    no_fat_chain: bool,
    geometry: Geometry,
    fat: &Fat,
    claimed: &mut BTreeSet<u32>,
) -> Result<Vec<u32>, String> {
    if data_length == 0 {
        return contiguous_clusters(first, data_length, geometry, claimed);
    }
    if no_fat_chain {
        return contiguous_clusters(first, data_length, geometry, claimed);
    }
    let chain = fat.chain(first, claimed)?;
    if (chain.len() as u64) * geometry.cluster_bytes() < data_length {
        return Err("exFAT FAT chain is shorter than stream DataLength".into());
    }
    Ok(chain)
}

fn stream_bytes(
    r: &mut dyn PartitionReader,
    total: u64,
    geometry: Geometry,
    clusters: &[u32],
    data_length: u64,
) -> Result<Vec<u8>, String> {
    if data_length > 32 * 1024 * 1024 {
        return Err("exFAT metadata stream exceeds Deep budget".into());
    }
    let mut out = Vec::new();
    for &cluster in clusters {
        let start = geometry.cluster_lba(cluster)?;
        for sector in 0..geometry.sectors_per_cluster {
            out.extend_from_slice(&read(r, start + sector, total)?);
            if out.len() as u64 >= data_length {
                out.truncate(data_length as usize);
                return Ok(out);
            }
        }
    }
    if out.len() as u64 != data_length {
        return Err("truncated exFAT metadata stream".into());
    }
    Ok(out)
}

fn timestamp(value: u32, ten_ms: u8) -> Option<String> {
    let date = (value >> 16) as u16;
    let time = value as u16;
    if date == 0 || ten_ms > 199 {
        return None;
    }
    let year = 1980 + (date >> 9) as i32;
    let month = ((date >> 5) & 15) as u8;
    let day = (date & 31) as u8;
    let hour = time >> 11;
    let minute = (time >> 5) & 63;
    let second = (time & 31) * 2 + (ten_ms / 100) as u16;
    time::Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()?;
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:02}",
        ten_ms % 100
    ))
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|ch| ch < ' ' || "/\\:*?\"<>|".contains(ch))
}

#[derive(Clone)]
struct StreamRecord {
    entry: FileEntry,
    first_cluster: u32,
    data_length: u64,
    no_fat_chain: bool,
}

fn parse_file_set(bytes: &[u8]) -> Result<StreamRecord, String> {
    if bytes.len() < 96 || bytes.len() % 32 != 0 || bytes[0] != 0x85 {
        return Err("invalid exFAT file entry set".into());
    }
    let secondary_count = bytes[1] as usize;
    if secondary_count < 2 || bytes.len() != (secondary_count + 1) * 32 {
        return Err("invalid exFAT secondary entry count".into());
    }
    if entry_set_checksum(bytes) != u16le(bytes, 2) {
        return Err("exFAT entry-set checksum mismatch".into());
    }
    let attributes = u16le(bytes, 4);
    if attributes & !0x0037 != 0 {
        return Err("invalid exFAT file attributes".into());
    }
    let stream = &bytes[32..64];
    if stream[0] != 0xc0 {
        return Err("exFAT file set is missing stream extension".into());
    }
    let flags = stream[1];
    if flags & !0x03 != 0 || flags & 0x01 == 0 {
        return Err("invalid exFAT stream flags".into());
    }
    let name_len = stream[3] as usize;
    if name_len == 0 || name_len > 255 {
        return Err("invalid exFAT filename length".into());
    }
    let filename_entries = name_len.div_ceil(15);
    if secondary_count != 1 + filename_entries {
        return Err("exFAT filename entry count mismatch".into());
    }
    let mut units = Vec::with_capacity(filename_entries * 15);
    for entry in bytes[64..].chunks_exact(32) {
        if entry[0] != 0xc1 || entry[1] != 0 {
            return Err("invalid exFAT filename entry".into());
        }
        for offset in (2..32).step_by(2) {
            units.push(u16le(entry, offset));
        }
    }
    units.truncate(name_len);
    let name = String::from_utf16(&units).map_err(|_| "invalid exFAT UTF-16 filename")?;
    if !valid_name(&name) {
        return Err("invalid exFAT filename".into());
    }
    let valid_data_length = u64le(stream, 8);
    let first_cluster = u32le(stream, 20);
    let data_length = u64le(stream, 24);
    if valid_data_length > data_length {
        return Err("exFAT ValidDataLength exceeds DataLength".into());
    }
    let is_directory = attributes & 0x10 != 0;
    if is_directory && data_length == 0 {
        return Err("zero-length exFAT directory stream".into());
    }
    Ok(StreamRecord {
        entry: FileEntry {
            path: name,
            is_directory,
            logical_size: data_length,
            allocated_size: None,
            mtime: timestamp(u32le(bytes, 12), bytes[21]),
            ctime: timestamp(u32le(bytes, 8), bytes[20]),
            attributes: attributes as u32,
        },
        first_cluster,
        data_length,
        no_fat_chain: flags & 0x02 != 0,
    })
}

#[derive(Clone, Copy)]
struct SystemStream {
    first_cluster: u32,
    data_length: u64,
}

pub(super) fn parse(
    r: &mut dyn PartitionReader,
    partition_sectors: u64,
    boot: &[u8],
) -> Result<Inventory, String> {
    if boot.len() != 512
        || &boot[3..11] != b"EXFAT   "
        || boot[11..64].iter().any(|&byte| byte != 0)
        || boot[510..512] != [0x55, 0xaa]
    {
        return Err("invalid exFAT main boot sector".into());
    }
    let sector_shift = boot[108];
    let cluster_shift = boot[109];
    if sector_shift != 9 || cluster_shift > 7 {
        return Err("unsupported exFAT sector or cluster size".into());
    }
    let sectors_per_cluster = 1u64 << cluster_shift;
    let volume_length = u64le(boot, 72);
    let fat_offset = u32le(boot, 80) as u64;
    let fat_length = u32le(boot, 84) as u64;
    let heap_offset = u32le(boot, 88) as u64;
    let cluster_count = u32le(boot, 92);
    let root_cluster = u32le(boot, 96);
    let revision = u16le(boot, 104);
    let volume_flags = u16le(boot, 106);
    let number_of_fats = boot[110];
    if revision >> 8 != 1
        || !(1..=2).contains(&number_of_fats)
        || volume_length == 0
        || volume_length > partition_sectors
        || fat_offset < 24
        || fat_length == 0
        || cluster_count == 0
        || cluster_count > 4_194_304
        || root_cluster < 2
        || root_cluster >= cluster_count + 2
    {
        return Err("invalid exFAT volume geometry".into());
    }
    if boot[112] != 0xff && boot[112] > 100 {
        return Err("invalid exFAT PercentInUse".into());
    }
    let active_fat = if number_of_fats == 2 {
        (volume_flags & 1) as u8
    } else {
        0
    };
    let fats_end = fat_offset
        .checked_add(fat_length * number_of_fats as u64)
        .ok_or_else(|| "exFAT FAT geometry overflow".to_string())?;
    let heap_end = heap_offset
        .checked_add(cluster_count as u64 * sectors_per_cluster)
        .ok_or_else(|| "exFAT cluster heap overflow".to_string())?;
    let required_fat_bytes = (cluster_count as u64 + 2) * 4;
    if heap_offset < fats_end || heap_end > volume_length || required_fat_bytes > fat_length * 512 {
        return Err("inconsistent exFAT FAT or cluster heap geometry".into());
    }
    let geometry = Geometry {
        heap_offset,
        cluster_count,
        sectors_per_cluster,
    };

    let mut boot_region = vec![boot.to_vec()];
    for sector in 1..=11 {
        boot_region.push(read(r, sector, volume_length)?);
    }
    let checksum = boot_checksum(&boot_region);
    if !boot_region[11]
        .chunks_exact(4)
        .all(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()) == checksum)
    {
        return Err("exFAT boot-region checksum mismatch".into());
    }

    let mut fat_bytes = Vec::with_capacity(required_fat_bytes as usize);
    let active_fat_offset = fat_offset + active_fat as u64 * fat_length;
    for sector in 0..required_fat_bytes.div_ceil(512) {
        fat_bytes.extend_from_slice(&read(r, active_fat_offset + sector, volume_length)?);
    }
    let values = (0..cluster_count as usize + 2)
        .map(|index| u32le(&fat_bytes, index * 4))
        .collect();
    let fat = Fat {
        values,
        count: cluster_count,
    };

    let mut claimed = BTreeSet::new();
    let root_chain = fat.chain(root_cluster, &mut claimed)?;
    let mut pending = VecDeque::from([(String::from("/"), root_chain)]);
    let mut entries = vec![FileEntry {
        path: "/".into(),
        is_directory: true,
        logical_size: 0,
        allocated_size: None,
        mtime: None,
        ctime: None,
        attributes: 0x10,
    }];
    let mut paths = BTreeSet::from([String::from("/")]);
    let mut bitmap_candidates = Vec::new();
    let mut upcase_candidates = Vec::new();
    let mut directory_bytes = 0usize;

    while let Some((path, clusters)) = pending.pop_front() {
        if clusters.len() as u64 * geometry.sectors_per_cluster > 65_536 {
            return Err("exFAT directory chain exceeds Deep budget".into());
        }
        let mut bytes = Vec::new();
        for cluster in clusters {
            let start = geometry.cluster_lba(cluster)?;
            for sector in 0..geometry.sectors_per_cluster {
                bytes.extend_from_slice(&read(r, start + sector, volume_length)?);
                directory_bytes += 512;
                if directory_bytes > 32 * 1024 * 1024 {
                    return Err("exFAT directory metadata exceeds Deep budget".into());
                }
            }
        }
        let mut offset = 0usize;
        while offset + 32 <= bytes.len() {
            let entry_type = bytes[offset];
            if entry_type == 0x00 {
                break;
            }
            if entry_type & 0x80 == 0 {
                offset += 32;
                continue;
            }
            match entry_type {
                0x81 if path == "/" => {
                    let flags = bytes[offset + 1];
                    if flags & !1 != 0 {
                        return Err("invalid exFAT allocation-bitmap flags".into());
                    }
                    bitmap_candidates.push((
                        flags & 1,
                        SystemStream {
                            first_cluster: u32le(&bytes, offset + 20),
                            data_length: u64le(&bytes, offset + 24),
                        },
                    ));
                    offset += 32;
                }
                0x82 if path == "/" => {
                    upcase_candidates.push((
                        u32le(&bytes, offset + 4),
                        SystemStream {
                            first_cluster: u32le(&bytes, offset + 20),
                            data_length: u64le(&bytes, offset + 24),
                        },
                    ));
                    offset += 32;
                }
                0x83 if path == "/" => {
                    let count = bytes[offset + 1] as usize;
                    if count > 11 {
                        return Err("invalid exFAT volume-label length".into());
                    }
                    let units = (0..count)
                        .map(|index| u16le(&bytes, offset + 2 + index * 2))
                        .collect::<Vec<_>>();
                    String::from_utf16(&units).map_err(|_| "invalid exFAT volume label")?;
                    offset += 32;
                }
                0x85 => {
                    let secondary_count = bytes[offset + 1] as usize;
                    let set_len = (secondary_count + 1) * 32;
                    if secondary_count < 2 || offset + set_len > bytes.len() {
                        return Err("truncated exFAT file entry set".into());
                    }
                    let mut record = parse_file_set(&bytes[offset..offset + set_len])?;
                    let stream_clusters = stream_clusters(
                        record.first_cluster,
                        record.data_length,
                        record.no_fat_chain,
                        geometry,
                        &fat,
                        &mut claimed,
                    )?;
                    let allocated = stream_clusters.len() as u64 * geometry.cluster_bytes();
                    let full = format!(
                        "{path}{}{}",
                        record.entry.path,
                        if record.entry.is_directory { "/" } else { "" }
                    );
                    if full.len() > 4096
                        || !paths.insert(full.to_lowercase())
                        || entries.len() >= 100_000
                    {
                        return Err("duplicate exFAT path or directory budget exceeded".into());
                    }
                    record.entry.path = full.clone();
                    record.entry.allocated_size = Some(allocated);
                    if record.entry.logical_size > allocated {
                        return Err("exFAT file size exceeds cluster allocation".into());
                    }
                    if record.entry.is_directory {
                        pending.push_back((full, stream_clusters));
                    }
                    entries.push(record.entry);
                    offset += set_len;
                }
                // Unknown critical primary entries are not safe to ignore.
                value if value & 0x20 == 0 => {
                    return Err(format!(
                        "unsupported critical exFAT directory entry {value:#04x}"
                    ));
                }
                // Unknown benign entries may be skipped by design.
                _ => offset += 32,
            }
        }
    }

    let bitmap = bitmap_candidates
        .iter()
        .find(|(flags, _)| *flags == active_fat)
        .map(|(_, stream)| *stream)
        .ok_or_else(|| "active exFAT allocation bitmap is missing".to_string())?;
    let minimum_bitmap = (cluster_count as u64).div_ceil(8);
    if bitmap.data_length < minimum_bitmap {
        return Err("exFAT allocation bitmap is too small".into());
    }
    let bitmap_chain = fat.chain(bitmap.first_cluster, &mut claimed)?;
    let bitmap_bytes = stream_bytes(
        r,
        volume_length,
        geometry,
        &bitmap_chain,
        bitmap.data_length,
    )?;

    for (expected_checksum, upcase) in upcase_candidates {
        let chain = fat.chain(upcase.first_cluster, &mut claimed)?;
        let bytes = stream_bytes(r, volume_length, geometry, &chain, upcase.data_length)?;
        let actual = bytes
            .iter()
            .fold(0u32, |checksum, &byte| rotate_checksum32(checksum, byte));
        if actual != expected_checksum {
            return Err("exFAT upcase-table checksum mismatch".into());
        }
    }

    for &cluster in &claimed {
        let bit = cluster as usize - 2;
        if bitmap_bytes[bit / 8] & (1 << (bit % 8)) == 0 {
            return Err("referenced exFAT cluster is marked free in allocation bitmap".into());
        }
    }
    let allocated_clusters = (0..cluster_count as usize)
        .filter(|&bit| bitmap_bytes[bit / 8] & (1 << (bit % 8)) != 0)
        .count() as u64;
    let free = (cluster_count as u64 - allocated_clusters) * geometry.cluster_bytes();
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Inventory {
        total: volume_length * 512,
        free,
        entries,
    })
}
