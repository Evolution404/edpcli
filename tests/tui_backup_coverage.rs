use edpcli::application::{backup_coverage::BackupCoverage, BackupWorkspaceItem};
use edpcli::edpb::{
    Artifact, ArtifactCompleteness, ChunkStorage, Extent, Region, RestorePolicy, SemanticStatus,
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
        is_nopwd: false,
        provision_kind: edpcli::provision::DiskProvisionKind::Mode0,
        integrity_status: edpcli::diskio::BackupIntegrityStatus::Verified,
        size_ok: true,
        content_sha256: Some("a".repeat(64)),
        coverage: Some(coverage()),
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
        "备份列表",
        "备份详情",
        "区域覆盖",
        "EDP主协议区",
        "12/20sector",
        "普通用户文件不保证完整备份",
        "身份未验证",
    ] {
        assert!(wide.contains(value), "missing {value}");
    }
    assert!(!wide.contains("EDPCORE·LIVE"));
}

#[test]
fn compact_backup_detail_and_coverage_remain_reachable_and_escape_returns() {
    let mut state = state();
    let list = text(&state, 40, 10).replace(' ', "");
    assert!(list.contains("名称"));
    assert!(list.contains("健康"));
    state.focus_backups_pane(PaneId::BackupSummary);
    let detail = text(&state, 40, 10).replace(' ', "");
    assert!(detail.contains("健康"));
    assert!(detail.contains("普通用户文件"));
    state.focus_backups_pane(PaneId::BackupCoverage);
    let coverage = text(&state, 40, 10).replace(' ', "");
    assert!(coverage.contains("区域覆盖"));
    state.navigate(NavCommand::Escape, 10);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupsList);
}
