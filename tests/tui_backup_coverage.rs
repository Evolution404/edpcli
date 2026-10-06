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
        display_cached: false,
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
        integrity_status:
            edpcli::infrastructure::backup_store::catalog::BackupIntegrityStatus::Verified,
        size_ok: true,
        verification_error: None,
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
        "备份元数据",
        "容量布局",
        "基本信息",
        "归属信息",
        "完整备份",
        "仅结构",
        "目录和用户文件未备份",
        "LBA",
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
    assert!(detail.contains("备份元数据"));
    state.focus_backups_pane(PaneId::BackupCoverage);
    let coverage = text(&state, 40, 10).replace(' ', "");
    assert!(coverage.contains("容量布局"));
    assert!(coverage.contains("LBA"));
    state.navigate(NavCommand::Escape, 10);
    assert_eq!(state.backups_focused_pane(), PaneId::BackupsList);
}

#[test]
fn capacity_region_navigation_updates_geometry_without_moving_backup() {
    let mut state = state();
    state.focus_backups_pane(PaneId::BackupCoverage);
    let path = state.selected_backup_path();
    let model = state
        .selected_backup()
        .unwrap()
        .restore_preview
        .as_ref()
        .unwrap()
        .layout
        .as_ref()
        .unwrap()
        .clone();
    let segments = model.collapsed_tail_model().segments;
    for segment in segments.iter().skip(1) {
        state.navigate(NavCommand::Down, 36);
        let selected = state.backup_capacity_selection().unwrap();
        assert_eq!(selected.start_lba, segment.start_lba);
        assert_eq!(selected.end_exclusive, segment.end_exclusive().unwrap());
        assert_eq!(selected.kind, segment.kind);
        assert_eq!(state.selected_backup_path(), path);
        let rendered = text(&state, 160, 45).replace(' ', "");
        assert!(
            rendered.contains(&segment.closed_range()),
            "selected LBA missing: {rendered}"
        );
        assert!(
            rendered.contains(&format!("当前区域{}", segment.label)),
            "selected label missing: {rendered}"
        );
    }
    state.navigate(NavCommand::Top, 36);
    assert_eq!(state.backup_capacity_selection().unwrap().start_lba, 0);
    state.navigate(NavCommand::Bottom, 36);
    assert_eq!(
        state.backup_capacity_selection().unwrap().start_lba,
        segments.last().unwrap().start_lba
    );
}

#[test]
fn changed_backup_content_resets_region_and_missing_layout_clears_map() {
    let mut state = state();
    state.focus_backups_pane(PaneId::BackupCoverage);
    state.navigate(NavCommand::Bottom, 36);
    assert_ne!(state.backup_capacity_selection().unwrap().start_lba, 0);
    let mut updated = state.selected_backup().unwrap().clone();
    updated.content_sha256 = Some("b".repeat(64));
    updated.index = 8;
    state.replace_backups(vec![updated.clone()]);
    assert_eq!(state.backup_capacity_selection().unwrap().start_lba, 0);
    assert!(text(&state, 160, 45).replace(' ', "").contains("备份#8"));
    updated.restore_preview.as_mut().unwrap().layout = None;
    updated.restore_preview.as_mut().unwrap().layout_error = Some("容量布局校验失败".into());
    state.replace_backups(vec![updated]);
    assert!(state.backup_capacity_selection().is_none());
    let rendered = text(&state, 40, 15).replace(' ', "");
    assert!(rendered.contains("容量布局校验失败"));
    assert!(!rendered.contains("当前区域"));
}

#[test]
fn metadata_scroll_reaches_full_hash_and_capacity_pane_stays_separate() {
    let mut state = state();
    state.focus_backups_pane(PaneId::BackupSummary);
    let path = state.selected_backup_path();
    state.navigate_in_viewport(NavCommand::Bottom, ratatui::layout::Size::new(80, 20));
    let bottom = text(&state, 80, 20).replace(' ', "");
    assert!(
        bottom.contains(&"a".repeat(64)),
        "full hash should remain reachable: {bottom}"
    );
    assert_eq!(state.selected_backup_path(), path);
    state.focus_backups_pane(PaneId::BackupCoverage);
    let capacity = text(&state, 40, 20).replace(' ', "");
    assert!(
        capacity.contains("LBA[0..12]"),
        "actual sector positions missing: {capacity}"
    );
    assert!(capacity.contains("扇区数"));
    assert!(!capacity.contains("目录和用户文件"));
    assert!(!capacity.contains("恢复能力"));
}

