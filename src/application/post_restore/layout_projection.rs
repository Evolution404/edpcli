//! UI-neutral projection of verified post-restore metadata into the shared whole-disk model.
//!
//! Restore success does not depend on this projection. Callers keep the Result so a
//! presentation layer can surface a projection warning without reclassifying a
//! successfully verified metadata transaction as failed.

use crate::application::disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind};
use crate::diskio::SectorDev;
use crate::edpb::ManifestPartition;
use crate::provision::{parse_existing_provision, ProvisionImage};

fn plain_layout(
    total_sectors: u64,
    partitions: &[ManifestPartition],
) -> Result<DiskLayoutModel, String> {
    let segments = partitions
        .iter()
        .map(|partition| DiskLayoutSegment {
            label: partition
                .role
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| format!("普通分区 P{}", partition.index)),
            start_lba: partition.start_lba,
            sector_count: partition.sector_count,
            kind: DiskRegionKind::Plain,
        })
        .collect::<Vec<_>>();
    DiskLayoutModel::canonical_plain_plan(total_sectors, segments)
        .map_err(|error| format!("恢复后普通盘布局不可用: {error}"))
}

fn edp_layout(
    dev: &mut dyn SectorDev,
    device_id: &str,
    total_sectors: u64,
) -> Result<DiskLayoutModel, String> {
    if device_id.is_empty() {
        return Err("缺少 device_id，不能投影恢复后的 EDP 布局".into());
    }

    let raw_protocol = super::read_protocol_image(dev)?;
    let image = ProvisionImage::from_bytes(raw_protocol)?;
    let parsed = parse_existing_provision(&image, device_id, total_sectors)?
        .ok_or_else(|| "恢复后的 LBA7/LBA12 未形成有效 EDPF 注册".to_string())?;

    let partitions = parsed
        .profile
        .partitions
        .iter()
        .map(|partition| DiskLayoutSegment {
            label: partition.role.label().into(),
            start_lba: partition.start_lba,
            sector_count: partition.sector_count,
            kind: DiskRegionKind::from_partition_role(partition.role),
        })
        .collect::<Vec<_>>();

    let lce =
        crate::application::provision_geometry::verified_usb_compatibility_extent(total_sectors)
            .ok_or_else(|| "无法从已验证整盘容量确定恢复后的 LCE 几何".to_string())?;

    DiskLayoutModel::canonical_edp(total_sectors, partitions, lce.start_lba, lce.sector_count)
        .map_err(|error| format!("恢复后 EDP 全盘布局不可用: {error}"))
}

pub fn project_restored_layout_readonly(
    dev: &mut dyn SectorDev,
    device_state: &str,
    device_id: &str,
    total_sectors: u64,
    partitions: &[ManifestPartition],
) -> Result<DiskLayoutModel, String> {
    if device_state.eq_ignore_ascii_case("plain") {
        plain_layout(total_sectors, partitions)
    } else {
        edp_layout(dev, device_id, total_sectors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_projection_covers_mbr_partitions_and_free_space() {
        let partitions = vec![ManifestPartition {
            index: 1,
            role: None,
            partition_type: Some("mbr:0x07".into()),
            start_lba: 2_048,
            sector_count: 4_096,
            filesystem_hint: Some("exfat".into()),
            volume_label_hint: None,
        }];
        let model = plain_layout(20_000, &partitions).expect("plain layout");

        model.validate_complete().expect("complete disk layout");
        assert_eq!(model.total_sectors, 20_000);
        assert_eq!(model.segments[0].kind, DiskRegionKind::Metadata);
        assert_eq!(model.segments[0].start_lba, 0);
        assert_eq!(model.segments[0].sector_count, 1);
        assert!(model
            .segments
            .iter()
            .any(|segment| segment.kind == DiskRegionKind::Plain
                && segment.start_lba == 2_048
                && segment.sector_count == 4_096));
        assert!(model
            .segments
            .iter()
            .any(|segment| segment.kind == DiskRegionKind::Free));
    }
}
