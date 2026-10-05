use super::*;
use ratatui::{backend::TestBackend, Terminal};

fn capacity_map_fixture() -> DiskLayoutModel {
    DiskLayoutModel::new(
        1_000,
        vec![
            DiskLayoutSegment {
                label: "启动区".into(),
                start_lba: 0,
                sector_count: 100,
                kind: DiskRegionKind::Boot,
            },
            DiskLayoutSegment {
                label: "交换区".into(),
                start_lba: 100,
                sector_count: 700,
                kind: DiskRegionKind::Share,
            },
            DiskLayoutSegment {
                label: "保密区".into(),
                start_lba: 800,
                sector_count: 200,
                kind: DiskRegionKind::Encrypt,
            },
        ],
    )
}

#[test]
fn capacity_map_keeps_small_blocks_and_selection_inside_each_block() {
    let mut model = capacity_map_fixture();
    let sizes = [13, 50, 131072, 131072, 131072, 245351234, 15487];
    let template = model.segments[0].clone();
    let mut start = 0;
    model.segments = sizes
        .into_iter()
        .map(|size| {
            let mut segment = template.clone();
            segment.start_lba = start;
            segment.sector_count = size;
            start += size;
            segment
        })
        .collect();
    model.total_sectors = start;
    for width in [7, 98, 158, 238] {
        let cells = capacity_map_allocations(&model, width);
        assert_eq!(cells.iter().sum::<usize>(), width);
        let mut offset = 0;
        for (segment, count) in model.segments.iter().zip(&cells) {
            assert!(*count >= if width >= sizes.len() * 3 { 3 } else { 1 });
            let selection = DiskCapacitySelection::from_segment(segment).unwrap();
            let marker = capacity_map_marker_column(&model, &cells, &selection);
            assert!((offset..offset + count).contains(&marker));
            offset += count;
        }
    }
    assert_eq!(capacity_map_allocations(&model, 98)[5], 80);
}

#[test]
fn tiny_regions_keep_visible_color_blocks_in_every_map_profile() {
    // Protocol/header/footer-like regions adjacent to one dominant partition.
    let sizes = [13, 50, 131072, 131072, 131072, 245351234, 15487];
    let mut start = 0;
    let segments = sizes
        .into_iter()
        .enumerate()
        .map(|(index, size)| {
            let segment = DiskLayoutSegment {
                label: format!("区域{index}"),
                start_lba: start,
                sector_count: size,
                kind: DiskRegionKind::Boot,
            };
            start += size;
            segment
        })
        .collect();
    let model = DiskLayoutModel::new(start, segments);
    for width in [98, 158, 238] {
        for (profile, diagram_line) in [
            (DiskCapacityMapProfile::Full, 3),
            (DiskCapacityMapProfile::Compact, 1),
            (DiskCapacityMapProfile::Mini, 0),
        ] {
            let lines = DiskCapacityMap::new(&model, profile)
                .with_tail(TailExpansion::Expanded)
                .with_marker(false)
                .lines(width);
            let blocks = &lines[diagram_line].spans;
            assert_eq!(blocks.len(), sizes.len());
            assert_eq!(lines[diagram_line].width(), width);
            assert!(blocks
                .iter()
                .all(|span| span.width() >= 3 && span.style.bg.is_some()));
            if profile == DiskCapacityMapProfile::Full {
                assert_eq!(lines.len(), 6);
                assert!(lines[1].to_string().contains('┈'));
                assert!(lines
                    .iter()
                    .all(|line| !line.to_string().contains("区域示意")));
            }
        }
        let mut previous_marker = None;
        for segment in &model.segments {
            let lines = DiskCapacityMap::new(&model, DiskCapacityMapProfile::Full)
                .with_tail(TailExpansion::Expanded)
                .with_selection(DiskCapacitySelection::from_segment(segment))
                .lines(width);
            let marker = lines.last().unwrap().to_string().find('▲').unwrap();
            assert_eq!(lines.len(), 7);
            assert!(previous_marker.is_none_or(|previous| marker > previous));
            let active = &lines[3].spans[model.segments.iter().position(|s| s == segment).unwrap()];
            assert_eq!(
                active.style.bg,
                super::super::theme::current()
                    .disk_region_fill(segment.kind, true)
                    .bg
            );
            previous_marker = Some(marker);
        }
    }
}

