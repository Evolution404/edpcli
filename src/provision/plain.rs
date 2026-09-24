use super::OfficialFilesystemFormat;

pub const DEFAULT_PLAIN_START_LBA: u64 = 2048;
pub const MAX_PLAIN_PARTITIONS: usize = 4;

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
