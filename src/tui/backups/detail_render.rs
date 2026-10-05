use super::*;

pub(super) fn draw_backup_metadata(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    let pane = crate::tui::pane::PaneId::BackupSummary;
    let base = crate::tui::ui::card("备份元数据", state.backups_focused_pane() == pane);
    let inner = base.inner(area);

    let content = crate::tui::backup_metadata::backup_metadata_lines(state, inner.width);
    let max_scroll = content.len().saturating_sub(usize::from(inner.height));
    let count = content.len();
    let paragraph = Paragraph::new(content);
    let offset = state
        .pane_viewport(pane)
        .scroll_y
        .offset
        .min(max_scroll)
        .min(u16::MAX as usize);
    let title = format!(
        "备份元数据 · {}-{}/{}",
        (offset + 1).min(count),
        (offset + usize::from(inner.height)).min(count),
        count
    );
    frame.render_widget(
        crate::tui::ui::card(title.as_str(), state.backups_focused_pane() == pane),
        area,
    );
    frame.render_widget(paragraph.scroll((offset as u16, 0)), inner);
}
