use crate::common;

use std::path::PathBuf;

use edpcli::application::inspect::load_backup_inspect;
use edpcli::common::{METADATA_IMAGE_LEN, METADATA_SECTOR_COUNT, SECTOR};
use edpcli::edpb::{self, CoreCapture};
use edpcli::tui::{
    pane::PaneId,
    render,
    state::{AdvancedInspectSource, AppState},
};
use ratatui::{backend::TestBackend, Terminal};

fn netac_edpb(tag: &str) -> Option<(common::TmpDir, PathBuf)> {
    let data = common::load_disk_image("netac")?;
    let tmp = common::TmpDir::new(tag);
    let path = tmp.0.join("netac.edpb");
    edpb::write_core_backup(
        &path,
        &CoreCapture {
            snapshot_id: format!("tui-inspect-{tag}"),
            created_epoch: 1_789_000_000,
            disk_number: Some(6),
            vid: "0dd8".into(),
            pid: "2005".into(),
            device_id: "disk&ven_netac&prod_onlydisk".into(),
            onlyid: Some("1402259934".into()),
            total_sectors: Some(122_880_000),
            logical_sector_size: 512,
            edpcli_version: env!("CARGO_PKG_VERSION").into(),
            device_state: "encrypted".into(),
            lba0_12: &data,
        },
    )
    .unwrap();
    Some((tmp, path))
}

fn plain_sparse_edpb(tag: &str) -> (common::TmpDir, PathBuf) {
    let tmp = common::TmpDir::new(tag);
    let path = tmp.0.join("plain.edpb");
    let total_sectors = 15_728_640u64;
    let protocol_placeholder = vec![0u8; METADATA_IMAGE_LEN];
    let region_id = "region.partition_table.mbr".to_string();
    let extent_id = "extent.partition_table.mbr".to_string();
    let mut mbr = vec![0u8; SECTOR];
    let entry = 0x1be;
    mbr[entry + 4] = 0x07;
    mbr[entry + 8..entry + 12].copy_from_slice(&2_048u32.to_le_bytes());
    mbr[entry + 12..entry + 16]
        .copy_from_slice(&u32::try_from(total_sectors - 2_048).unwrap().to_le_bytes());
    mbr[510..512].copy_from_slice(&[0x55, 0xaa]);

    let capture = edpb::MetadataCapture {
        core: CoreCapture {
            snapshot_id: format!("plain-inspect-{tag}"),
            created_epoch: 1_789_000_100,
            disk_number: Some(5),
            vid: "3535".into(),
            pid: "6300".into(),
            device_id: "disk&ven_aigo&prod_u335".into(),
            onlyid: None,
            total_sectors: Some(total_sectors),
            logical_sector_size: SECTOR as u32,
            edpcli_version: env!("CARGO_PKG_VERSION").into(),
            device_state: "plain".into(),
            lba0_12: &protocol_placeholder,
        },
        partitions: vec![edpb::ManifestPartition {
            index: 1,
            role: Some("mbr_primary".into()),
            partition_type: Some("mbr:0x07".into()),
            start_lba: 2_048,
            sector_count: total_sectors - 2_048,
            filesystem_hint: Some("exfat".into()),
            volume_label_hint: None,
        }],
        regions: vec![edpb::Region {
            id: region_id.clone(),
            role: "partition_table".into(),
            start_lba: Some(0),
            sector_count: Some(1),
            semantic_status: edpb::SemanticStatus::Identified,
        }],
        extents: vec![edpb::Extent {
            id: extent_id.clone(),
            region_id,
            start_lba: 0,
            sector_count: 1,
            purpose: "mbr_partition_table".into(),
        }],
        artifacts: vec![edpb::ArtifactInput {
            id: "raw.partition_table.mbr".into(),
            kind: "raw_sectors".into(),
            media_type: "application/octet-stream".into(),
            source_extent_ids: vec![extent_id],
            derivation: None,
            restore_policy: edpb::RestorePolicy::Restorable,
            completeness: edpb::ArtifactCompleteness::Complete,
            data: mbr,
        }],
        notes: Vec::new(),
    };
    edpb::write_metadata_backup(&path, &capture).unwrap();
    (tmp, path)
}

#[test]
fn plain_metadata_only_backup_inspect_is_sparse_and_fail_soft() {
    let (_tmp, path) = plain_sparse_edpb("plain_sparse_inspect");
    let workspace = load_backup_inspect(&path).expect("sparse Plain backup Inspect");
    assert_eq!(
        workspace
            .items
            .iter()
            .map(|item| item.lba)
            .collect::<Vec<_>>(),
        vec![0],
        "default browser sweep must decode only physically captured protocol-prefix sectors"
    );
    assert!(workspace.backup_manifest.is_some());
    assert!(
        workspace.disk_layout.is_some(),
        "Manifest + captured partition-table metadata should still build the whole-disk layout"
    );
}

#[test]
fn backup_inspect_reuses_domain_analyzer_for_all_metadata_lbas() {
    let Some((_tmp, path)) = netac_edpb("inspect_workspace") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let workspace = load_backup_inspect(&path).expect("inspect backup");
    assert_eq!(workspace.items.len(), METADATA_SECTOR_COUNT);
    let manifest = workspace
        .backup_manifest
        .as_ref()
        .expect("backup Inspect must retain verified EDPB manifest");
    assert!(!manifest.schema.is_empty());
    assert!(!manifest.regions.is_empty());
    assert!(!manifest.extents.is_empty());
    assert!(!manifest.artifacts.is_empty());
    for (lba, item) in workspace.items.iter().enumerate() {
        assert_eq!(item.lba, lba as u64);
        assert_eq!(item.raw.len(), 512);
        assert_eq!(item.decoded.as_ref().map(Vec::len), Some(512));
        assert!(item.method.is_some());
    }
}

#[test]
fn backup_inspect_renders_manifest_technical_evidence_outside_backup_main_page() {
    let Some((_tmp, path)) = netac_edpb("inspect_manifest_render") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let workspace = load_backup_inspect(&path).expect("inspect backup");
    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Backup(path)));
    state.advanced_inspect_finish(Ok(workspace));

    let mut terminal = Terminal::new(TestBackend::new(160, 45)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let initial = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .replace(' ', "");
    assert!(initial.contains("备份Manifest"), "{initial}");
    assert!(
        initial.contains("Region/Extent/Arti"),
        "technical evidence should expose the Region/Extent/Artifact summary even when the value column is clipped: {initial}"
    );
    assert!(initial.contains("Manifest.R"), "{initial}");
    assert!(initial.contains("Manifest.E"), "{initial}");
    assert!(initial.contains("Manifest.A"), "{initial}");

    state.advanced_inspect_focus_pane(PaneId::InspectDetail);
    state.advanced_inspect_focused_bottom();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let bottom = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .replace(' ', "");
    assert!(bottom.contains("Manifest.A"), "{bottom}");
    assert!(bottom.contains("SHA-256"), "{bottom}");
}

#[test]
fn backup_inspect_rejects_legacy_bin_images() {
    let tmp = common::TmpDir::new("inspect_reject_7168");
    let path = tmp.0.join("legacy-lba0-13.bin");
    std::fs::write(&path, vec![0u8; METADATA_IMAGE_LEN + SECTOR]).unwrap();

    let err = load_backup_inspect(&path).expect_err("legacy .bin must be rejected");
    assert!(err.message().contains(".edpb"), "{err}");
}
