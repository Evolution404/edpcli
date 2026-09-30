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

fn region_row(segment: &DiskLayoutSegment, total_sectors: u64) -> String {
    format!(
        "{}  {}  {}  {}  {}  {}",
        crate::ui::pad_to(&segment.label, 16),
        crate::ui::pad_to(segment.kind.label(), 14),
        crate::ui::pad_to(&segment.closed_range(), 23),
        crate::ui::pad_to(&segment.sector_count.to_string(), 13),
        crate::ui::pad_to(
            &crate::common::fmt_capacity(
                segment
                    .sector_count
                    .saturating_mul(crate::common::SECTOR as u64),
            ),
            13,
        ),
        percentage(segment.sector_count, total_sectors)
    )
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
            region_row(segment, model.total_sectors),
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
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let visible = model.collapsed_tail_model();
    let selection = state.selection();
    let selected = match mode {
        DiskRegionListMode::Readonly => None,
        DiskRegionListMode::Interactive { .. } => selection.as_ref(),
    };
    let row_capacity = inner.height.saturating_sub(1) as usize;
    let start = state.viewport.offset.min(visible.segments.len());
    let end = start
        .saturating_add(row_capacity)
        .min(visible.segments.len());
    let theme = crate::tui::theme::current();
    let mut lines = vec![Line::from(Span::styled(
        region_header(),
        theme.secondary_accent(),
    ))];
    lines.extend(visible.segments[start..end].iter().map(|segment| {
        let active =
            selected.is_some_and(|selection| segment_matches_selection(segment, selection));
        let style = theme.apply_selection(
            theme.disk_region(segment.kind),
            active,
            focused,
        );
        Line::from(Span::styled(
            region_row(segment, model.total_sectors),
            style,
        ))
    }));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn percentage(sectors: u64, total: u64) -> String {
    if total == 0 {
        return "0.00%".into();
    }
    let ratio = sectors as f64 * 100.0 / total as f64;
    if ratio > 0.0 && ratio < 0.01 {
        "<0.01%".into()
    } else {
        format!("{ratio:.2}%")
    }
}
