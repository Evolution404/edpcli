use super::*;
use super::presentation::device_detail_lines;

pub(super) fn draw_device_detail(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let focused = state.devices_focused_pane() == PaneId::DevicesDetail;
    let Some(row) = state.selected_device() else {
        frame.render_widget(
            Paragraph::new("选择设备后显示详情。").block(crate::tui::ui::card("设备详情", focused)),
            area,
        );
        return;
    };

    let key = state.device_info_selected_key();
    let title = state
        .device_info_tree_rows()
        .into_iter()
        .find(|node| node.key == key)
        .map(|node| node.label)
        .unwrap_or_else(|| "设备详情".into());
    let lines = device_detail_lines(state, row, key, area.width.saturating_sub(4) as usize);
    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card(title.as_str(), focused))
            .scroll((
                state.pane_viewport(PaneId::DevicesDetail).scroll_y.offset as u16,
                0,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

