//! Ratatui rendering for the top-level shell.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use super::state::{AppState, InputMode};

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

    let body = match state.input_mode() {
        InputMode::Help => Paragraph::new(vec![
            Line::from("Vim 键位"),
            Line::from("j/k/h/l 移动   gg/G 首/尾   Ctrl-d/u 半页"),
            Line::from("/ 搜索   n/N 匹配   : 命令   Esc 返回   q 退出   ? 帮助"),
        ]),
        _ => Paragraph::new(vec![
            Line::from("Device dashboard"),
            Line::from("设备扫描将在后台任务中刷新，不阻塞终端 redraw。"),
            Line::from(format!(
                "当前选择: {} / {}",
                state.selected().saturating_add(usize::from(state.item_count() > 0)),
                state.item_count()
            )),
        ]),
    }
    .block(Block::default().borders(Borders::ALL).title("工作区"))
    .wrap(Wrap { trim: true });
    frame.render_widget(body, chunks[1]);

    let status = if state.is_critical_operation() {
        "关键写盘阶段：q / Esc / Ctrl-C 将延迟到安全检查点"
    } else {
        "j/k 移动  ? 帮助  : 命令  / 搜索  q 退出"
    };
    frame.render_widget(Paragraph::new(status), chunks[2]);
}
