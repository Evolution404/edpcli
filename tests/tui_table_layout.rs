use edpcli::tui::table_layout::{
    display_width, identity_column_specs, table_column_schema, truncate_cell, AdaptiveColumnSpec,
    AdaptiveTableLayout, ColumnId, HorizontalScrollState, TableKind, TruncatePolicy,
};

#[test]
fn ch14_identity_column_schema_is_identical_between_workspaces() {
    let shared = identity_column_specs();
    let devices = table_column_schema(TableKind::Devices).unwrap();
    let backups = table_column_schema(TableKind::Backups).unwrap();
    assert_eq!(devices.len(), 10);
    assert_eq!(backups.len(), 11);
    assert_eq!(devices[1..8], shared);
    assert_eq!(backups[3..10], shared);
    assert_eq!(devices[7].id, ColumnId::ProvisionKind);
    assert_eq!(backups[9].id, ColumnId::ProvisionKind);
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
fn high_priority_disk_kind_survives_long_department_and_narrow_width() {
    let specs = vec![
        spec(7, 9, 12, 100, true),
        spec(8, 10, 12, 70, false),
        spec(10, 18, 40, 10, false),
        spec(10, 17, 24, 95, true),
    ];
    let layout = AdaptiveTableLayout::new(specs);
    let viewport = layout.layout(42, &[8, 10, 38, 20], 0);
    assert!(viewport.columns.iter().any(|column| column.index == 0));
    assert!(viewport.columns.iter().any(|column| column.index == 3));
    assert!(
        viewport
            .columns
            .iter()
            .map(|column| usize::from(column.width))
            .sum::<usize>()
            <= 42
    );
    assert!(viewport
        .columns
        .iter()
        .all(|column| column.width >= layout.specs()[column.index].min_width));
}

#[test]
fn horizontal_scroll_steps_by_column_and_clamps_at_edges() {
    let layout = AdaptiveTableLayout::new(
        (0..9)
            .map(|index| spec(8, 10, 14, 50, index == 0))
            .collect(),
    );
    let mut scroll = HorizontalScrollState::default();
    let first = layout.layout(35, &[10; 9], scroll.offset());
    assert_eq!(first.columns[0].index, 0);
    assert!(scroll.right(&layout));
    let second = layout.layout(35, &[10; 9], scroll.offset());
    assert_ne!(first.columns[1].index, second.columns[1].index);
    for _ in 0..20 {
        scroll.right(&layout);
    }
    assert!(!scroll.right(&layout));
    assert_eq!(scroll.offset(), 7);
    assert!(scroll.left());
    for _ in 0..20 {
        scroll.left();
    }
    assert_eq!(scroll.offset(), 0);
    assert!(!scroll.left());
}
