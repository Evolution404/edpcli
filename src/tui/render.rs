//! Ratatui rendering for the top-level shell.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row as TableRow, Table, TableState, Wrap},
    Frame,
};

use super::state::{AppState, InputMode, InspectMode, WizardStage, Workspace, WriteKind};

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

fn draw_backups(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let rows = state.backups().iter().map(|backup| {
        let health = if !backup.size_ok {
            "大小异常".to_string()
        } else {
            match backup.md5_status {
                crate::diskio::Md5Status::Ok => "MD5 ✓".to_string(),
                crate::diskio::Md5Status::Mismatch => "MD5 ✗".to_string(),
                crate::diskio::Md5Status::NoSidecar => "缺 MD5".to_string(),
            }
        };
        TableRow::new(vec![
            Cell::from(backup.index.to_string()),
            Cell::from(backup.display_time.clone()),
            Cell::from(if backup.is_nopwd {
                "免密状态"
            } else {
                "加密原盘"
            }),
            Cell::from(backup.user.clone().unwrap_or_else(|| "—".into())),
            Cell::from(backup.dept.clone().unwrap_or_else(|| "—".into())),
            Cell::from(backup.onlyid.clone().unwrap_or_else(|| "—".into())),
            Cell::from(health),
        ])
    });
    let header = TableRow::new(["#", "时间", "状态", "姓名", "部门", "onlyid", "健康"])
        .style(Style::default().add_modifier(Modifier::BOLD));
    let title = if state.backup_scan_pending() {
        "备份 · 扫描中…"
    } else {
        "备份"
    };
    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(17),
            Constraint::Length(10),
            Constraint::Length(14),
            Constraint::Min(18),
            Constraint::Length(14),
            Constraint::Length(10),
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

fn draw_command_palette(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let commands = [
        "devices  切到设备",
        "backups  切到备份",
        "inspect  打开 Inspect",
        "apply    Apply 安全向导",
        "restore  Restore 安全向导",
        "refresh  刷新当前工作区",
        "help     帮助",
        "quit/q   退出",
    ];
    let mut lines = vec![
        Line::from(format!(":{}", state.input_buffer())),
        Line::from(""),
    ];
    lines.extend(commands.into_iter().map(Line::from));
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Command Palette"),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn plain_hex_lines(data: &[u8]) -> Vec<Line<'static>> {
    data.chunks(16)
        .enumerate()
        .map(|(line_no, chunk)| {
            let offset = line_no * 16;
            let mut hex = String::new();
            let mut ascii = String::new();
            for i in 0..16 {
                if i == 8 {
                    hex.push(' ');
                }
                if let Some(byte) = chunk.get(i) {
                    hex.push_str(&format!("{byte:02X} "));
                    ascii.push(if (0x20..=0x7e).contains(byte) {
                        *byte as char
                    } else {
                        '.'
                    });
                } else {
                    hex.push_str("   ");
                }
            }
            Line::from(format!("+0x{offset:03X}: {hex} {ascii}"))
        })
        .collect()
}

fn draw_inspect(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(workspace) = state.inspect_data() else {
        return;
    };
    let Some(lba) = state.inspect_selected_lba() else {
        return;
    };
    let Some(view) = workspace.views.get(lba as usize) else {
        return;
    };
    let mode = state.inspect_mode().unwrap_or(InspectMode::Fields);
    let mut lines = vec![
        Line::from(format!("来源: {}", workspace.source)),
        Line::from(format!("LBA{} · {}", view.lba, view.method)),
        Line::from(""),
    ];
    match mode {
        InspectMode::Fields => {
            if view.fields.is_empty() {
                lines.push(Line::from("未检测到已知结构化字段。"));
            } else {
                for field in &view.fields {
                    let group = field
                        .group
                        .as_deref()
                        .map(|value| format!("{value} · "))
                        .unwrap_or_default();
                    lines.push(Line::from(format!(
                        "{group}{}  {}",
                        field.label, field.value
                    )));
                    for child in &field.children {
                        lines.push(Line::from(format!("  └─ {}  {}", child.label, child.value)));
                    }
                }
            }
            for note in &view.notes {
                lines.push(Line::from(format!("注: {note}")));
            }
        }
        InspectMode::DecodedHex => lines.extend(plain_hex_lines(&view.decoded)),
        InspectMode::RawHex => lines.extend(plain_hex_lines(&view.raw)),
    }
    let mode_label = match mode {
        InspectMode::Fields => "字段",
        InspectMode::DecodedHex => "Decoded Hex",
        InspectMode::RawHex => "Raw Hex",
    };
    let scroll = state.inspect_scroll().unwrap_or(0).min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(format!(
                "Inspect · {mode_label} · h/l 切换视图 · Ctrl-d/u 滚动"
            )))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
}

fn draw_wizard(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(wizard) = state.wizard() else {
        return;
    };
    let operation = match wizard.kind {
        WriteKind::Apply => "Apply 免密转换",
        WriteKind::Restore => "Restore 备份还原",
    };
    let mut lines = vec![
        Line::from(Span::styled(
            operation,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("目标: disk{}", wizard.disk)),
    ];
    if let Some(path) = &wizard.backup {
        lines.push(Line::from(format!("备份: {}", path.display())));
    }
    lines.push(Line::from(
        "安全链：系统盘/USB整盘检查 → selector pinning → 写前保护 → 卸载/锁卷 → reopen复核 → atomic write → sync/readback/rollback",
    ));
    match wizard.stage {
        WizardStage::Confirm => {
            lines.push(Line::from("确认后进入关键写盘阶段。请输入 YES："));
            lines.push(Line::from(format!("> {}", wizard.confirmation)));
        }
        WizardStage::Running => {
            lines.push(Line::from(
                "关键写盘阶段进行中；q / Esc / Ctrl-C 不会中断当前事务。",
            ));
        }
        WizardStage::Result => {
            lines.push(Line::from("操作已到达安全结束点；Esc 返回。"));
        }
    }
    if let Some(message) = &wizard.message {
        lines.push(Line::from(message.clone()));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("安全向导"))
            .wrap(Wrap { trim: true }),
        area,
    );
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

    if state.inspect_data().is_some() {
        draw_inspect(frame, chunks[1], state);
    } else if state.wizard().is_some() {
        draw_wizard(frame, chunks[1], state);
    } else {
        match state.input_mode() {
            InputMode::Command => {
                draw_command_palette(frame, chunks[1], state);
            }
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
            _ => match state.workspace() {
                Workspace::Devices => draw_devices(frame, chunks[1], state),
                Workspace::Backups => draw_backups(frame, chunks[1], state),
            },
        }
    }

    let status = if state.is_critical_operation() {
        "关键写盘阶段：q / Esc / Ctrl-C 将延迟到安全检查点".to_string()
    } else if state.input_mode() == InputMode::Search {
        format!("/{}", state.input_buffer())
    } else if state.input_mode() == InputMode::Command {
        format!(":{}", state.input_buffer())
    } else if let Some(message) = state.notice() {
        message.to_string()
    } else if let Some(search) = state.search_status() {
        format!("{search}  ·  n/N 下一个/上一个")
    } else if state.inspect_pending() {
        "后台读取 Inspect 数据中；界面可继续响应".to_string()
    } else if state.active_scan_pending() {
        "后台扫描中；界面可继续操作".to_string()
    } else {
        "h/l 工作区  j/k 移动  i Inspect  a Apply  R Restore  r 刷新  ? 帮助  : 命令  / 搜索  q 退出".to_string()
    };
    frame.render_widget(Paragraph::new(status), chunks[2]);
}
