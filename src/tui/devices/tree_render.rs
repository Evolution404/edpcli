use super::*;
use crate::tui::state::{DeviceInfoNodeKey, DeviceInfoTreeNode};

pub(super) fn draw_device_tree(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let focused = state.devices_focused_pane() == PaneId::DevicesTree;
    let Some(_) = state.selected_device() else {
        frame.render_widget(
            Paragraph::new("请先在设备列表中选择设备。")
                .block(crate::tui::ui::card("设备信息", focused)),
            area,
        );
        return;
    };

    let selected = state.device_info_selected_key();
    let rows = state.device_info_tree_rows();
    let lines = rows
        .iter()
        .map(|row| device_tree_line(row, selected, focused))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card("设备信息", focused))
            .scroll((
                state.pane_viewport(PaneId::DevicesTree).scroll_y.offset as u16,
                0,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn device_tree_line(
    row: &DeviceInfoTreeNode,
    selected: DeviceInfoNodeKey,
    focused: bool,
) -> Line<'static> {
    let active = row.key == selected;
    let marker = if focused && active { "▌" } else { " " };
    let disclosure = if row.expandable {
        if row.expanded {
            "▾"
        } else {
            "▸"
        }
    } else {
        " "
    };
    let branch = match row.depth {
        0 => "",
        1 => "├─ ",
        _ => "  ├─ ",
    };
    let style = if active { accent() } else { secondary() };
    let mut spans = vec![
        Span::styled(marker, style),
        Span::raw(" "),
        Span::raw("  ".repeat(row.depth as usize)),
        Span::styled(disclosure, style),
        Span::raw(" "),
        Span::styled(branch, muted()),
        Span::styled(row.label.clone(), style.add_modifier(Modifier::BOLD)),
    ];
    if let Some(value) = &row.value {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(value.clone(), muted()));
    }
    Line::from(spans)
}
