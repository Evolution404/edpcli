use super::*;

impl AppState {
    pub(super) fn provision_display_capacity(&self, sectors: u64) -> String {
        let sector_bytes = self
            .selected_device()
            .and_then(|row| row.layout_geometry().ok())
            .map_or(crate::common::SECTOR as u32, |geometry| {
                geometry.logical_sector_bytes
            });
        sectors
            .checked_mul(u64::from(sector_bytes))
            .map(crate::common::fmt_capacity)
            .unwrap_or_else(|| "容量溢出".into())
    }

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

        let Ok((total_sectors, logical_bytes, lce_start)) = self.provision_preview_geometry()
        else {
            return DiskLayoutModel::new(0, Vec::new());
        };
        let Ok((resolved, _)) = self.provision_resolved_prefill() else {
            return DiskLayoutModel::new(0, Vec::new());
        };
        let Ok(parts) = resolved.draft_partitions(u64::from(logical_bytes)) else {
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

        if logical_bytes != crate::common::SECTOR as u32 {
            // A Plain source has no existing LCE record. Show the *target*
            // reserved extent instead of erasing the entire layout preview.
            // For registered sources preserve the independently observed LCE
            // block count. Application-level prepare remains authoritative.
            let lce_blocks =
                match crate::application::provision_geometry::native_compatibility_extent(
                    total_sectors,
                    logical_bytes,
                ) {
                    Ok(extent) => extent.sector_count,
                    Err(_) => return DiskLayoutModel::new(0, Vec::new()),
                };
            return DiskLayoutModel::canonical_edp_with_sector_bytes(
                total_sectors,
                partitions,
                lce_start,
                lce_blocks,
                logical_bytes,
            )
            .unwrap_or_else(|_| DiskLayoutModel::new(0, Vec::new()));
        }
        let lce = crate::application::provision_geometry::verified_usb_compatibility_extent(
            total_sectors,
        )
        .expect("already validated 512B compatibility extent");
        DiskLayoutModel::draft_edp(total_sectors, partitions, lce.start_lba, lce.sector_count)
    }
}
