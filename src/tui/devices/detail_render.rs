use super::presentation::device_detail_lines;
use super::*;

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
    if matches!(
        key,
        crate::tui::state::DeviceInfoNodeKey::Status
            | crate::tui::state::DeviceInfoNodeKey::Backups
    ) {
        draw_status_backup_detail(frame, area, state, row, title.as_str(), focused);
        return;
    }
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

fn draw_status_backup_detail(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
    row: &crate::disk_scan::Row,
    title: &str,
    focused: bool,
) {
    use crate::application::media_identity::BackupAffinity;

    let block = crate::tui::ui::card(title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let summary = device_detail_lines(
        state,
        row,
        crate::tui::state::DeviceInfoNodeKey::Status,
        inner.width as usize,
    );
    let summary_height = (summary.len() as u16)
        .min(inner.height.saturating_sub(4))
        .max(1);
    let sections = Layout::vertical([
        Constraint::Length(summary_height),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(summary).wrap(Wrap { trim: false }),
        sections[0],
    );

    let related = state.device_related_backups();
    if related.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                if row.n_baks + row.n_possible_baks == 0 {
                    "没有识别到与当前设备相关的备份。"
                } else {
                    "关联备份已计数，备份工作区尚未加载完整列表。"
                },
                muted(),
            )),
            sections[1],
        );
        frame.render_widget(
            Paragraph::new(Span::styled("R 恢复 · 当前没有可操作备份", muted())),
            sections[2],
        );
        return;
    }

    let header = TableRow::new(["关系", "时间", "容量", "备份文件"])
        .style(crate::tui::theme::current().table_header(false, focused));
    let rows = related.iter().map(|(source, affinity)| {
        let backup = &state.backups()[*source];
        let (relation, relation_style) = match affinity {
            BackupAffinity::Confirmed => ("● 确认", success()),
            BackupAffinity::Possible => ("▲ 疑似", warning()),
            BackupAffinity::Unrelated => ("—", muted()),
        };
        let capacity = backup
            .size_bytes
            .map(crate::common::fmt_capacity)
            .unwrap_or_else(|| "—".into());
        TableRow::new([
            Cell::from(relation).style(relation_style),
            Cell::from(safe(&backup.display_time)),
            Cell::from(capacity),
            Cell::from(safe(&backup.file_name)),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(9),
            Constraint::Length(19),
            Constraint::Length(12),
            Constraint::Min(12),
        ],
    )
    .style(crate::tui::theme::current().pane_surface(focused))
    .header(header)
    .row_highlight_style(crate::tui::theme::current().selection_overlay(focused))
    .highlight_symbol("▌ ");
    let mut table_state = TableState::default();
    table_state.select(state.device_related_backup_selected_index());
    frame.render_stateful_widget(table, sections[1], &mut table_state);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("j/k", accent()),
            Span::raw(" 选择  ·  "),
            Span::styled("R", accent()),
            Span::raw(" 恢复当前备份"),
        ])),
        sections[2],
    );
}
