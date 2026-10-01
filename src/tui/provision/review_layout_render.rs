use super::review_render_style::action_style;
use super::*;
use crate::tui::state::ProvisionConfirmationViewModel;

pub(super) fn draw_final_layout(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    view: &ProvisionConfirmationViewModel,
    selected: usize,
    tail: crate::tui::disk_layout::TailExpansion,
    focused: bool,
) {
    let block = crate::tui::ui::card("最终磁盘布局", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let selection = view
        .regions
        .get(selected)
        .map(|region| region.selection.clone());
    let profile = if inner.height >= 7 {
        crate::tui::disk_layout::DiskCapacityMapProfile::Full
    } else if inner.height >= 4 {
        crate::tui::disk_layout::DiskCapacityMapProfile::Compact
    } else {
        crate::tui::disk_layout::DiskCapacityMapProfile::Mini
    };
    let mut lines = crate::tui::disk_layout::DiskCapacityMap::new(&view.layout, profile)
        .with_tail(tail)
        .with_selection(selection)
        .with_marker(!matches!(
            profile,
            crate::tui::disk_layout::DiskCapacityMapProfile::Mini
        ))
        .lines(inner.width as usize);

    if lines.len() < inner.height as usize {
        if let Some(region) = view.regions.get(selected) {
            lines.push(Line::from(vec![
                Span::styled("当前  ", muted()),
                Span::styled(safe(&region.label), action_style(region.action)),
                Span::raw(format!(
                    " · LBA {}–{} · {}",
                    region.selection.start_lba,
                    region.selection.end_exclusive.saturating_sub(1),
                    AppState::format_sector_size(region.sector_count)
                )),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}
