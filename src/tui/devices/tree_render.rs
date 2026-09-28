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
    let selected_index = rows.iter().position(|row| row.key == selected).unwrap_or(0);
    let visible_rows = area.height.saturating_sub(2).max(1) as usize;
    let stored_offset = state.pane_viewport(PaneId::DevicesTree).scroll_y.offset;
    let scroll_offset = tree_scroll_offset(rows.len(), selected_index, stored_offset, visible_rows);
    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card("设备信息", focused))
            .scroll((scroll_offset.min(u16::MAX as usize) as u16, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn tree_scroll_offset(
    content_len: usize,
    selected_index: usize,
    stored_offset: usize,
    visible_rows: usize,
) -> usize {
    let visible_rows = visible_rows.max(1);
    let max_offset = content_len.saturating_sub(visible_rows);
    let mut offset = stored_offset.min(max_offset);
    if selected_index < offset {
        offset = selected_index;
    } else if selected_index >= offset.saturating_add(visible_rows) {
        offset = selected_index
            .saturating_add(1)
            .saturating_sub(visible_rows);
    }
    offset.min(max_offset)
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
    let region_style = match row.key {
        DeviceInfoNodeKey::LayoutSegment { kind, .. } => {
            Some(crate::tui::theme::current().disk_region_tree(kind, active))
        }
        DeviceInfoNodeKey::TailGroup => Some(
            crate::tui::theme::current()
                .disk_region_tree(crate::tui::disk_layout::DiskRegionKind::Tail, active),
        ),
        _ => None,
    };
    let style = region_style.unwrap_or_else(|| if active { accent() } else { secondary() });
    let marker_style = style;
    let structural_style = if active && region_style.is_some() {
        style
    } else {
        ratatui::style::Style::default()
    };
    let branch_style = if active && region_style.is_some() {
        style
    } else {
        muted()
    };
    let mut spans = vec![
        Span::styled(marker, marker_style),
        Span::styled(" ", structural_style),
        Span::styled("  ".repeat(row.depth as usize), structural_style),
        Span::styled(disclosure, style),
        Span::styled(" ", structural_style),
        Span::styled(branch, branch_style),
        Span::styled(row.label.clone(), style.add_modifier(Modifier::BOLD)),
    ];
    if let Some(value) = &row.value {
        spans.push(Span::styled("  ", structural_style));
        let value_style = match row.key {
            DeviceInfoNodeKey::Identity => match value.as_str() {
                "强" => success().add_modifier(Modifier::BOLD),
                "中" | "待确认" => warning().add_modifier(Modifier::BOLD),
                "弱" => danger().add_modifier(Modifier::BOLD),
                _ => muted(),
            },
            DeviceInfoNodeKey::Status if value.starts_with("正常") => success(),
            DeviceInfoNodeKey::Status if value.starts_with("异常") => danger(),
            _ => region_style.unwrap_or_else(muted),
        };
        spans.push(Span::styled(value.clone(), value_style));
    }
    Line::from(spans)
}
