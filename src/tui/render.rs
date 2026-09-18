//! Ratatui rendering for the top-level shell.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row as TableRow, Table, TableState, Wrap},
    Frame,
};

use super::state::{AppState, InputMode};

fn device_status(row: &crate::disk_scan::Row) -> String {
    if row.proto != "USB" {
        "非 USB / 不支持".into()
    } else if row.denied {
        "需要管理员权限".into()
    } else if let Some(error) = &row.probe_error {
        format!("读取异常: {error}")
    } else if row.device_id.is_none() {
        "非 cems 盘".into()
    } else if row.is_nopwd {
        "cems · 免密".into()
    } else {
        "cems".into()
    }
}

fn draw_devices(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let rows = state.devices().iter().map(|row| {
        TableRow::new(vec![
            Cell::from(format!("disk{}", row.disk)),
            Cell::from(crate::common::fmt_gb(row.size)),
            Cell::from(row.proto.clone()),
            Cell::from(format!("{}:{}", row.vid, row.pid)),
            Cell::from(row.user.clone().unwrap_or_else(|| "—".into())),
            Cell::from(row.dept.clone().unwrap_or_else(|| "—".into())),
            Cell::from(device_status(row)),
        ])
    });
    let header = TableRow::new(["设备", "容量", "总线", "VID:PID", "姓名", "部门", "状态"])
        .style(Style::default().add_modifier(Modifier::BOLD));
    let title = if state.device_scan_pending() {
        "设备 · 扫描中…"
    } else {
        "设备"
    };
    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(11),
            Constraint::Length(14),
            Constraint::Min(18),
            Constraint::Min(16),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(title))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut table_state = TableState::default();
    if state.item_count() > 0 {
        table_state.select(Some(state.selected()));
    }
    frame.render_stateful_widget(table, area, &mut table_state);
}

pub fn draw(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(2),
        ])
        .split(area);

    let title = Paragraph::new(Line::from(vec![
        Span::styled("edpcli", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  TUI"),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, chunks[0]);

    match state.input_mode() {
        InputMode::Help => {
            let help = Paragraph::new(vec![
                Line::from("Vim 键位"),
                Line::from("j/k/h/l 移动   gg/G 首/尾   Ctrl-d/u 半页"),
                Line::from("/ 搜索   n/N 匹配   : 命令   Esc 返回   q 退出   ? 帮助"),
                Line::from("r 刷新设备"),
            ])
            .block(Block::default().borders(Borders::ALL).title("帮助"))
            .wrap(Wrap { trim: true });
            frame.render_widget(help, chunks[1]);
        }
        _ => draw_devices(frame, chunks[1], state),
    }

    let status = if state.is_critical_operation() {
        "关键写盘阶段：q / Esc / Ctrl-C 将延迟到安全检查点"
    } else if state.device_scan_pending() {
        "后台扫描中；界面可继续操作"
    } else {
        "j/k 移动  r 刷新  ? 帮助  : 命令  / 搜索  q 退出"
    };
    frame.render_widget(Paragraph::new(status), chunks[2]);
}
