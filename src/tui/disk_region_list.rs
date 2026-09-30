use ratatui::{
    style::Modifier,
    text::{Line, Span},
};

use super::disk_layout::DiskLayoutModel;

pub(crate) fn disk_region_list_lines(model: &DiskLayoutModel) -> Vec<Line<'static>> {
    let visible = model.collapsed_tail_model();
    let theme = crate::tui::theme::current();
    let mut lines = vec![
        Line::from(Span::styled(
            "区域列表",
            theme.secondary_accent().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(
                "{}  {}  {}  {}",
                crate::ui::pad_to("区域", 18),
                crate::ui::pad_to("LBA 范围", 24),
                crate::ui::pad_to("容量", 14),
                "占比"
            ),
            theme.secondary_accent(),
        )),
    ];

    for segment in &visible.segments {
        lines.push(Line::from(Span::styled(
            format!(
                "{}  {}  {}  {}",
                crate::ui::pad_to(&segment.label, 18),
                crate::ui::pad_to(&segment.closed_range(), 24),
                crate::ui::pad_to(
                    &crate::common::fmt_capacity(
                        segment
                            .sector_count
                            .saturating_mul(crate::common::SECTOR as u64),
                    ),
                    14,
                ),
                percentage(segment.sector_count, model.total_sectors)
            ),
            theme.disk_region(segment.kind),
        )));
    }
    lines
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