#[test]
fn capacity_map_profiles_share_one_renderer_with_page_specific_density() {
    let model = capacity_map_fixture();
    let full = DiskCapacityMap::new(&model, DiskCapacityMapProfile::Full)
        .with_marker(false)
        .lines(80);
    assert_eq!(full.len(), 6);
    assert!(full[0].to_string().contains("0%"));
    assert!(full[0].to_string().contains("100%"));
    assert!(full[1].to_string().contains('┈'));
    assert!(full[2].to_string().contains('▄'));
    assert!(full[5].to_string().contains('▀'));

    let compact = DiskCapacityMap::new(&model, DiskCapacityMapProfile::Compact).lines(80);
    assert_eq!(compact.len(), 3);
    assert!(compact[0].to_string().contains('▄'));
    assert!(compact[2].to_string().contains('▀'));
    assert!(!compact.iter().any(|line| line.to_string().contains('%')));

    let mini = DiskCapacityMap::new(&model, DiskCapacityMapProfile::Mini).lines(80);
    assert_eq!(mini.len(), 1);
    let mini_text = mini[0].to_string();
    assert!(!mini_text.contains('%'));
    assert!(!mini_text.contains('▄'));
    assert!(!mini_text.contains('▀'));
    assert!(!mini_text.contains('▲'));
}

#[test]
fn mini_capacity_map_highlights_the_region_containing_current_lba() {
    let model = capacity_map_fixture();
    let selection = DiskCapacitySelection::from_lba(&model, 850).expect("encrypt selection");
    let line = DiskCapacityMap::new(&model, DiskCapacityMapProfile::Mini)
        .with_selection(Some(selection))
        .lines(80)
        .pop()
        .expect("mini line");
    let active_encrypt = super::super::theme::current()
        .disk_region_fill(DiskRegionKind::Encrypt, true)
        .bg;
    let normal_share = super::super::theme::current()
        .disk_region_fill(DiskRegionKind::Share, false)
        .bg;
    assert!(line
        .spans
        .iter()
        .any(|span| span.style.bg == active_encrypt));
    assert!(line.spans.iter().any(|span| span.style.bg == normal_share));
}

#[test]
fn partition_status_stays_visible_at_60_80_100_and_120_columns() {
    let model = DiskLayoutModel::new(
        100_000,
        vec![DiskLayoutSegment {
            label: "保密区".into(),
            start_lba: 0,
            sector_count: 100_000,
            kind: DiskRegionKind::Encrypt,
        }],
    );
    let details = [DiskLayoutDetail::region_columns(
        DiskRegionKind::Encrypt,
        true,
        "保密区",
        "48.8 MiB",
        "LBA 0–99999",
        "⚠ 需重建",
        DiskLayoutDetailTone::Warning,
    )];
    for width in [60u16, 80, 100, 120] {
        let mut terminal = Terminal::new(TestBackend::new(width, 16)).unwrap();
        terminal
            .draw(|frame| {
                model.render_pane(
                    frame,
                    frame.area(),
                    DiskLayoutPane {
                        title: "布局",
                        summary: "disk6",
                        details: &details,
                        focused: true,
                        scroll_y: 0,
                        profile: DiskLayoutProfile::DetailedExact,
                        tail: TailExpansion::Collapsed,
                        selected_segment: 0,
                        map_selection: None,
                        show_map_marker: false,
                        show_linked_selection: false,
                    },
                );
            })
            .unwrap();
        let screen = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        let compact = screen
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>();
        assert!(
            compact.contains("⚠需重建"),
            "width={width} screen={screen:?}"
        );
    }
}

