use crate::common::fmt_sector_percentage as percentage;
use ratatui::{
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::disk_layout::{DiskLayoutModel, DiskLayoutSegment};

use super::state::{segment_matches_selection, DiskRegionListMode, DiskRegionListState};

fn region_header() -> String {
    format!(
        "{}  {}  {}  {}  {}  {}",
        crate::ui::pad_to("区域", 16),
        crate::ui::pad_to("类型", 14),
        crate::ui::pad_to("LBA 范围", 23),
        crate::ui::pad_to("扇区数", 13),
        crate::ui::pad_to("容量", 13),
        "占比"
    )
}

pub(super) fn region_row(segment: &DiskLayoutSegment, model: &DiskLayoutModel) -> String {
    format!(
        "{}  {}  {}  {}  {}  {}",
        crate::ui::pad_to(&segment.label, 16),
        crate::ui::pad_to(segment.kind.label(), 14),
        crate::ui::pad_to(&segment.closed_range(), 23),
        crate::ui::pad_to(&segment.sector_count.to_string(), 13),
        crate::ui::pad_to(
            &crate::tui::disk_layout::format_layout_capacity(model, segment.sector_count),
            13,
        ),
        percentage(segment.sector_count, model.total_sectors)
    )
}

pub(crate) fn region_row_height(width: u16) -> usize {
    if usize::from(width) >= crate::tui::table_layout::display_width(&region_header()) {
        1
    } else {
        3
    }
}

pub(super) fn compact_region_rows(
    segment: &DiskLayoutSegment,
    model: &DiskLayoutModel,
    width: u16,
    style: ratatui::style::Style,
) -> Vec<Line<'static>> {
    let capacity = crate::tui::disk_layout::format_layout_capacity(model, segment.sector_count);
    let name_width =
        usize::from(width).saturating_sub(crate::tui::table_layout::display_width(&capacity) + 2);
    let name = crate::tui::table_layout::truncate_cell(
        &segment.label,
        name_width,
        crate::tui::table_layout::TruncatePolicy::Ellipsis,
    );
    [
        format!("{name}  {capacity}"),
        format!("LBA {}", segment.closed_range()),
        format!(
            "扇区数 {} · {}",
            segment.sector_count,
            percentage(segment.sector_count, model.total_sectors)
        ),
    ]
    .into_iter()
    .map(|text| Line::from(Span::styled(text, style)))
    .collect()
}

pub(crate) fn disk_region_list_lines(model: &DiskLayoutModel) -> Vec<Line<'static>> {
    let visible = model.collapsed_tail_model();
    let theme = crate::tui::theme::current();
    let mut lines = vec![
        Line::from(Span::styled(
            "区域列表",
            theme.secondary_accent().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(region_header(), theme.secondary_accent())),
    ];
    lines.extend(visible.segments.iter().map(|segment| {
        Line::from(Span::styled(
            region_row(segment, model),
            theme.disk_region(segment.kind),
        ))
    }));
    lines
}

pub(crate) fn render_disk_region_list(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    model: &DiskLayoutModel,
    state: &DiskRegionListState,
    mode: DiskRegionListMode,
) {
    let focused = matches!(mode, DiskRegionListMode::Interactive { focused: true });
    let block = crate::tui::ui::card("区域列表", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    render_disk_region_list_body(frame, inner, model, state, mode);
}

pub(crate) fn render_disk_region_list_body(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    model: &DiskLayoutModel,
    state: &DiskRegionListState,
    mode: DiskRegionListMode,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let focused = matches!(mode, DiskRegionListMode::Interactive { focused: true });
    let visible = model.collapsed_tail_model();
    let selection = state.selection();
    let selected = match mode {
        DiskRegionListMode::Readonly => None,
        DiskRegionListMode::Interactive { .. } => selection.as_ref(),
    };
    let row_height = region_row_height(area.width);
    let row_capacity = (area.height.saturating_sub(1) as usize / row_height).max(1);
    let mut state = state.clone();
    state.reconcile(model, row_capacity);
    let start = state.viewport.offset.min(visible.segments.len());
    let end = start
        .saturating_add(row_capacity)
        .min(visible.segments.len());
    let theme = crate::tui::theme::current();
    let mut lines = vec![Line::from(Span::styled(
        if row_height == 1 {
            region_header()
        } else {
            "区域 / 容量 · LBA 范围 · 扇区数 / 占比".into()
        },
        theme.secondary_accent(),
    ))];
    lines.extend(visible.segments[start..end].iter().flat_map(|segment| {
        let active =
            selected.is_some_and(|selection| segment_matches_selection(segment, selection));
        let style = theme.apply_selection(theme.disk_region(segment.kind), active, focused);
        if row_height == 1 {
            vec![Line::from(Span::styled(region_row(segment, model), style))]
        } else {
            compact_region_rows(segment, model, area.width, style)
        }
    }));
    frame.render_widget(Paragraph::new(lines), area);
}
