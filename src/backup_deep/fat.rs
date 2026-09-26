//! FAT BPB, allocation tables and directory metadata; never file payloads.
//! Layout: Microsoft FAT specification v1.03 (docs/backup/DEEP_BACKUP_V1.md).
use super::{FileEntry, PartitionReader};
use std::collections::{BTreeSet, VecDeque};

pub(super) struct Inventory {
    pub kind: &'static str,
    pub total: u64,
    pub free: u64,
    pub entries: Vec<FileEntry>,
}

#[cfg(test)]
mod boot_bounds_tests {
    use super::*;
    use std::io;

    struct UnusedReader;

    impl PartitionReader for UnusedReader {
        fn read_sector(&mut self, _: u64) -> io::Result<Vec<u8>> {
            panic!("short boot must fail before sector reads")
        }
    }

    #[test]
    fn short_boot_is_a_parse_error() {
        for length in [0, 1, 510, 511] {
            assert_eq!(
                parse(&mut UnusedReader, 1, &vec![0; length])
                    .err()
                    .as_deref(),
                Some("truncated FAT boot sector")
            );
        }
    }
}
// Callers have exact 512-byte sectors or complete directory entries, and
// offsets are fixed FAT layout fields.
fn u16le(b: &[u8], n: usize) -> u16 {
    u16::from_le_bytes([b[n], b[n + 1]])
}
fn u32le(b: &[u8], n: usize) -> u32 {
    u32::from_le_bytes(b[n..n + 4].try_into().unwrap())
}
fn read(r: &mut dyn PartitionReader, lba: u64, total: u64) -> Result<Vec<u8>, String> {
    if lba >= total {
        return Err("filesystem sector out of bounds".into());
    }
    let b = r
        .read_sector(lba)
        .map_err(|e| format!("read relative LBA {lba}: {e}"))?;
    if b.len() != 512 {
        return Err("truncated filesystem sector".into());
    }
    Ok(b)
}
struct Fat {
    values: Vec<u32>,
    count: u32,
    eoc: u32,
    reserved: u32,
}
impl Fat {
    fn chain(&self, first: u32, used: &mut BTreeSet<u32>) -> Result<Vec<u32>, String> {
        let mut out = Vec::new();
        let mut c = first;
        loop {
            if c < 2 || c >= self.count + 2 || c >= self.reserved {
                return Err("FAT cluster out of bounds".into());
            }
            if !used.insert(c) {
                return Err("cyclic or cross-linked FAT chain".into());
            }
            out.push(c);
            let next = self.values[c as usize];
            if next >= self.eoc {
                return Ok(out);
            }
            if next == 0 || next >= self.reserved {
                return Err("invalid/free/bad FAT chain link".into());
            }
            c = next;
        }
    }
}

