use std::collections::BTreeMap;

use crate::common::{METADATA_LAST_LBA, SECTOR};

use super::{build_empty_exfat, build_empty_fat16, OfficialFilesystemFormat};

pub const DEFAULT_PLAIN_START_LBA: u64 = 2048;
pub const MAX_PLAIN_PARTITIONS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlainCleanupExtent {
    pub start_lba: u64,
    pub sector_count: u64,
}

impl PlainCleanupExtent {
    pub const fn new(start_lba: u64, sector_count: u64) -> Self {
        Self {
            start_lba,
            sector_count,
        }
    }

    fn end_exclusive(self) -> Result<u64, String> {
        self.start_lba
            .checked_add(self.sector_count)
            .ok_or_else(|| "旧 LCE 清理范围溢出".to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlainSectorOwner {
    Mbr,
    EdpMetadataCleanup,
    LceCleanup,
    Filesystem { partition_index: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlainSectorWrite {
    pub bytes: [u8; SECTOR],
    pub owner: PlainSectorOwner,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlainProvisionWritePlan {
    pub total_sectors: u64,
    pub mbr: [u8; SECTOR],
    pub writes: BTreeMap<u32, PlainSectorWrite>,
    preserved_lbas: Vec<u32>,
}

impl PlainProvisionWritePlan {
    pub fn preserved_lbas(&self) -> &[u32] {
        &self.preserved_lbas
    }

    pub fn touched_sector_count(&self) -> usize {
        self.writes.len()
    }

    pub fn highest_touched_lba(&self) -> Option<u32> {
        self.writes.keys().next_back().copied()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlainPartitionSpec {
    pub start_lba: u64,
    pub sector_count: u64,
    pub filesystem: OfficialFilesystemFormat,
    pub volume_label: String,
}

impl PlainPartitionSpec {
    pub fn new(
        start_lba: u64,
        sector_count: u64,
        filesystem: OfficialFilesystemFormat,
        volume_label: impl Into<String>,
    ) -> Self {
        Self {
            start_lba,
            sector_count,
            filesystem,
            volume_label: volume_label.into(),
        }
    }

    pub fn end_exclusive(&self) -> Result<u64, String> {
        self.start_lba
            .checked_add(self.sector_count)
            .ok_or_else(|| "普通分区 LBA 范围溢出".to_string())
    }

    pub fn end_lba(&self) -> Result<u64, String> {
        self.end_exclusive()?
            .checked_sub(1)
            .ok_or_else(|| "普通分区容量必须大于 0".to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlainGap {
    pub start_lba: u64,
    pub sector_count: u64,
}

impl PlainGap {
    pub const fn end_lba(self) -> u64 {
        self.start_lba + self.sector_count - 1
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlainProvisionPlan {
    pub total_sectors: u64,
    pub partitions: Vec<PlainPartitionSpec>,
    pub gaps: Vec<PlainGap>,
}

impl PlainProvisionPlan {
    pub fn default_for_disk(total_sectors: u64) -> Result<Self, String> {
        if total_sectors <= DEFAULT_PLAIN_START_LBA {
            return Err(format!(
                "磁盘只有 {total_sectors} sectors，无法从 LBA{DEFAULT_PLAIN_START_LBA} 创建普通分区"
            ));
        }
        Self::new(
            total_sectors,
            vec![PlainPartitionSpec::new(
                DEFAULT_PLAIN_START_LBA,
                total_sectors - DEFAULT_PLAIN_START_LBA,
                OfficialFilesystemFormat::ExFat,
                "普通卷",
            )],
        )
    }

    pub fn new(total_sectors: u64, partitions: Vec<PlainPartitionSpec>) -> Result<Self, String> {
        validate_plain_partitions(total_sectors, &partitions)?;
        let gaps = plain_gaps(total_sectors, &partitions)?;
        Ok(Self {
            total_sectors,
            partitions,
            gaps,
        })
    }

    pub fn max_sector_count(&self, index: usize) -> Result<u64, String> {
        max_plain_sector_count(self.total_sectors, &self.partitions, index)
    }

    pub fn fill_partition(&mut self, index: usize) -> Result<(), String> {
        let max = self.max_sector_count(index)?;
        if max == 0 {
            return Err("当前普通分区没有可填满的空间".into());
        }
        self.partitions
            .get_mut(index)
            .ok_or_else(|| "普通分区索引越界".to_string())?
            .sector_count = max;
        validate_plain_partitions(self.total_sectors, &self.partitions)
    }
}

pub fn max_plain_sector_count(
    total_sectors: u64,
    partitions: &[PlainPartitionSpec],
    index: usize,
) -> Result<u64, String> {
    let current = partitions
        .get(index)
        .ok_or_else(|| "普通分区索引越界".to_string())?;
    if current.start_lba >= total_sectors {
        return Ok(0);
    }
    let boundary = partitions
        .iter()
        .enumerate()
        .filter(|(other_index, part)| *other_index != index && part.start_lba > current.start_lba)
        .map(|(_, part)| part.start_lba)
        .min()
        .unwrap_or(total_sectors);
    Ok(boundary.saturating_sub(current.start_lba))
}

pub fn validate_plain_partitions(
    total_sectors: u64,
    partitions: &[PlainPartitionSpec],
) -> Result<(), String> {
    if partitions.is_empty() || partitions.len() > MAX_PLAIN_PARTITIONS {
        return Err(format!(
            "普通盘必须包含 1～{MAX_PLAIN_PARTITIONS} 个 MBR 主分区"
        ));
    }
    if total_sectors <= 1 {
        return Err("磁盘总扇区数不足".into());
    }

    let mut ranges = Vec::with_capacity(partitions.len());
    for (index, part) in partitions.iter().enumerate() {
        let number = index + 1;
        if part.start_lba == 0 {
            return Err(format!("P{number} 不能占用 LBA0（MBR）"));
        }
        if part.sector_count == 0 {
            return Err(format!("P{number} 容量必须大于 0"));
        }
        if part.start_lba > u32::MAX as u64 {
            return Err(format!("P{number} 起点超出 MBR 32-bit LBA 范围"));
        }
        if part.sector_count > u32::MAX as u64 {
            return Err(format!("P{number} 容量超出 MBR 32-bit sector_count 范围"));
        }
        let end_exclusive = part.end_exclusive()?;
        if end_exclusive > total_sectors {
            return Err(format!(
                "P{number} 越过磁盘末端：结束 LBA {}，磁盘末端 LBA {}",
                end_exclusive - 1,
                total_sectors - 1
            ));
        }
        if end_exclusive - 1 > u32::MAX as u64 {
            return Err(format!("P{number} 结束位置超出 MBR 32-bit LBA 范围"));
        }
        ranges.push((part.start_lba, end_exclusive, number));
    }

    ranges.sort_by_key(|(start, _, _)| *start);
    for pair in ranges.windows(2) {
        let (left_start, left_end, left_number) = pair[0];
        let (right_start, _, right_number) = pair[1];
        if left_end > right_start {
            return Err(format!(
                "P{left_number} LBA{left_start}..{} 与 P{right_number} 起点 LBA{right_start} 重叠",
                left_end - 1
            ));
        }
    }
    Ok(())
}

pub fn plain_gaps(
    total_sectors: u64,
    partitions: &[PlainPartitionSpec],
) -> Result<Vec<PlainGap>, String> {
    validate_plain_partitions(total_sectors, partitions)?;
    let mut ranges = partitions
        .iter()
        .map(|part| Ok((part.start_lba, part.end_exclusive()?)))
        .collect::<Result<Vec<_>, String>>()?;
    ranges.sort_by_key(|(start, _)| *start);

    let mut gaps = Vec::new();
    let mut cursor = 1u64; // LBA0 belongs to the MBR itself.
    for (start, end) in ranges {
        if start > cursor {
            gaps.push(PlainGap {
                start_lba: cursor,
                sector_count: start - cursor,
            });
        }
        cursor = cursor.max(end);
    }
    if cursor < total_sectors {
        gaps.push(PlainGap {
            start_lba: cursor,
            sector_count: total_sectors - cursor,
        });
    }
    Ok(gaps)
}

fn plain_mbr_partition_type(format: OfficialFilesystemFormat) -> u8 {
    match format {
        OfficialFilesystemFormat::Fat16 => 0x0e,
        OfficialFilesystemFormat::ExFat | OfficialFilesystemFormat::Ntfs => 0x07,
        OfficialFilesystemFormat::Fat32 => 0x0c,
    }
}

fn build_plain_mbr(plan: &PlainProvisionPlan) -> Result<[u8; SECTOR], String> {
    let mut mbr = [0u8; SECTOR];
    for (index, partition) in plan.partitions.iter().enumerate() {
        let entry = 0x1be + index * 16;
        mbr[entry + 4] = plain_mbr_partition_type(partition.filesystem);
        let start = u32::try_from(partition.start_lba)
            .map_err(|_| format!("P{} 起点超出 MBR 32-bit LBA 范围", index + 1))?;
        let count = u32::try_from(partition.sector_count)
            .map_err(|_| format!("P{} 容量超出 MBR 32-bit sector_count 范围", index + 1))?;
        mbr[entry + 8..entry + 12].copy_from_slice(&start.to_le_bytes());
        mbr[entry + 12..entry + 16].copy_from_slice(&count.to_le_bytes());
    }
    mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
    Ok(mbr)
}

fn plain_filesystem_image(
    partition: &PlainPartitionSpec,
    volume_serial: u32,
) -> Result<super::SparseFilesystemImage, String> {
    match partition.filesystem {
        OfficialFilesystemFormat::Fat16 => build_empty_fat16(
            partition.start_lba,
            partition.sector_count,
            volume_serial,
            &partition.volume_label,
        ),
        OfficialFilesystemFormat::ExFat => build_empty_exfat(
            partition.start_lba,
            partition.sector_count,
            volume_serial,
            &partition.volume_label,
        ),
        OfficialFilesystemFormat::Fat32 | OfficialFilesystemFormat::Ntfs => Err(format!(
            "Plain portable writer 尚未实现 {} 文件系统",
            partition.filesystem.config_token()
        )),
    }
}

fn insert_plain_write(
    writes: &mut BTreeMap<u32, PlainSectorWrite>,
    lba: u64,
    bytes: [u8; SECTOR],
    owner: PlainSectorOwner,
    total_sectors: u64,
) -> Result<(), String> {
    if lba >= total_sectors {
        return Err(format!("Plain 写计划 LBA{lba} 超过目标磁盘末端"));
    }
    let lba = u32::try_from(lba).map_err(|_| format!("Plain 写计划 LBA{lba} 超出 u32"))?;
    if lba == 3 {
        return Err("Plain 写计划禁止覆盖需要 byte-for-byte preserve 的 LBA3".into());
    }
    writes.insert(lba, PlainSectorWrite { bytes, owner });
    Ok(())
}

pub fn build_plain_provision_write_plan(
    plan: &PlainProvisionPlan,
    source_lce: Option<PlainCleanupExtent>,
    volume_serials: &[u32],
) -> Result<PlainProvisionWritePlan, String> {
    validate_plain_partitions(plan.total_sectors, &plan.partitions)?;
    if plan.total_sectors <= u64::from(METADATA_LAST_LBA) {
        return Err("普通盘目标太小，无法完整清理 LBA0-12 EDP 元数据".into());
    }
    if volume_serials.len() != plan.partitions.len() {
        return Err(format!(
            "普通盘文件系统序列号数量 {} 与分区数量 {} 不一致",
            volume_serials.len(),
            plan.partitions.len()
        ));
    }
    for (index, partition) in plan.partitions.iter().enumerate() {
        if partition.start_lba <= 3 && partition.end_exclusive()? > 3 {
            return Err(format!(
                "P{} 覆盖 LBA3；Plain 必须 byte-for-byte 保留制造商 LBA3",
                index + 1
            ));
        }
    }

    let mbr = build_plain_mbr(plan)?;
    let mut writes = BTreeMap::new();

    for lba in 1..=u64::from(METADATA_LAST_LBA) {
        if lba == 3 {
            continue;
        }
        insert_plain_write(
            &mut writes,
            lba,
            [0; SECTOR],
            PlainSectorOwner::EdpMetadataCleanup,
            plan.total_sectors,
        )?;
    }

    if let Some(extent) = source_lce {
        let end = extent.end_exclusive()?;
        if end > plan.total_sectors {
            return Err(format!(
                "旧 LCE 清理范围 LBA{}..{} 超过目标磁盘末端 LBA{}",
                extent.start_lba,
                end.saturating_sub(1),
                plan.total_sectors - 1
            ));
        }
        for lba in extent.start_lba..end {
            if lba == 3 {
                continue;
            }
            insert_plain_write(
                &mut writes,
                lba,
                [0; SECTOR],
                PlainSectorOwner::LceCleanup,
                plan.total_sectors,
            )?;
        }
    }

    for (index, (partition, volume_serial)) in plan
        .partitions
        .iter()
        .zip(volume_serials.iter().copied())
        .enumerate()
    {
        let image = plain_filesystem_image(partition, volume_serial)?;
        for (&relative_lba, sector) in image.sectors() {
            let lba = partition
                .start_lba
                .checked_add(relative_lba)
                .ok_or_else(|| format!("P{} 文件系统 LBA 溢出", index + 1))?;
            insert_plain_write(
                &mut writes,
                lba,
                *sector,
                PlainSectorOwner::Filesystem {
                    partition_index: index,
                },
                plan.total_sectors,
            )?;
        }
    }

    insert_plain_write(
        &mut writes,
        0,
        mbr,
        PlainSectorOwner::Mbr,
        plan.total_sectors,
    )?;

    Ok(PlainProvisionWritePlan {
        total_sectors: plan.total_sectors,
        mbr,
        writes,
        preserved_lbas: vec![3],
    })
}
