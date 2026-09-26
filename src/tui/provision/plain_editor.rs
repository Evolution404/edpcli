use super::{toggle_supported_fs, PlainPartitionForm, PlainProvisionForm};

pub(super) fn plain_field_parts(slot: usize) -> Option<(usize, usize)> {
    let relative = slot.checked_sub(100)?;
    let partition = relative / 4;
    let field = relative % 4;
    (partition < crate::provision::MAX_PLAIN_PARTITIONS).then_some((partition, field))
}

impl PlainProvisionForm {
    fn specs(&self) -> Result<Vec<crate::provision::PlainPartitionSpec>, String> {
        self.partitions
            .iter()
            .enumerate()
            .map(|(index, part)| {
                let number = index + 1;
                let start_lba = part
                    .start_lba
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| format!("P{number} 起点 LBA 必须是整数"))?;
                let sector_count = part.resolve_sector_count(&format!("P{number} 容量"))?;
                Ok(crate::provision::PlainPartitionSpec::new(
                    start_lba,
                    sector_count,
                    part.filesystem,
                    part.volume_label.trim(),
                ))
            })
            .collect()
    }

    pub(super) fn plan(
        &self,
        total_sectors: u64,
    ) -> Result<crate::provision::PlainProvisionPlan, String> {
        crate::provision::PlainProvisionPlan::new(total_sectors, self.specs()?)
    }

    pub(super) fn fill_partition_capacity(
        &mut self,
        total_sectors: u64,
        partition: usize,
    ) -> Result<(), String> {
        // The selected capacity is deliberately replaced with a placeholder: the
        // user can recover a cleared or invalid capacity by pressing f.
        let specs = self
            .partitions
            .iter()
            .enumerate()
            .map(|(index, part)| {
                let start_lba = part
                    .start_lba
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| format!("P{} 起点 LBA 必须是整数", index + 1))?;
                Ok(crate::provision::PlainPartitionSpec::new(
                    start_lba,
                    1,
                    part.filesystem,
                    part.volume_label.clone(),
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let max_sectors =
            crate::provision::max_plain_sector_count(total_sectors, &specs, partition)?;
        if max_sectors == 0 {
            return Err("当前普通分区没有可填满的空间".into());
        }
        if let Some(part) = self.partitions.get_mut(partition) {
            part.set_sector_count(max_sectors);
        }
        Ok(())
    }

    pub(super) fn toggle_partition_option(
        &mut self,
        partition: usize,
        field: usize,
    ) -> Result<bool, String> {
        let Some(part) = self.partitions.get_mut(partition) else {
            return Ok(false);
        };
        match field {
            1 => {
                part.cycle_capacity_unit()?;
                Ok(true)
            }
            2 => {
                part.filesystem = toggle_supported_fs(part.filesystem);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub(super) fn add_partition(&mut self, total_sectors: Option<u64>) -> Result<usize, String> {
        if self.partitions.len() >= crate::provision::MAX_PLAIN_PARTITIONS {
            return Err("普通盘最多支持 4 个 MBR 主分区".into());
        }
        let total_sectors =
            total_sectors.ok_or_else(|| "先修正当前布局: 目标 USB 已不存在".to_string())?;
        let plan = self
            .plan(total_sectors)
            .map_err(|message| format!("先修正当前布局: {message}"))?;
        let next_start = plan
            .partitions
            .iter()
            .filter_map(|part| part.end_exclusive().ok())
            .max()
            .unwrap_or(crate::provision::DEFAULT_PLAIN_START_LBA);
        if next_start >= plan.total_sectors {
            return Err("当前最后一个分区已占满盘尾；请先缩小它再添加分区".into());
        }
        let number = self.partitions.len() + 1;
        let spec = crate::provision::PlainPartitionSpec::new(
            next_start,
            plan.total_sectors - next_start,
            crate::provision::OfficialFilesystemFormat::ExFat,
            format!("普通卷{number}"),
        );
        self.partitions.push(PlainPartitionForm::from_spec(&spec));
        Ok(number - 1)
    }

    pub(super) fn delete_partition(&mut self, partition: Option<usize>) -> Result<bool, String> {
        if self.partitions.len() <= 1 {
            return Err("普通盘至少保留 1 个分区".into());
        }
        let Some(partition) = partition else {
            return Ok(false);
        };
        if partition >= self.partitions.len() {
            return Ok(false);
        }
        self.partitions.remove(partition);
        Ok(true)
    }
}