#[test]
fn linked_region_selection_uses_full_row_background_without_brightening_name() {
    let model = DiskLayoutModel::new(
        100_000,
        vec![DiskLayoutSegment {
            label: "保密区".into(),
            start_lba: 0,
            sector_count: 100_000,
            kind: DiskRegionKind::Encrypt,
        }],
    );
    let details = [DiskLayoutDetail::region_columns(
        DiskRegionKind::Encrypt,
        true,
        "保密区",
        "48.8 MiB",
        "LBA 0–99999",
        "⚠ 需重建",
        DiskLayoutDetailTone::Warning,
    )];
    let width = 100u16;
    let mut terminal = Terminal::new(TestBackend::new(width, 16)).unwrap();
    terminal
        .draw(|frame| {
            model.render_pane(
                frame,
                frame.area(),
                DiskLayoutPane {
                    title: "布局",
                    summary: "",
                    details: &details,
                    focused: false,
                    scroll_y: 0,
                    profile: DiskLayoutProfile::DetailedExact,
                    tail: TailExpansion::Collapsed,
                    selected_segment: 0,
                    map_selection: None,
                    show_map_marker: false,
                    show_linked_selection: true,
                },
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let selected_y = (0..16u16)
        .find(|&y| {
            let text = (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>();
            text.chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>()
                .contains("⚠需重建")
        })
        .expect("selected region row");
    let theme = super::super::theme::current();
    let selected_bg = theme
        .selection_overlay(false)
        .bg
        .expect("selection overlay background");
    assert_eq!(buffer[(1, selected_y)].bg, selected_bg);
    assert_eq!(
        buffer[(width - 2, selected_y)].bg,
        selected_bg,
        "selection background must fill the complete inner row"
    );
    assert_eq!(
        buffer[(2, selected_y)].fg,
        theme
            .disk_region_tree(DiskRegionKind::Encrypt, false)
            .fg
            .expect("base region foreground"),
        "selection must keep the normal region foreground instead of brightening it"
    );
}

#[test]
fn tiny_lce_keeps_exact_legend_and_visible_bar_cell() {
    let model = DiskLayoutModel::new(
        1_000_000,
        vec![
            DiskLayoutSegment {
                label: "未知区域".into(),
                start_lba: 0,
                sector_count: 999_994,
                kind: DiskRegionKind::Unknown,
            },
            DiskLayoutSegment {
                label: "LCE".into(),
                start_lba: 999_994,
                sector_count: 6,
                kind: DiskRegionKind::Lce,
            },
        ],
    );
    assert_eq!(model.bar(40).len(), 40);
    assert!(model.bar(40).contains(&DiskRegionKind::Lce));
    assert!(model.legend_lines()[1].contains("[999994..999999]"));
    assert!(model.legend_lines()[1].contains("6 sectors"));
    assert!(model.legend_lines()[1].contains("<0.01%"));
    assert!(!model.legend_lines()[1].contains(" · "));
    assert!(model.legend_lines()[0].starts_with("未知区域"));
}

#[test]
fn complete_contract_rejects_holes_overlap_and_wrong_last_sector() {
    let hole = DiskLayoutModel::new(
        10,
        vec![
            DiskLayoutSegment {
                label: "a".into(),
                start_lba: 0,
                sector_count: 4,
                kind: DiskRegionKind::Protocol,
            },
            DiskLayoutSegment {
                label: "b".into(),
                start_lba: 5,
                sector_count: 5,
                kind: DiskRegionKind::Unknown,
            },
        ],
    );
    assert!(hole.validate_complete().unwrap_err().contains("hole"));

    let overlap = DiskLayoutModel::new(
        10,
        vec![
            DiskLayoutSegment {
                label: "a".into(),
                start_lba: 0,
                sector_count: 6,
                kind: DiskRegionKind::Protocol,
            },
            DiskLayoutSegment {
                label: "b".into(),
                start_lba: 5,
                sector_count: 5,
                kind: DiskRegionKind::Unknown,
            },
        ],
    );
    assert!(overlap.validate_complete().unwrap_err().contains("overlap"));

    let short = DiskLayoutModel::new(
        10,
        vec![DiskLayoutSegment {
            label: "a".into(),
            start_lba: 0,
            sector_count: 9,
            kind: DiskRegionKind::Unknown,
        }],
    );
    assert!(short.validate_complete().unwrap_err().contains("ends at"));
}

#[test]
fn canonical_layout_bar_preserves_lce_and_complete_coverage() {
    let model = DiskLayoutModel::canonical_edp(
        10_000,
        vec![
            DiskLayoutSegment {
                label: "启动区".into(),
                start_lba: 63,
                sector_count: 37,
                kind: DiskRegionKind::Boot,
            },
            DiskLayoutSegment {
                label: "交换区".into(),
                start_lba: 100,
                sector_count: 4_900,
                kind: DiskRegionKind::Share,
            },
            DiskLayoutSegment {
                label: "保密区".into(),
                start_lba: 5_000,
                sector_count: 1_000,
                kind: DiskRegionKind::Encrypt,
            },
        ],
        6_000,
        6,
    )
    .unwrap();
    assert_eq!(model.total_sectors, 10_000);
    assert_eq!(
        model
            .segments
            .iter()
            .find(|segment| segment.kind == DiskRegionKind::Lce)
            .unwrap()
            .sector_count,
        6
    );
    assert_eq!(model.bar(64).len(), 64);
    assert_eq!(
        model
            .segments
            .iter()
            .map(|segment| segment.sector_count)
            .sum::<u64>(),
        10_000
    );
    assert_eq!(
        model
            .collapsed_tail_model()
            .segments
            .last()
            .map(|segment| segment.kind),
        Some(DiskRegionKind::Tail)
    );
}
