use edpcli::tui::table_layout::{
    display_width, identity_column_specs, table_column_schema, truncate_cell, AdaptiveColumnSpec,
    AdaptiveTableLayout, ColumnId, SortDirection, TableInteractionState, TableKind, TruncatePolicy,
};

#[test]
fn d0_device_schema_is_task_specific_while_backup_identity_schema_stays_shared() {
    let shared = identity_column_specs();
    let devices = table_column_schema(TableKind::Devices).unwrap();
    let backups = table_column_schema(TableKind::Backups).unwrap();

    assert_eq!(
        devices
            .iter()
            .map(|column| column.heading)
            .collect::<Vec<_>>(),
        vec!["设备", "容量", "部门", "姓名", "盘型", "状态", "备份", "型号"]
    );
    assert_eq!(backups.len(), 12);
    assert_eq!(backups[4..11], shared);
    assert_eq!(backups[10].id, ColumnId::ProvisionKind);
    assert_eq!(backups[2].id, ColumnId::Name);
}

fn spec(min: u16, preferred: u16, max: u16, priority: u8, pinned: bool) -> AdaptiveColumnSpec {
    AdaptiveColumnSpec {
        min_width: min,
        preferred_width: preferred,
        max_width: max,
        priority,
        weight: 1,
        truncate_policy: TruncatePolicy::Ellipsis,
        pinned,
    }
}

#[test]
fn cjk_and_emoji_use_terminal_display_width() {
    assert_eq!(display_width("盘型A"), 5);
    assert_eq!(display_width("👩‍💻"), 2);
    let clipped = truncate_cell("输电运检中心", 5, TruncatePolicy::Ellipsis);
    assert!(display_width(&clipped) <= 5);
    assert!(clipped.ends_with('…'));
}

#[test]
fn narrow_table_scrolls_all_columns_and_last_column_is_reachable() {
    let specs = vec![
        spec(7, 9, 12, 100, true),
        spec(8, 10, 12, 70, false),
        spec(10, 18, 40, 10, false),
        spec(10, 17, 24, 95, true),
    ];
    let layout = AdaptiveTableLayout::new(specs);
    let content = [8, 10, 38, 20];

    let initial = layout.layout_with_active(42, &content, 0, Some(0));
    assert!(initial.columns.iter().any(|column| column.index == 0));
    assert!(
        !initial.columns.iter().any(|column| column.index == 3),
        "no column is pinned outside the cell viewport"
    );

    let mut state = TableInteractionState::default();
    assert!(state.move_active_edge(&layout, &content, 42, true));
    let last = layout.layout_with_active(
        42,
        &content,
        state.viewport_offset(),
        Some(state.active_column()),
    );
    assert!(last.columns.iter().any(|column| column.index == 3));
    let active = last
        .columns
        .iter()
        .find(|column| column.index == 3)
        .unwrap();
    assert_eq!(active.clip_left, 0);
    assert_eq!(usize::from(active.width), 20);
}

#[test]
fn smooth_horizontal_scroll_clips_by_two_terminal_cells_not_whole_columns() {
    let layout = AdaptiveTableLayout::new(vec![
        spec(8, 12, 12, 50, false),
        spec(8, 12, 12, 50, false),
        spec(8, 12, 12, 50, false),
    ]);
    let content = [12, 12, 12];
    let mut state = TableInteractionState::default();

    let before = layout.layout_with_active(20, &content, 0, Some(0));
    let first_before = before.columns.first().unwrap();
    assert_eq!(first_before.index, 0);
    assert_eq!(first_before.clip_left, 0);

    assert!(state.scroll_viewport(&layout, &content, 20, false));
    assert_eq!(state.viewport_offset(), 2);

    let after = layout.layout_with_active(20, &content, state.viewport_offset(), Some(0));
    let first_after = after.columns.first().unwrap();
    assert_eq!(
        first_after.index, 0,
        "same column remains partially visible"
    );
    assert_eq!(first_after.clip_left, 2, "viewport moved exactly two cells");
    assert_eq!(
        usize::from(first_before.width) - usize::from(first_after.width),
        2
    );
}

