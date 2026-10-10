use super::{ProvisionResultPartition, ProvisionResultSnapshot};

impl ProvisionResultPartition {
    pub fn sector_count(&self, logical_sector_bytes: u32) -> u64 {
        self.size_bytes / u64::from(logical_sector_bytes)
    }

    pub fn end_exclusive(&self, logical_sector_bytes: u32) -> u64 {
        self.start_lba
            .saturating_add(self.sector_count(logical_sector_bytes))
    }

    pub fn capacity_selection(
        &self,
        logical_sector_bytes: u32,
    ) -> crate::tui::disk_layout::DiskCapacitySelection {
        crate::tui::disk_layout::DiskCapacitySelection {
            start_lba: self.start_lba,
            end_exclusive: self.end_exclusive(logical_sector_bytes),
            kind: self
                .role
                .map(crate::tui::disk_layout::DiskRegionKind::from_partition_role)
                .unwrap_or(crate::tui::disk_layout::DiskRegionKind::Plain),
        }
    }
}

impl ProvisionResultSnapshot {
    pub fn partition_selection(
        &self,
        index: usize,
    ) -> Option<crate::tui::disk_layout::DiskCapacitySelection> {
        if self.logical_sector_bytes == 0 {
            return None;
        }
        self.partitions
            .get(index)
            .map(|part| part.capacity_selection(self.logical_sector_bytes))
    }

    pub fn partition_index_for_selection(
        &self,
        selection: &crate::tui::disk_layout::DiskCapacitySelection,
    ) -> Option<usize> {
        self.partitions.iter().position(|partition| {
            self.logical_sector_bytes != 0
                && partition.capacity_selection(self.logical_sector_bytes) == *selection
        })
    }

    pub fn disk_layout_model(&self) -> Result<crate::tui::disk_layout::DiskLayoutModel, String> {
        use crate::tui::disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind};

        if !(512..=65_536).contains(&self.logical_sector_bytes)
            || !self.logical_sector_bytes.is_power_of_two()
        {
            return Err("制盘结果的原生逻辑扇区大小无效".into());
        }
        let block = u64::from(self.logical_sector_bytes);
        if !self.total_bytes.is_multiple_of(block) {
            return Err("制盘结果总容量不对齐原生扇区".into());
        }
        let total_sectors = self.total_bytes / block;
        let partitions = self
            .partitions
            .iter()
            .map(|partition| {
                if partition.size_bytes == 0 || !partition.size_bytes.is_multiple_of(block) {
                    return Err("制盘结果分区容量不对齐原生扇区".to_string());
                }
                let (label, kind) = match partition.role {
                    Some(role) => (
                        role.label().to_string(),
                        DiskRegionKind::from_partition_role(role),
                    ),
                    None => ("普通分区".into(), DiskRegionKind::Plain),
                };
                Ok(DiskLayoutSegment {
                    label,
                    start_lba: partition.start_lba,
                    sector_count: partition.size_bytes / block,
                    kind,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        match self.target {
            crate::provision::ProvisionTarget::Plain => {
                DiskLayoutModel::canonical_plain_plan(total_sectors, partitions)?
                    .with_logical_sector_bytes(self.logical_sector_bytes)
            }
            crate::provision::ProvisionTarget::Official(_) => {
                let lce = match self.lce_extent {
                    Some((start_lba, sector_count)) => {
                        crate::application::provision_geometry::ProvisionCompatibilityExtent {
                            start_lba,
                            sector_count,
                        }
                    }
                    None => crate::application::provision_geometry::native_compatibility_extent(
                        total_sectors,
                        self.logical_sector_bytes,
                    )?,
                };
                DiskLayoutModel::canonical_edp_with_sector_bytes(
                    total_sectors,
                    partitions,
                    lce.start_lba,
                    lce.sector_count,
                    self.logical_sector_bytes,
                )
            }
        }
    }
}
