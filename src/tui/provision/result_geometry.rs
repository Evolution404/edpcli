use super::{ProvisionResultPartition, ProvisionResultSnapshot};

impl ProvisionResultPartition {
    pub fn sector_count(&self) -> u64 {
        self.size_bytes / crate::common::SECTOR as u64
    }

    pub fn end_exclusive(&self) -> u64 {
        self.start_lba.saturating_add(self.sector_count())
    }

    pub fn capacity_selection(&self) -> crate::tui::disk_layout::DiskCapacitySelection {
        crate::tui::disk_layout::DiskCapacitySelection {
            start_lba: self.start_lba,
            end_exclusive: self.end_exclusive(),
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
        self.partitions
            .get(index)
            .map(ProvisionResultPartition::capacity_selection)
    }

    pub fn partition_index_for_selection(
        &self,
        selection: &crate::tui::disk_layout::DiskCapacitySelection,
    ) -> Option<usize> {
        self.partitions
            .iter()
            .position(|partition| partition.capacity_selection() == *selection)
    }

    pub fn disk_layout_model(&self) -> Result<crate::tui::disk_layout::DiskLayoutModel, String> {
        use crate::tui::disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind};

        let total_sectors = self.total_bytes / crate::common::SECTOR as u64;
        let partitions = self
            .partitions
            .iter()
            .map(|partition| {
                let (label, kind) = match partition.role {
                    Some(role) => (
                        role.label().to_string(),
                        DiskRegionKind::from_partition_role(role),
                    ),
                    None => ("普通分区".into(), DiskRegionKind::Plain),
                };
                DiskLayoutSegment {
                    label,
                    start_lba: partition.start_lba,
                    sector_count: partition.size_bytes / crate::common::SECTOR as u64,
                    kind,
                }
            })
            .collect::<Vec<_>>();

        match self.target {
            crate::provision::ProvisionTarget::Plain => {
                DiskLayoutModel::canonical_plain_plan(total_sectors, partitions)
            }
            crate::provision::ProvisionTarget::Official(_) => {
                let lce =
                    crate::application::provision_geometry::verified_usb_compatibility_extent(
                        total_sectors,
                    )
                    .ok_or_else(|| "无法重建新盘 LCE 几何".to_string())?;
                DiskLayoutModel::canonical_edp(
                    total_sectors,
                    partitions,
                    lce.start_lba,
                    lce.sector_count,
                )
            }
        }
    }
}
