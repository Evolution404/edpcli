use edpcli::application::{
    backup_coverage::BackupCoverage,
    backup_restore_preview::{
        BackupRestorePreview, BackupRestoreRegionKind, BackupRestoreRegionStatus,
    },
    disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind},
    BackupWorkspaceItem,
};
use edpcli::edpb::{
    Artifact, ArtifactCompleteness, ChunkStorage, Extent, Region, RestoreContract, RestorePolicy,
    SemanticStatus,
};
use edpcli::tui::{
    pane::PaneId,
    render,
    state::{AppState, NavCommand},
};
use ratatui::{backend::TestBackend, Terminal};

fn text(state: &AppState, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render::draw(frame, state)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

fn coverage() -> BackupCoverage {
    let regions = [Region {
        id: "protocol".into(),
        role: "protocol".into(),
        start_lba: Some(0),
        sector_count: Some(20),
        semantic_status: SemanticStatus::Identified,
    }];
    let extents = [
        Extent {
            id: "a".into(),
            region_id: "protocol".into(),
            start_lba: 0,
            sector_count: 8,
            purpose: "raw".into(),
        },
        Extent {
            id: "b".into(),
            region_id: "protocol".into(),
            start_lba: 4,
            sector_count: 8,
            purpose: "raw".into(),
        },
    ];
    let artifacts = [Artifact {
        id: "raw".into(),
        kind: "raw".into(),
        media_type: "application/octet-stream".into(),
        source_extent_ids: vec!["a".into(), "b".into()],
        derivation: None,
        restore_policy: RestorePolicy::Restorable,
        completeness: ArtifactCompleteness::Complete,
        storage: ChunkStorage {
            frame_offset: 0,
            data_offset: 0,
            stored_length: 0,
            original_length: 0,
            codec: "none".into(),
            sha256: String::new(),
        },
    }];
    BackupCoverage::from_parts(&regions, &extents, &artifacts)
}

fn restore_preview() -> BackupRestorePreview {
    let layout = DiskLayoutModel::canonical_edp(
        200_000,
        vec![
            DiskLayoutSegment {
                label: "启动区".into(),
                start_lba: 63,
                sector_count: 20_417,
                kind: DiskRegionKind::Boot,
            },
            DiskLayoutSegment {
                label: "交换区".into(),
                start_lba: 20_480,
                sector_count: 20_000,
                kind: DiskRegionKind::Share,
            },
            DiskLayoutSegment {
                label: "保密区".into(),
                start_lba: 40_480,
                sector_count: 20_000,
                kind: DiskRegionKind::Encrypt,
            },
        ],
        180_000,
        6,
    )
    .unwrap();
    BackupRestorePreview {
        total_sectors: Some(200_000),
        layout: Some(layout),
        layout_error: None,
        region_statuses: vec![
            BackupRestoreRegionStatus {
                label: "EDP 协议 LBA0-12".into(),
                kind: BackupRestoreRegionKind::CompleteBytes,
                detail: Some("13 sector".into()),
            },
            BackupRestoreRegionStatus {
                label: "启动区".into(),
                kind: BackupRestoreRegionKind::StructureOnly,
                detail: Some("数据内容未备份".into()),
            },
            BackupRestoreRegionStatus {
                label: "LCE".into(),
                kind: BackupRestoreRegionKind::CompleteBytes,
                detail: Some("6 sector".into()),
            },
            BackupRestoreRegionStatus {
                label: "用户文件".into(),
                kind: BackupRestoreRegionKind::OutOfScope,
                detail: Some("目录与用户文件不在备份范围".into()),
            },
        ],
        restore_contract: Some(RestoreContract::metadata_only(true)),
        is_plain: false,
    }
}

fn state() -> AppState {
    let mut state = AppState::new();
    state.replace_backups(vec![BackupWorkspaceItem {
        index: 1,
        path: "fixture.edpb".into(),
        file_name: "fixture.edpb".into(),
        display_time: "2026-09-27 10:00".into(),
        size_bytes: Some(10_240),
        vid: Some("1234".into()),
        pid: Some("5678".into()),
        device_id: Some("disk&ven_demo&prod_usb".into()),
        onlyid: Some("7001".into()),
        identity: None,
        user: Some("张三".into()),
        dept: Some("输电运检中心".into()),
        provision_kind: Some(edpcli::provision::DiskProvisionKind::Mode0),
        integrity_status: edpcli::diskio::BackupIntegrityStatus::Verified,
        size_ok: true,
        content_sha256: Some("a".repeat(64)),
        coverage: Some(coverage()),
        restore_preview: Some(restore_preview()),
    }]);
    state.navigate(NavCommand::WorkspaceBackups, 20);
    state
}

#[test]
fn manifest_coverage_unions_overlapping_extents_and_marks_partial_region() {
    let coverage = coverage();
    assert_eq!(coverage.regions[0].captured_sectors, 12);
    assert_eq!(coverage.regions[0].artifact_count, 1);
    assert_eq!(
        coverage.regions[0].completeness,
        ArtifactCompleteness::Partial
    );
}

#[test]
fn backup_workspace_shows_detail_coverage_and_no_animation_sidebar() {
    let state = state();
    let wide = text(&state, 160, 45).replace(' ', "");
    for value in [
        "设备",
        "全部备份",
        "备份列表",
        "备份信息",
        "恢复范围",
        "容量地图",
        "完整恢复",
        "结构恢复",
        "不在备份范围",
        "身份未验证",
    ] {
        assert!(wide.contains(value), "missing {value}: {wide}");
    }
    for stale in [
        "区域覆盖",
        "12/20sector",
        "Extent",
        "Artifact",
        "EDPCORE·LIVE",
    ] {
        assert!(!wide.contains(stale), "stale {stale}: {wide}");
    }
}

#[test]
fn compact_backup_detail_and_coverage_remain_reachable_and_escape_returns() {
    let mut state = state();
    let list = text(&state, 40, 10).replace(' ', "");
    assert!(list.contains("序号"), "{list}");
    assert!(list.contains("时间"), "{list}");

    use edpcli::tui::table_layout::TableKind;
    assert!(state.move_table_column_edge_for_viewport(TableKind::Backups, true, 40, 10,));
    let last_columns = text(&state, 40, 10).replace(' ', "");
    assert!(
        last_columns.contains("名称"),
        "last backup column should be 名称 after $: {last_columns}"
    );

    state.focus_backups_pane(PaneId::BackupSummary);
    let detail = text(&state, 40, 10).replace(' ', "");
    assert!(detail.contains("健康"));
    assert!(detail.contains("目录和用户文件"));
    state.focus_backups_pane(PaneId::BackupCoverage);
    let coverage = text(&state, 40, 10).replace(' ', "");
    assert!(coverage.contains("恢复范围"));
    assert!(coverage.contains("容量地图"));
    state.navigate(NavCommand::Escape, 10);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupsList);
}
