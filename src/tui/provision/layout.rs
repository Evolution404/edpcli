use super::*;

impl AppState {
    pub(crate) fn format_sector_size(sectors: u64) -> String {
        crate::common::fmt_capacity_sectors(sectors)
    }

    pub fn provision_layout_model(&self) -> crate::tui::disk_layout::DiskLayoutModel {
        use crate::tui::disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind};

        if self.provision.kind == ProvisionKind::Plain {
            let Ok(plan) = self.provision_plain_plan() else {
                return DiskLayoutModel::new(0, Vec::new());
            };
            let partitions = plan
                .partitions
                .iter()
                .enumerate()
                .map(|(index, part)| DiskLayoutSegment {
                    label: format!("普通分区[{}]", index + 1),
                    start_lba: part.start_lba,
                    sector_count: part.sector_count,
                    kind: DiskRegionKind::Plain,
                })
                .collect();
            return DiskLayoutModel::canonical_plain_plan(plan.total_sectors, partitions)
                .unwrap_or_else(|_| DiskLayoutModel::new(0, Vec::new()));
        }

        let total_sectors = self
            .selected_device()
            .map(|row| row.size / crate::common::SECTOR as u64)
            .unwrap_or_default();
        if total_sectors == 0 {
            return DiskLayoutModel::new(0, Vec::new());
        }
        let Some(lce) = crate::application::provision_geometry::verified_usb_compatibility_extent(
            total_sectors,
        ) else {
            return DiskLayoutModel::new(0, Vec::new());
        };
        let Ok((resolved, _)) = self.provision_resolved_prefill() else {
            return DiskLayoutModel::new(0, Vec::new());
        };
        let Ok(parts) = resolved.draft_partitions(crate::common::SECTOR as u64) else {
            return DiskLayoutModel::new(0, Vec::new());
        };
        let partitions = parts
            .into_iter()
            .map(|part| DiskLayoutSegment {
                label: part.role.label().into(),
                start_lba: part.start_lba,
                sector_count: part.sector_count,
                kind: DiskRegionKind::from_partition_role(part.role),
            })
            .collect();

        DiskLayoutModel::draft_edp(total_sectors, partitions, lce.start_lba, lce.sector_count)
    }
}