#[test]
fn unified_table_state_separates_column_focus_cell_viewport_and_sort() {
    let layout = AdaptiveTableLayout::new(
        (0..9)
            .map(|index| spec(8, 10, 14, 50, index == 0))
            .collect(),
    );
    let content = [10; 9];
    let mut state = TableInteractionState::default();

    assert_eq!(state.active_column(), 0);
    assert_eq!(state.viewport_offset(), 0);

    assert!(state.move_active(&layout, &content, 35, false));
    assert_eq!(state.active_column(), 1);
    assert_eq!(
        state.viewport_offset(),
        0,
        "visible h/l must not move viewport"
    );

    assert!(state.move_active(&layout, &content, 35, false));
    assert_eq!(state.active_column(), 2);
    assert_eq!(
        state.viewport_offset(),
        0,
        "still visible: viewport remains fixed"
    );

    assert!(state.move_active(&layout, &content, 35, false));
    assert_eq!(state.active_column(), 3);
    assert_eq!(
        state.viewport_offset(),
        8,
        "first off-screen column should minimally follow and touch the right edge"
    );

    let before = state.active_column();
    assert!(state.scroll_viewport(&layout, &content, 35, false));
    assert_eq!(state.active_column(), before);
    assert_eq!(
        state.viewport_offset(),
        10,
        "H/L moves exactly two terminal cells"
    );
    assert!(state.scroll_viewport(&layout, &content, 35, true));
    assert_eq!(state.viewport_offset(), 8);

    assert!(state.move_active_edge(&layout, &content, 35, true));
    assert_eq!(state.active_column(), 8);
    assert_eq!(
        state.viewport_offset(),
        layout.max_scroll(&content, Some(8), 35),
        "$ places the last column at the right edge"
    );
    assert!(state.move_active_edge(&layout, &content, 35, false));
    assert_eq!(state.active_column(), 0);
    assert_eq!(
        state.viewport_offset(),
        0,
        "0 returns to the first column and left edge"
    );

    state.toggle_sort();
    assert_eq!(state.sort().unwrap().column, 0);
    assert_eq!(state.sort().unwrap().direction, SortDirection::Ascending);
    state.toggle_sort();
    assert_eq!(state.sort().unwrap().direction, SortDirection::Descending);
    assert!(state.clear_sort());
    assert_eq!(state.sort(), None);
}

#[test]
fn active_column_is_kept_visible_and_expands_without_ellipsis() {
    let layout = AdaptiveTableLayout::new(vec![
        spec(7, 9, 12, 100, true),
        spec(8, 10, 12, 90, true),
        spec(8, 16, 32, 80, true),
        spec(6, 10, 18, 70, true),
        spec(12, 22, 30, 95, true),
    ]);
    let content = [8, 10, 36, 10, 20];
    let viewport = layout.layout_with_active(72, &content, 0, Some(2));
    let dept = viewport
        .columns
        .iter()
        .find(|column| column.index == 2)
        .expect("active department column visible");
    assert_eq!(usize::from(dept.width), 36);
    assert_eq!(dept.truncate_policy, TruncatePolicy::Clip);
    assert!(
        viewport
            .columns
            .iter()
            .map(|c| usize::from(c.width))
            .sum::<usize>()
            <= 72
    );
}

#[test]
fn every_interactive_table_renderer_uses_unified_active_column_layout() {
    for (name, source, minimum) in [
        (
            "devices",
            include_str!("../src/tui/devices/render.rs"),
            1usize,
        ),
        (
            "backups",
            include_str!("../src/tui/backups/render.rs"),
            1usize,
        ),
        (
            "provision",
            include_str!("../src/tui/provision/render.rs"),
            2usize,
        ),
        (
            "inspect",
            include_str!("../src/tui/inspect/render.rs"),
            1usize,
        ),
    ] {
        let active_layouts = source.matches("layout_with_active(").count();
        assert!(
            active_layouts >= minimum,
            "{name} must route every interactive table through layout_with_active; got {active_layouts}"
        );
        assert!(
            source.contains("table_interaction("),
            "{name} must consume the shared TableInteractionState"
        );
    }
}