#[test]
fn device_tree_counts_keep_their_color_and_weight_when_focus_changes() {
    use ratatui::style::Modifier;
    let mut state = state();
    for focused in [PaneId::BackupsList, PaneId::BackupDevices] {
        state.focus_backups_pane(focused);
        state.backup_device_tree_move(1);
        let mut terminal = Terminal::new(TestBackend::new(160, 45)).unwrap();
        terminal.draw(|frame| render::draw(frame, &state)).unwrap();
        let buffer = terminal.backend().buffer();
        // Child rows have a neutral name; their count must retain semantic emphasis.
        let root = (0..45)
            .find(|y| {
                (0..39)
                    .map(|x| buffer[(x, *y)].symbol())
                    .collect::<String>()
                    .replace(' ', "")
                    .contains("全部备份")
            })
            .expect("device tree root");
        let row = root + 1;
        let count_x = (0..39)
            .find(|x| buffer[(*x, row)].symbol() == "[")
            .expect("device tree child count");
        let count = &buffer[(count_x + 1, row)];
        let name = &buffer[(5, row)];
        assert_ne!(count.fg, name.fg);
        assert!(count.modifier.contains(Modifier::BOLD));
        assert_eq!(
            count.bg, name.bg,
            "count should share the row selection background"
        );
    }
}

#[test]
fn damaged_backup_remains_visible_but_cannot_enter_restore_selection() {
    let mut state = state();
    let mut row = state.backups()[0].clone();
    row.integrity_status = edpcli::application::BackupIntegrityStatus::Invalid;
    row.verification_error = Some("artifact 摘要不匹配".into());
    state.replace_backups(vec![row]);
    assert!(state.selected_backup_path().is_some());
    assert!(state.selected_restore_backup_path().is_none());
    let output = text(&state, 160, 55).replace(' ', "");
    assert!(output.contains("校验失败"));
    assert!(output.contains("摘要不匹配"));
}

#[test]
fn long_unicode_filename_and_hash_remain_reachable_across_resize() {
    let mut state = state();
    let mut row = state.backups()[0].clone();
    row.file_name = format!("{}-end.edpb", "备份文件_".repeat(50));
    state.replace_backups(vec![row]);
    state.focus_backups_pane(PaneId::BackupSummary);
    for width in [40, 80, 160, 40] {
        let size = ratatui::layout::Size::new(width, 24);
        state.navigate_in_viewport(NavCommand::Bottom, size);
        let bottom = text(&state, width, size.height).replace(' ', "");
        assert!(
            bottom.contains("end.edpb"),
            "filename tail missing at {width}: {bottom}"
        );
        assert!(
            bottom.contains("SHA-256"),
            "hash missing at {width}: {bottom}"
        );
        let offset = state.pane_viewport(PaneId::BackupSummary).scroll_y.offset;
        assert!(offset > 0);
        state.navigate_in_viewport(NavCommand::Top, size);
        state.navigate_in_viewport(NavCommand::HalfPageDown, size);
        let half = state.pane_viewport(PaneId::BackupSummary).scroll_y.offset;
        state.navigate_in_viewport(NavCommand::Top, size);
        state.navigate_in_viewport(NavCommand::PageDown, size);
        let page = state.pane_viewport(PaneId::BackupSummary).scroll_y.offset;
        assert!(
            page >= half,
            "PageDown must scroll at least as far as HalfPageDown at {width}"
        );
        if page == half {
            state.navigate_in_viewport(NavCommand::Bottom, size);
            assert_eq!(
                state.pane_viewport(PaneId::BackupSummary).scroll_y.offset,
                page
            );
        }
    }
}
