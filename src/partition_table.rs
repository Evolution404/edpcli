//! UI-neutral physical partition-table reader shared by device scan and Inspect.
//! Read-only parsing for standard MBR, extended MBR/EBR, and GPT.

use std::collections::BTreeSet;

use crate::common::SECTOR;
use crate::protocol::{
    lba0::{self, MbrPartition},
    lba1::{self, GptHeaderState},
};

const GPT_ENTRY_SIZE: usize = 128;
const MAX_GPT_ENTRY_BYTES: usize = 8 * 1024 * 1024;
const MAX_EBR_CHAIN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionTableKind {
    Mbr,
    Gpt,
}

impl PartitionTableKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Mbr => "MBR",
            Self::Gpt => "GPT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionSource {
    Mbr {
        partition_type: u8,
        primary_slot: Option<usize>,
    },
    Gpt {
        type_guid: [u8; 16],
        unique_guid: [u8; 16],
        name: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalPartition {
    pub index: usize,
    pub start_lba: u64,
    pub sector_count: u64,
    pub source: PartitionSource,
    pub filesystem: Option<String>,
    pub volume_label: Option<String>,
}

impl PhysicalPartition {
    pub fn end_exclusive(&self) -> Option<u64> {
        self.start_lba.checked_add(self.sector_count)
    }

    pub fn type_label(&self) -> String {
        match &self.source {
            PartitionSource::Mbr { partition_type, .. } => {
                format!("0x{partition_type:02X} {}", mbr_type_name(*partition_type))
            }
            PartitionSource::Gpt {
                type_guid, name, ..
            } => {
                let mut label = gpt_type_name(type_guid).to_string();
                if let Some(name) = name.as_deref().filter(|value| !value.is_empty()) {
                    label.push_str(" · ");
                    label.push_str(name);
                }
                label
            }
        }
    }

    pub fn display_label(&self) -> String {
        match self.filesystem.as_deref() {
            Some(filesystem) => format!("P{} {filesystem}", self.index),
            None => format!("P{} {}", self.index, self.type_label()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionTableExtent {
    pub label: String,
    pub start_lba: u64,
    pub sector_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionTableSnapshot {
    pub kind: PartitionTableKind,
    pub partitions: Vec<PhysicalPartition>,
    pub table_extents: Vec<PartitionTableExtent>,
    pub issues: Vec<String>,
}

impl PartitionTableSnapshot {
    pub fn partition_for_lba(&self, lba: u64) -> Option<&PhysicalPartition> {
        self.partitions.iter().find(|partition| {
            partition.start_lba <= lba && partition.end_exclusive().is_some_and(|end| lba < end)
        })
    }
}

pub fn mbr_type_name(value: u8) -> &'static str {
    match value {
        0x00 => "空",
        0x01 => "FAT12",
        0x04 | 0x06 | 0x0e => "FAT16",
        0x05 | 0x0f | 0x85 => "扩展分区",
        0x07 => "NTFS/exFAT",
        0x0b | 0x0c => "FAT32",
        0x82 => "Linux swap",
        0x83 => "Linux",
        0xee => "GPT Protective",
        0xef => "EFI",
        _ => "其他",
    }
}

fn gpt_type_name(guid: &[u8; 16]) -> &'static str {
    match guid {
        [0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99, 0xc7] => {
            "Microsoft Basic Data"
        }
        [0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b] => {
            "EFI System"
        }
        [0xaf, 0x3d, 0xc6, 0x0f, 0x83, 0x84, 0x72, 0x47, 0x8e, 0x79, 0x3d, 0x69, 0xd8, 0x47, 0x7d, 0xe4] => {
            "Linux filesystem"
        }
        _ => "GPT 分区",
    }
}

fn is_extended_type(value: u8) -> bool {
    matches!(value, 0x05 | 0x0f | 0x85)
}

fn validate_extent(label: &str, start: u64, count: u64, total: u64) -> Result<(), String> {
    if count == 0 {
        return Err(format!("{label} sector_count=0"));
    }
    let end = start
        .checked_add(count)
        .ok_or_else(|| format!("{label} LBA 范围溢出"))?;
    if start >= total || end > total {
        return Err(format!(
            "{label} 越界: start={start} sectors={count} total={total}"
        ));
    }
    Ok(())
}

fn read_exact_sector<F>(read_sector: &mut F, lba: u64) -> Result<Vec<u8>, String>
where
    F: FnMut(u64) -> Result<Vec<u8>, String>,
{
    let data = read_sector(lba)?;
    if data.len() != SECTOR {
        return Err(format!("LBA{lba} 读取 {}B，预期 {SECTOR}B", data.len()));
    }
    Ok(data)
}

fn push_primary(
    out: &mut Vec<PhysicalPartition>,
    slot: usize,
    part: &MbrPartition,
    total: u64,
) -> Result<(), String> {
    if part.partition_type == 0
        || part.sector_count == 0
        || is_extended_type(part.partition_type)
        || part.partition_type == 0xee
    {
        return Ok(());
    }
    let start = u64::from(part.start_lba);
    let count = u64::from(part.sector_count);
    validate_extent(&format!("MBR P{}", slot + 1), start, count, total)?;
    out.push(PhysicalPartition {
        index: out.len() + 1,
        start_lba: start,
        sector_count: count,
        source: PartitionSource::Mbr {
            partition_type: part.partition_type,
            primary_slot: Some(slot + 1),
        },
        filesystem: None,
        volume_label: None,
    });
    Ok(())
}

fn parse_ebr_chain<F>(
    base: u64,
    total: u64,
    read_sector: &mut F,
    partitions: &mut Vec<PhysicalPartition>,
    extents: &mut Vec<PartitionTableExtent>,
) -> Result<(), String>
where
    F: FnMut(u64) -> Result<Vec<u8>, String>,
{
    let mut visited = BTreeSet::new();
    let mut current = base;
    for chain_index in 0..MAX_EBR_CHAIN {
        if !visited.insert(current) {
            return Err(format!("EBR 链出现循环: LBA{current}"));
        }
        let raw = read_exact_sector(read_sector, current)?;
        let raw: &[u8; SECTOR] = raw.as_slice().try_into().map_err(|_| "EBR 长度异常")?;
        let view =
            lba0::parse_lba0(raw).map_err(|error| format!("EBR LBA{current} 无效: {error:?}"))?;
        extents.push(PartitionTableExtent {
            label: format!("EBR #{}", chain_index + 1),
            start_lba: current,
            sector_count: 1,
        });

        let logical = &view.partitions[0];
        if logical.partition_type != 0 && logical.sector_count != 0 {
            let start = current
                .checked_add(u64::from(logical.start_lba))
                .ok_or_else(|| "EBR 逻辑分区起点溢出".to_string())?;
            let count = u64::from(logical.sector_count);
            validate_extent(
                &format!("逻辑分区 P{}", partitions.len() + 1),
                start,
                count,
                total,
            )?;
            partitions.push(PhysicalPartition {
                index: partitions.len() + 1,
                start_lba: start,
                sector_count: count,
                source: PartitionSource::Mbr {
                    partition_type: logical.partition_type,
                    primary_slot: None,
                },
                filesystem: None,
                volume_label: None,
            });
        }

        let next = &view.partitions[1];
        if next.partition_type == 0 || next.sector_count == 0 {
            return Ok(());
        }
        if !is_extended_type(next.partition_type) {
            return Err(format!("EBR LBA{current} 第二条目不是扩展链指针"));
        }
        current = base
            .checked_add(u64::from(next.start_lba))
            .ok_or_else(|| "EBR 下一节点起点溢出".to_string())?;
        if current >= total {
            return Err(format!("EBR 下一节点 LBA{current} 越界"));
        }
    }
    Err(format!("EBR 链超过 {MAX_EBR_CHAIN} 个节点"))
}

fn decode_utf16_name(entry: &[u8]) -> Option<String> {
    let units = entry
        .get(56..128)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|bytes| u16::from_le_bytes(*bytes))
        .take_while(|value| *value != 0)
        .collect::<Vec<_>>();
    let value = String::from_utf16_lossy(&units).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn parse_gpt<F>(
    total: u64,
    read_sector: &mut F,
    validate_mirror: bool,
) -> Result<PartitionTableSnapshot, String>
where
    F: FnMut(u64) -> Result<Vec<u8>, String>,
{
    let raw = read_exact_sector(read_sector, 1)?;
    let raw: &[u8; SECTOR] = raw
        .as_slice()
        .try_into()
        .map_err(|_| "GPT header 长度异常")?;
    let view = lba1::parse_lba1(raw).map_err(|error| format!("GPT header 无效: {error:?}"))?;
    let GptHeaderState::Enabled(header) = view.header else {
        return Err("protective MBR 存在，但 GPT header 缺失".into());
    };
    if header.backup_lba >= total
        || header.first_usable_lba >= total
        || header.last_usable_lba >= total
        || header.first_usable_lba > header.last_usable_lba
    {
        return Err("GPT header geometry 越界".into());
    }
    if header.entry_size as usize != GPT_ENTRY_SIZE {
        return Err(format!(
            "GPT entry_size={}，当前仅支持标准 128B entry",
            header.entry_size
        ));
    }
    let entry_bytes = usize::try_from(header.entry_count)
        .ok()
        .and_then(|count| count.checked_mul(GPT_ENTRY_SIZE))
        .ok_or_else(|| "GPT partition array 大小溢出".to_string())?;
    if entry_bytes == 0 || entry_bytes > MAX_GPT_ENTRY_BYTES {
        return Err(format!(
            "GPT partition array 大小 {entry_bytes}B 超出安全上限 {MAX_GPT_ENTRY_BYTES}B"
        ));
    }
    let entry_sectors = entry_bytes.div_ceil(SECTOR);
    validate_extent(
        "GPT primary array",
        header.partition_entries_lba,
        entry_sectors as u64,
        total,
    )?;
    if header.partition_entries_lba < 2
        || header
            .partition_entries_lba
            .checked_add(entry_sectors as u64)
            .is_none_or(|end| end > header.first_usable_lba)
    {
        return Err("GPT primary array overlaps usable/header sectors".into());
    }
    let mut array = Vec::with_capacity(entry_sectors * SECTOR);
    for offset in 0..entry_sectors {
        let lba = header
            .partition_entries_lba
            .checked_add(offset as u64)
            .ok_or_else(|| "GPT partition array LBA 溢出".to_string())?;
        array.extend_from_slice(&read_exact_sector(read_sector, lba)?);
    }
    array.truncate(entry_bytes);
    header
        .validate_partition_array(&array)
        .map_err(|error| format!("GPT partition array CRC/长度无效: {error:?}"))?;

    if validate_mirror {
        if header.backup_lba != total - 1 {
            return Err("GPT backup header conflicts with disk geometry".into());
        }
        let raw = read_exact_sector(read_sector, header.backup_lba)?;
        let GptHeaderState::Enabled(backup) = lba1::parse_header_at(
            raw.as_slice()
                .try_into()
                .map_err(|_| "GPT backup header length")?,
            header.backup_lba,
        )
        .map_err(|error| format!("GPT backup header invalid: {error:?}"))?
        .header
        else {
            return Err("GPT backup header missing".into());
        };
        if backup.backup_lba != header.current_lba
            || backup.first_usable_lba != header.first_usable_lba
            || backup.last_usable_lba != header.last_usable_lba
            || backup.disk_guid != header.disk_guid
            || backup.entry_count != header.entry_count
            || backup.entry_size != header.entry_size
            || backup.partition_array_crc32 != header.partition_array_crc32
            || backup
                .partition_entries_lba
                .checked_add(entry_sectors as u64)
                != Some(backup.current_lba)
            || backup.partition_entries_lba <= header.last_usable_lba
        {
            return Err("GPT backup header geometry/CRC contract conflicts with primary".into());
        }
        let mut mirror = Vec::with_capacity(entry_sectors * SECTOR);
        for offset in 0..entry_sectors {
            mirror.extend_from_slice(&read_exact_sector(
                read_sector,
                backup.partition_entries_lba + offset as u64,
            )?);
        }
        mirror.truncate(entry_bytes);
        backup
            .validate_partition_array(&mirror)
            .map_err(|error| format!("GPT backup array CRC invalid: {error:?}"))?;
        if mirror != array {
            return Err("GPT primary and backup partition arrays differ".into());
        }
    }

    let mut partitions = Vec::new();
    for (entry_index, entry) in array.as_chunks::<GPT_ENTRY_SIZE>().0.iter().enumerate() {
        let type_guid: [u8; 16] = entry[..16].try_into().unwrap();
        if type_guid.iter().all(|byte| *byte == 0) {
            continue;
        }
        let unique_guid: [u8; 16] = entry[16..32].try_into().unwrap();
        let first_lba = u64::from_le_bytes(entry[32..40].try_into().unwrap());
        let last_lba = u64::from_le_bytes(entry[40..48].try_into().unwrap());
        if first_lba > last_lba {
            return Err(format!(
                "GPT entry {} first_lba > last_lba",
                entry_index + 1
            ));
        }
        let count = last_lba
            .checked_sub(first_lba)
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| format!("GPT entry {} sector_count 溢出", entry_index + 1))?;
        validate_extent(
            &format!("GPT P{}", entry_index + 1),
            first_lba,
            count,
            total,
        )?;
        partitions.push(PhysicalPartition {
            index: partitions.len() + 1,
            start_lba: first_lba,
            sector_count: count,
            source: PartitionSource::Gpt {
                type_guid,
                unique_guid,
                name: decode_utf16_name(entry),
            },
            filesystem: None,
            volume_label: None,
        });
    }

    let mut extents = vec![
        PartitionTableExtent {
            label: "Protective MBR".into(),
            start_lba: 0,
            sector_count: 1,
        },
        PartitionTableExtent {
            label: "GPT 主表头".into(),
            start_lba: 1,
            sector_count: 1,
        },
        PartitionTableExtent {
            label: "GPT 主分区项".into(),
            start_lba: header.partition_entries_lba,
            sector_count: entry_sectors as u64,
        },
    ];
    let backup_entries_start = header.backup_lba.saturating_sub(entry_sectors as u64);
    if backup_entries_start > header.last_usable_lba && backup_entries_start < header.backup_lba {
        extents.push(PartitionTableExtent {
            label: "GPT 备份分区项".into(),
            start_lba: backup_entries_start,
            sector_count: entry_sectors as u64,
        });
    }
    extents.push(PartitionTableExtent {
        label: "GPT 备份表头".into(),
        start_lba: header.backup_lba,
        sector_count: 1,
    });

    Ok(PartitionTableSnapshot {
        kind: PartitionTableKind::Gpt,
        partitions,
        table_extents: extents,
        issues: Vec::new(),
    })
}

pub fn read_partition_table<F>(total: u64, read_sector: F) -> Result<PartitionTableSnapshot, String>
where
    F: FnMut(u64) -> Result<Vec<u8>, String>,
{
    read_partition_table_inner(total, read_sector, false)
}

/// Captures once, validates once, and returns the exact table bytes used by the parser.
/// Backup creation and restore authorization share this bounded evidence contract.
pub(crate) fn capture_partition_table<F>(
    total: u64,
    mut read_sector: F,
) -> Result<(PartitionTableSnapshot, Vec<Vec<u8>>), String>
where
    F: FnMut(u64) -> Result<Vec<u8>, String>,
{
    let mut sectors = std::collections::BTreeMap::new();
    let mut cached = |lba| {
        if let Some(bytes) = sectors.get(&lba) {
            return Ok(Vec::clone(bytes));
        }
        if lba >= total {
            return Err("partition-table read exceeds geometry".into());
        }
        let bytes = read_sector(lba)?;
        if bytes.len() != SECTOR {
            return Err("partition-table sector length invalid".into());
        }
        sectors.insert(lba, bytes.clone());
        Ok(bytes)
    };
    let table = read_partition_table_inner(total, &mut cached, true)?;
    let mut evidence = Vec::new();
    for extent in &table.table_extents {
        let mut bytes = Vec::new();
        for offset in 0..extent.sector_count {
            bytes.extend_from_slice(&cached(
                extent
                    .start_lba
                    .checked_add(offset)
                    .ok_or("partition-table address overflow")?,
            )?);
        }
        evidence.push(bytes);
    }
    Ok((table, evidence))
}

fn read_partition_table_inner<F>(
    total: u64,
    mut read_sector: F,
    validate_mirror: bool,
) -> Result<PartitionTableSnapshot, String>
where
    F: FnMut(u64) -> Result<Vec<u8>, String>,
{
    if total == 0 {
        return Err("磁盘总扇区数为 0".into());
    }
    let raw = read_exact_sector(&mut read_sector, 0)?;
    let raw: &[u8; SECTOR] = raw.as_slice().try_into().map_err(|_| "LBA0 长度异常")?;
    let view = lba0::parse_lba0(raw).map_err(|error| format!("LBA0 不是有效 MBR: {error:?}"))?;

    if view
        .partitions
        .iter()
        .any(|partition| partition.partition_type == 0xee)
    {
        return parse_gpt(total, &mut read_sector, validate_mirror);
    }

    let mut partitions = Vec::new();
    let mut extents = vec![PartitionTableExtent {
        label: "MBR 分区表".into(),
        start_lba: 0,
        sector_count: 1,
    }];
    for (slot, partition) in view.partitions.iter().enumerate() {
        push_primary(&mut partitions, slot, partition, total)?;
    }
    for partition in view
        .partitions
        .iter()
        .filter(|partition| is_extended_type(partition.partition_type) && partition.start_lba != 0)
    {
        parse_ebr_chain(
            u64::from(partition.start_lba),
            total,
            &mut read_sector,
            &mut partitions,
            &mut extents,
        )?;
    }
    partitions.sort_by_key(|partition| partition.start_lba);
    for (index, partition) in partitions.iter_mut().enumerate() {
        partition.index = index + 1;
    }
    Ok(PartitionTableSnapshot {
        kind: PartitionTableKind::Mbr,
        partitions,
        table_extents: extents,
        issues: Vec::new(),
    })
}

pub fn confirmed_plain_protocol_prefix(protocol_image: &[u8], total: u64) -> bool {
    if protocol_image.len() < 2 * SECTOR || total == 0 {
        return false;
    }
    let raw0: &[u8; SECTOR] = match protocol_image[..SECTOR].try_into() {
        Ok(value) => value,
        Err(_) => return false,
    };
    // Some ordinary removable media are formatted as a whole-disk
    // filesystem ("superfloppy") with no MBR/GPT partition table. Accept
    // those only when the existing strict FAT/exFAT/NTFS boot-sector
    // validator confirms LBA0 against the physical whole-disk geometry.
    if crate::filesystem::detect_boot_sector(total, raw0)
        .ok()
        .flatten()
        .is_some()
    {
        return true;
    }
    let Ok(mbr) = lba0::parse_lba0(raw0) else {
        return false;
    };
    if !mbr
        .partitions
        .iter()
        .any(|partition| partition.partition_type != 0 && partition.sector_count != 0)
    {
        return false;
    }
    if mbr
        .partitions
        .iter()
        .any(|partition| partition.partition_type == 0xee)
    {
        let raw1: &[u8; SECTOR] = match protocol_image[SECTOR..2 * SECTOR].try_into() {
            Ok(value) => value,
            Err(_) => return false,
        };
        return lba1::parse_lba1(raw1)
            .is_ok_and(|view| matches!(view.header, GptHeaderState::Enabled(_)));
    }
    mbr.partitions.iter().all(|partition| {
        if partition.partition_type == 0 || partition.sector_count == 0 {
            return true;
        }
        let start = u64::from(partition.start_lba);
        let count = u64::from(partition.sector_count);
        start
            .checked_add(count)
            .is_some_and(|end| start < total && end <= total)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mbr_with_partition(start: u32, count: u32, partition_type: u8) -> [u8; SECTOR] {
        let mut sector = [0u8; SECTOR];
        let entry = 0x1be;
        sector[entry + 4] = partition_type;
        sector[entry + 8..entry + 12].copy_from_slice(&start.to_le_bytes());
        sector[entry + 12..entry + 16].copy_from_slice(&count.to_le_bytes());
        sector[510..512].copy_from_slice(&[0x55, 0xaa]);
        sector
    }

    #[test]
    fn plain_prefix_requires_a_valid_physical_partition_table() {
        let mut image = vec![0u8; 13 * SECTOR];
        image[..SECTOR].copy_from_slice(&mbr_with_partition(2048, 10_000, 0x07));
        assert!(confirmed_plain_protocol_prefix(&image, 12_048));

        // Low LBAs may legally contain non-zero non-EDP data. Plain classification
        // is gated by the caller's absence of EDP device_id evidence.
        image[7 * SECTOR] = 1;
        assert!(confirmed_plain_protocol_prefix(&image, 12_048));
    }

    #[test]
    fn plain_prefix_accepts_strict_whole_disk_ntfs_superfloppy() {
        let total = 30_277_632u64;
        let mut image = vec![0u8; 13 * SECTOR];
        let boot = &mut image[..SECTOR];
        boot[..3].copy_from_slice(&[0xeb, 0x52, 0x90]);
        boot[3..11].copy_from_slice(b"NTFS    ");
        boot[11..13].copy_from_slice(&(SECTOR as u16).to_le_bytes());
        boot[13] = 8;
        boot[21] = 0xf8;
        boot[40..48].copy_from_slice(&(total - 1).to_le_bytes());
        boot[48..56].copy_from_slice(&4u64.to_le_bytes());
        boot[56..64].copy_from_slice(&8u64.to_le_bytes());
        // The legacy partition-entry byte range is boot code on a
        // superfloppy; make it explicitly non-empty so this regression
        // cannot accidentally pass through the MBR path.
        boot[0x1c2] = 0x99;
        boot[0x1c6..0x1ca].copy_from_slice(&u32::MAX.to_le_bytes());
        boot[0x1ca..0x1ce].copy_from_slice(&u32::MAX.to_le_bytes());
        boot[510..512].copy_from_slice(&[0x55, 0xaa]);

        assert!(confirmed_plain_protocol_prefix(&image, total));
    }

    #[test]
    fn reads_standard_mbr_partition() {
        let mbr = mbr_with_partition(2048, 10_000, 0x07);
        let table = read_partition_table(12_048, |lba| {
            if lba == 0 {
                Ok(mbr.to_vec())
            } else {
                Ok(vec![0; SECTOR])
            }
        })
        .unwrap();
        assert_eq!(table.kind, PartitionTableKind::Mbr);
        assert_eq!(table.partitions.len(), 1);
        assert_eq!(table.partitions[0].start_lba, 2048);
        assert_eq!(table.partitions[0].sector_count, 10_000);
    }
}

#[cfg(test)]
mod extended_tests {
    use super::*;

    fn mbr(entries: &[(usize, u8, u32, u32)]) -> [u8; SECTOR] {
        let mut sector = [0u8; SECTOR];
        for (slot, partition_type, start, count) in entries {
            let off = 0x1be + slot * 16;
            sector[off + 4] = *partition_type;
            sector[off + 8..off + 12].copy_from_slice(&start.to_le_bytes());
            sector[off + 12..off + 16].copy_from_slice(&count.to_le_bytes());
        }
        sector[510..512].copy_from_slice(&[0x55, 0xaa]);
        sector
    }

    #[test]
    fn follows_single_ebr_logical_partition() {
        let root = mbr(&[(0, 0x0f, 2048, 10_000)]);
        let ebr = mbr(&[(0, 0x07, 63, 1_000)]);
        let table = read_partition_table(20_000, |lba| match lba {
            0 => Ok(root.to_vec()),
            2048 => Ok(ebr.to_vec()),
            _ => Ok(vec![0; SECTOR]),
        })
        .unwrap();

        assert_eq!(table.kind, PartitionTableKind::Mbr);
        assert_eq!(table.partitions.len(), 1);
        assert_eq!(table.partitions[0].start_lba, 2111);
        assert_eq!(table.partitions[0].sector_count, 1_000);
        assert!(table
            .table_extents
            .iter()
            .any(|extent| extent.label == "EBR #1" && extent.start_lba == 2048));
    }

    #[test]
    fn reads_standard_gpt_partition_array_and_crc() {
        let total = 20_000u64;
        let protective = mbr(&[(0, 0xee, 1, (total - 1) as u32)]);

        let mut entries = vec![0u8; 4 * GPT_ENTRY_SIZE];
        entries[..16].copy_from_slice(&[
            0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26,
            0x99, 0xc7,
        ]);
        entries[16..32].copy_from_slice(&[0x11; 16]);
        entries[32..40].copy_from_slice(&2048u64.to_le_bytes());
        entries[40..48].copy_from_slice(&19_000u64.to_le_bytes());
        for (index, unit) in "DATA".encode_utf16().enumerate() {
            let off = 56 + index * 2;
            entries[off..off + 2].copy_from_slice(&unit.to_le_bytes());
        }
        let entries_crc = crate::protocol::lba1::crc32_ieee(&entries);

        let mut header = [0u8; SECTOR];
        header[..8].copy_from_slice(b"EFI PART");
        header[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        header[12..16].copy_from_slice(&92u32.to_le_bytes());
        header[24..32].copy_from_slice(&1u64.to_le_bytes());
        header[32..40].copy_from_slice(&(total - 1).to_le_bytes());
        header[40..48].copy_from_slice(&34u64.to_le_bytes());
        header[48..56].copy_from_slice(&(total - 34).to_le_bytes());
        header[56..72].copy_from_slice(&[0x22; 16]);
        header[72..80].copy_from_slice(&2u64.to_le_bytes());
        header[80..84].copy_from_slice(&4u32.to_le_bytes());
        header[84..88].copy_from_slice(&(GPT_ENTRY_SIZE as u32).to_le_bytes());
        header[88..92].copy_from_slice(&entries_crc.to_le_bytes());
        let header_crc = crate::protocol::lba1::crc32_ieee(&header[..92]);
        header[16..20].copy_from_slice(&header_crc.to_le_bytes());

        let table = read_partition_table(total, |lba| match lba {
            0 => Ok(protective.to_vec()),
            1 => Ok(header.to_vec()),
            2 => Ok(entries.clone()),
            _ => Ok(vec![0; SECTOR]),
        })
        .unwrap();

        assert_eq!(table.kind, PartitionTableKind::Gpt);
        assert_eq!(table.partitions.len(), 1);
        assert_eq!(table.partitions[0].start_lba, 2048);
        assert_eq!(table.partitions[0].sector_count, 19_000 - 2048 + 1);
        assert_eq!(table.partitions[0].filesystem, None);
        assert!(table.partitions[0]
            .type_label()
            .contains("Microsoft Basic Data"));
        assert!(table.partitions[0].type_label().contains("DATA"));
    }

    #[test]
    fn zero_or_random_metadata_without_a_valid_partition_table_is_not_plain() {
        let zero = vec![0u8; 13 * SECTOR];
        assert!(!confirmed_plain_protocol_prefix(&zero, 20_000));

        let mut random = vec![0u8; 13 * SECTOR];
        random[4 * SECTOR] = 0x55;
        assert!(!confirmed_plain_protocol_prefix(&random, 20_000));
    }
}