pub(super) fn parse(
    r: &mut dyn PartitionReader,
    partition_sectors: u64,
    boot: &[u8],
) -> Result<Inventory, String> {
    if boot.len() != 512 {
        return Err("truncated FAT boot sector".into());
    }
    if boot[510..512] != [0x55, 0xaa] || u16le(boot, 11) != 512 {
        return Err("invalid FAT boot signature or sector size".into());
    }
    let spc = boot[13] as u64;
    let reserved = u16le(boot, 14) as u64;
    let copies = boot[16] as u64;
    if spc == 0
        || !spc.is_power_of_two()
        || spc > 128
        || reserved == 0
        || !(1..=2).contains(&copies)
    {
        return Err("invalid FAT cluster or table geometry".into());
    }
    let root_entries = u16le(boot, 17) as u64;
    let root_sectors = (root_entries * 32).div_ceil(512);
    let fat16 = u16le(boot, 22) as u64;
    let fat_sectors = if fat16 != 0 {
        fat16
    } else {
        u32le(boot, 36) as u64
    };
    let total16 = u16le(boot, 19) as u64;
    let total = if total16 != 0 {
        total16
    } else {
        u32le(boot, 32) as u64
    };
    let data = reserved + copies * fat_sectors + root_sectors;
    if total > partition_sectors || total <= data || fat_sectors == 0 {
        return Err("FAT volume exceeds partition or has invalid data region".into());
    }
    let clusters = (total - data) / spc;
    // Explicit work budget: 4M clusters / 16MiB FAT, 64MiB read evidence.
    if !(4085..=4_194_304).contains(&clusters) {
        return Err("unsupported FAT12 or cluster count exceeds Deep budget".into());
    }
    let is32 = clusters >= 65525;
    if (is32 && (fat16 != 0 || root_entries != 0 || u16le(boot, 42) != 0))
        || (!is32 && (fat16 == 0 || root_entries == 0 || !root_entries.is_multiple_of(16)))
    {
        return Err("inconsistent FAT type and BPB".into());
    }
    let width = if is32 { 4 } else { 2 };
    let required = (clusters + 2) * width;
    if required > fat_sectors * 512 {
        return Err("FAT too small for cluster count".into());
    }
    let flags = if is32 { u16le(boot, 40) } else { 0 };
    let mirrored = flags & 0x80 == 0;
    let active = if mirrored { 0 } else { (flags & 15) as u64 };
    if active >= copies {
        return Err("invalid active FAT index".into());
    }
    let mut bytes = Vec::new();
    for i in 0..required.div_ceil(512) {
        let b = read(r, reserved + active * fat_sectors + i, total)?;
        if mirrored {
            for other in 1..copies {
                if b != read(r, reserved + other * fat_sectors + i, total)? {
                    return Err("FAT copies disagree".into());
                }
            }
        }
        bytes.extend_from_slice(&b);
    }
    let values: Vec<u32> = (0..clusters as usize + 2)
        .map(|i| {
            if is32 {
                u32le(&bytes, i * 4) & 0x0fffffff
            } else {
                u16le(&bytes, i * 2) as u32
            }
        })
        .collect();
    let fat = Fat {
        values,
        count: clusters as u32,
        eoc: if is32 { 0x0ffffff8 } else { 0xfff8 },
        reserved: if is32 { 0x0ffffff0 } else { 0xfff0 },
    };
    let free = fat.values[2..].iter().filter(|&&v| v == 0).count() as u64 * spc * 512;
    let mut claimed = BTreeSet::new();
    let root_chain = if is32 {
        fat.chain(u32le(boot, 44) & 0x0fffffff, &mut claimed)?
    } else {
        vec![]
    };
    let mut pending = VecDeque::from([(String::from("/"), root_chain)]);
    let mut entries = vec![FileEntry {
        path: "/".into(),
        is_directory: true,
        logical_size: 0,
        allocated_size: None,
        mtime: None,
        ctime: None,
        attributes: 16,
    }];
    let mut paths = BTreeSet::from([String::from("/")]);
    let mut dir_bytes = 0usize;
    while let Some((path, chain)) = pending.pop_front() {
        if chain.len() as u64 * spc > 65_536 {
            return Err("directory chain exceeds Deep budget".into());
        }
        let lbas: Vec<u64> = if !is32 && path == "/" {
            (reserved + copies * fat_sectors..data).collect()
        } else {
            chain
                .iter()
                .flat_map(|&c| {
                    let start = data + (c as u64 - 2) * spc;
                    start..start + spc
                })
                .collect()
        };
        let mut lfn = LongName::default();
        let mut ended = false;
        for lba in lbas {
            dir_bytes += 512;
            if dir_bytes > 32 * 1024 * 1024 {
                return Err("directory metadata exceeds Deep budget".into());
            }
            let sector = read(r, lba, total)?;
            for e in sector.as_chunks::<32>().0 {
                if e[0] == 0 {
                    if !lfn.parts.is_empty() {
                        return Err("orphaned long filename".into());
                    }
                    ended = true;
                    break;
                }
                if e[0] == 0xe5 {
                    lfn = LongName::default();
                    continue;
                }
                if e[11] == 15 {
                    lfn.push(e)?;
                    continue;
                }
                if e[11] & 0xc0 != 0 {
                    return Err("invalid directory attributes".into());
                }
                if e[11] & 8 != 0 {
                    if e[11] & 16 != 0 || !lfn.parts.is_empty() {
                        return Err("invalid volume entry".into());
                    }
                    continue;
                }
                if &e[..11] == b".          " || &e[..11] == b"..         " {
                    if !lfn.parts.is_empty() || e[11] & 16 == 0 {
                        return Err("invalid dot entry".into());
                    }
                    continue;
                }
                let name = lfn.finish(e)?;
                if name.is_empty()
                    || name == "."
                    || name == ".."
                    || name.chars().any(|c| c < ' ' || "/\\:*?\"<>|".contains(c))
                {
                    return Err("invalid directory name".into());
                }
                let is_dir = e[11] & 16 != 0;
                let full = format!("{path}{name}{}", if is_dir { "/" } else { "" });
                if full.len() > 4096
                    || !paths.insert(full.to_lowercase())
                    || entries.len() >= 100_000
                {
                    return Err("duplicate path or directory budget exceeded".into());
                }
                let first =
                    u16le(e, 26) as u32 | if is32 { (u16le(e, 20) as u32) << 16 } else { 0 };
                let size = u32le(e, 28) as u64;
                if is_dir && size != 0 {
                    return Err("invalid directory size".into());
                }
                let chain = if first == 0 && !is_dir && size == 0 {
                    vec![]
                } else {
                    fat.chain(first, &mut claimed)?
                };
                let allocated = chain.len() as u64 * spc * 512;
                if size > allocated {
                    return Err("file size exceeds cluster allocation".into());
                }
                entries.push(FileEntry {
                    path: full.clone(),
                    is_directory: is_dir,
                    logical_size: size,
                    allocated_size: Some(allocated),
                    mtime: timestamp(u16le(e, 24), u16le(e, 22), 0),
                    ctime: timestamp(u16le(e, 16), u16le(e, 14), e[13]),
                    attributes: e[11] as u32,
                });
                if is_dir {
                    pending.push_back((full, chain));
                }
            }
            if ended {
                break;
            }
        }
        if !lfn.parts.is_empty() {
            return Err("truncated long filename".into());
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Inventory {
        kind: if is32 { "fat32" } else { "fat16" },
        total: total * 512,
        free,
        entries,
    })
}

fn timestamp(date: u16, time: u16, centis: u8) -> Option<String> {
    if date == 0 || centis > 199 {
        return None;
    }
    let y = 1980 + (date >> 9) as i32;
    let m = ((date >> 5) & 15) as u8;
    let d = (date & 31) as u8;
    let h = time >> 11;
    let min = (time >> 5) & 63;
    let sec = (time & 31) * 2 + (centis / 100) as u16;
    time::Date::from_calendar_date(y, time::Month::try_from(m).ok()?, d).ok()?;
    if h > 23 || min > 59 || sec > 59 {
        return None;
    }
    Some(format!(
        "{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{sec:02}.{:02}",
        centis % 100
    ))
}

#[derive(Default)]
struct LongName {
    parts: Vec<Vec<u16>>,
    next: u8,
    checksum: u8,
}
impl LongName {
    fn push(&mut self, e: &[u8]) -> Result<(), String> {
        let ord = e[0] & 31;
        if ord == 0 || ord > 20 || e[0] & 0xa0 != 0 || e[12] != 0 || u16le(e, 26) != 0 {
            return Err("invalid long filename entry".into());
        }
        if e[0] & 0x40 != 0 {
            if !self.parts.is_empty() {
                return Err("overlapping long filename entries".into());
            }
            self.next = ord;
            self.checksum = e[13];
        }
        if self.next != ord || self.checksum != e[13] {
            return Err("long filename sequence/checksum mismatch".into());
        }
        self.parts.push(
            [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30]
                .iter()
                .map(|&i| u16le(e, i))
                .collect(),
        );
        self.next -= 1;
        Ok(())
    }
    fn finish(&mut self, e: &[u8]) -> Result<String, String> {
        if self.parts.is_empty() {
            if e[..11].iter().any(|&c| !(32..128).contains(&c)) {
                return Err("non-ASCII short name needs an explicit OEM code page".into());
            }
            let mut base = std::str::from_utf8(&e[..8])
                .map_err(|_| "invalid ASCII short filename")?
                .trim_end()
                .to_string();
            let mut ext = std::str::from_utf8(&e[8..11])
                .map_err(|_| "invalid ASCII short filename extension")?
                .trim_end()
                .to_string();
            if e[12] & 8 != 0 {
                base = base.to_ascii_lowercase();
            }
            if e[12] & 16 != 0 {
                ext = ext.to_ascii_lowercase();
            }
            return Ok(if ext.is_empty() {
                base
            } else {
                format!("{base}.{ext}")
            });
        }
        let check = e[..11]
            .iter()
            .fold(0u8, |sum, &c| sum.rotate_right(1).wrapping_add(c));
        if self.next != 0 || self.checksum != check {
            return Err("long filename does not match short name".into());
        }
        let units: Vec<u16> = self.parts.iter().rev().flatten().copied().collect();
        let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
        if end > 255
            || units[end.saturating_add(1).min(units.len())..]
                .iter()
                .any(|&c| c != 0xffff)
        {
            return Err("invalid long filename padding".into());
        }
        let name = String::from_utf16(&units[..end]).map_err(|_| "invalid UTF-16 long filename")?;
        *self = Self::default();
        Ok(name)
    }
}
