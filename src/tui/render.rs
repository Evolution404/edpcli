//! Ratatui rendering for the top-level shell.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row as TableRow, Table, TableState, Tabs, Wrap},
    Frame,
};

use super::state::{AppState, InputMode, InspectMode, WizardStage, Workspace, WriteKind};
use super::{animation, animation::CoreMode};

fn safe(value: &str) -> String {
    crate::ui::sanitize_terminal_text(value)
}

fn hard_wrap_value(value: &str, width: usize) -> Vec<String> {
    let value = safe(value);
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for ch in value.chars() {
        let char_width = crate::ui::disp_width(&ch.to_string()).max(1);
        if current_width > 0 && current_width + char_width > width {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push(ch);
        current_width += char_width;
    }

    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

fn wrapped_field_lines(
    label: &'static str,
    value: &str,
    content_width: usize,
) -> Vec<Line<'static>> {
    let label_width = crate::ui::disp_width(label);
    let value_width = content_width.saturating_sub(label_width).max(1);
    hard_wrap_value(value, value_width)
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| {
            if index == 0 {
                Line::from(vec![Span::styled(label, muted()), Span::raw(chunk)])
            } else {
                Line::from(vec![Span::raw(" ".repeat(label_width)), Span::raw(chunk)])
            }
        })
        .collect()
}

fn accent() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn secondary() -> Style {
    Style::default().fg(Color::Magenta)
}

fn success() -> Style {
    Style::default().fg(Color::Green)
}

fn warning() -> Style {
    Style::default().fg(Color::Yellow)
}

fn danger() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}

fn muted() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn selected() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn device_status_style(row: &crate::disk_scan::Row) -> Style {
    if row.proto != "USB" || row.denied {
        warning()
    } else if row.probe_error.is_some() {
        danger()
    } else if row.device_id.is_none() {
        muted()
    } else if row.is_nopwd {
        success()
    } else {
        accent()
    }
}

fn device_status(row: &crate::disk_scan::Row) -> String {
    if row.proto != "USB" {
        "非 USB / 不支持".into()
    } else if row.denied {
        "需要管理员权限".into()
    } else if let Some(error) = &row.probe_error {
        format!("读取异常: {}", safe(error))
    } else if row.device_id.is_none() {
        "非 cems 盘".into()
    } else if row.is_nopwd {
        "cems · 免密".into()
    } else {
        "cems".into()
    }
}

fn workspace_sidebar_layout(
    area: ratatui::layout::Rect,
) -> (
    ratatui::layout::Rect,
    Option<(ratatui::layout::Rect, Option<ratatui::layout::Rect>)>,
) {
    if area.width < 108 || area.height < 12 {
        return (area, None);
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(68), Constraint::Length(40)])
        .split(area);
    if columns[1].height >= 18 {
        let sidebar = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(8), Constraint::Length(10)])
            .split(columns[1]);
        (columns[0], Some((sidebar[0], Some(sidebar[1]))))
    } else {
        (columns[0], Some((columns[1], None)))
    }
}

fn draw_workspace_animation(
    frame: &mut Frame,
    area: Option<ratatui::layout::Rect>,
    state: &AppState,
    activity: &'static str,
) {
    let Some(area) = area else {
        return;
    };
    let mode = if state.active_scan_pending() {
        CoreMode::Busy
    } else {
        CoreMode::Stable
    };
    animation::draw(frame, area, state.animation_frame(), mode, activity);
}

fn visible_window(selected: usize, total: usize, area_height: u16) -> std::ops::Range<usize> {
    let capacity = usize::from(area_height.saturating_sub(3)).max(1);
    let start = selected
        .saturating_sub(capacity / 2)
        .min(total.saturating_sub(capacity));
    start..(start + capacity).min(total)
}

fn draw_devices(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let (list_area, sidebar) = workspace_sidebar_layout(area);
    let visible_count = state.visible_device_count();
    let total_count = state.devices().len();
    let count_label = if state.workspace_filter_active() {
        format!("{visible_count}/{total_count}")
    } else {
        total_count.to_string()
    };
    let title = if state.device_scan_pending() {
        format!("设备列表 ({count_label}) · 扫描中…")
    } else {
        format!("设备列表 ({count_label})")
    };

    if visible_count == 0 {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .title_style(secondary());
        let inner = block.inner(list_area);
        frame.render_widget(block, list_area);
        let (heading, message, hint) = if state.workspace_filter_active() {
            (
                "没有匹配记录",
                "当前搜索条件没有匹配任何设备。",
                "继续输入可实时更新；清空搜索词后恢复全部设备。",
            )
        } else {
            (
                "暂无设备数据",
                "未检测到符合条件的存储设备。",
                "按 r 刷新设备；插入 U 盘后可再次扫描。",
            )
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    heading,
                    secondary().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(message),
                Line::from(hint),
                Line::from(""),
                Line::from("Tab / h / l 可切换到备份页面。"),
            ])
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
            inner,
        );
    } else {
        let window = visible_window(state.selected(), visible_count, list_area.height);
        let window_start = window.start;
        let rows = window
            .filter_map(|position| state.device_at_visible(position))
            .map(|row| {
                TableRow::new(vec![
                    Cell::from(format!("disk{}", row.disk)).style(accent()),
                    Cell::from(crate::common::fmt_gb(row.size)),
                    Cell::from(safe(&row.proto)).style(if row.proto == "USB" {
                        success()
                    } else {
                        warning()
                    }),
                    Cell::from(format!("{}:{}", safe(&row.vid), safe(&row.pid))).style(secondary()),
                    Cell::from(row.user.as_deref().map(safe).unwrap_or_else(|| "—".into())),
                    Cell::from(row.dept.as_deref().map(safe).unwrap_or_else(|| "—".into())),
                    Cell::from(device_status(row)).style(device_status_style(row)),
                ])
            });
        let header = TableRow::new(["设备", "容量", "总线", "VID:PID", "姓名", "部门", "状态"])
            .style(accent());
        let table = Table::new(
            rows,
            [
                Constraint::Length(9),
                Constraint::Length(10),
                Constraint::Length(7),
                Constraint::Length(11),
                Constraint::Length(12),
                Constraint::Min(18),
                Constraint::Length(14),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_style(secondary()),
        )
        .row_highlight_style(selected());
        let mut table_state = TableState::default();
        table_state.select(Some(state.selected().saturating_sub(window_start)));
        frame.render_stateful_widget(table, list_area, &mut table_state);
    }

    if let Some((detail_area, animation_area)) = sidebar {
        let detail = if let Some(row) = state.selected_device() {
            let onlyid = row.onlyid.as_deref().unwrap_or("—");
            let device_id = row.device_id.as_deref().unwrap_or("—");
            let content_width = detail_area.width.saturating_sub(2) as usize;
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("状态  ", muted()),
                    Span::styled(
                        device_status(row),
                        device_status_style(row).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(format!(
                    "设备  disk{}  ·  {}  ·  {}",
                    row.disk,
                    crate::common::fmt_gb(row.size),
                    safe(&row.proto)
                )),
                Line::from(format!("VID:PID  {}:{}", safe(&row.vid), safe(&row.pid))),
                Line::from(format!(
                    "姓名  {}",
                    row.user.as_deref().map(safe).unwrap_or_else(|| "—".into())
                )),
            ];
            lines.extend(wrapped_field_lines(
                "部门  ",
                row.dept.as_deref().unwrap_or("—"),
                content_width,
            ));
            lines.extend([
                Line::from(format!("onlyid  {}", safe(onlyid))),
                Line::from(format!("device_id  {}", safe(device_id))),
                Line::from(format!("已有备份  {} 份", row.n_baks)),
                Line::from(""),
                Line::from(Span::styled(
                    "可用操作",
                    secondary().add_modifier(Modifier::BOLD),
                )),
                Line::from(vec![
                    Span::styled("i", accent()),
                    Span::raw(" Inspect    "),
                    Span::styled("b", accent()),
                    Span::raw(" 新建备份"),
                ]),
                Line::from(vec![
                    Span::styled("a", warning()),
                    Span::raw(" Apply      "),
                    Span::styled("r", success()),
                    Span::raw(" 刷新"),
                ]),
            ]);
            Paragraph::new(lines)
        } else {
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "操作与信息",
                    secondary().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("选择设备后，这里会显示身份、所属人员、备份数量和可用操作。"),
                Line::from(""),
                Line::from("r  刷新设备"),
                Line::from("/  搜索设备"),
                Line::from("Tab  切换页面"),
            ])
        }
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("设备详情")
                .title_style(secondary()),
        )
        .wrap(Wrap { trim: false });
        frame.render_widget(detail, detail_area);
        draw_workspace_animation(frame, animation_area, state, "DEVICE WORKSPACE");
    }
}

fn backup_health(backup: &crate::application::BackupWorkspaceItem) -> (&'static str, Style) {
    if !backup.size_ok {
        ("大小异常", danger())
    } else {
        match backup.sha256_status {
            crate::diskio::Sha256Status::Ok => ("SHA-256 ✓", success()),
            crate::diskio::Sha256Status::Mismatch => ("SHA-256 ✗", danger()),
            crate::diskio::Sha256Status::NoSidecar => ("缺 SHA-256", warning()),
        }
    }
}

fn draw_backups(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let (list_area, sidebar) = workspace_sidebar_layout(area);
    let visible_count = state.visible_backup_count();
    let total_count = state.backups().len();
    let count_label = if state.workspace_filter_active() {
        format!("{visible_count}/{total_count}")
    } else {
        total_count.to_string()
    };
    let backup_parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4)])
        .split(list_area);

    let nopwd_count = state.backups().iter().filter(|row| row.is_nopwd).count();
    let original_count = state.backups().len().saturating_sub(nopwd_count);
    let summary_parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(38)])
        .split(backup_parts[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("总计 ", muted()),
            Span::styled(state.backups().len().to_string(), accent()),
            Span::styled("  ·  免密快照 ", muted()),
            Span::styled(nopwd_count.to_string(), success()),
            Span::styled("  ·  原盘备份 ", muted()),
            Span::styled(original_count.to_string(), accent()),
        ]))
        .block(Block::default().borders(Borders::ALL).title("备份概览")),
        summary_parts[0],
    );
    let search_active = state.input_mode() == InputMode::Search;
    let search_filtered = state.workspace_filter_active();
    let search_text = if search_active {
        format!("/{}▌", safe(state.input_buffer()))
    } else if let Some(status) = state.search_status() {
        status
    } else {
        "/ 搜索姓名、部门、onlyid".to_string()
    };
    let search_style = if search_active {
        accent()
    } else if search_filtered {
        secondary()
    } else {
        muted()
    };
    frame.render_widget(
        Paragraph::new(search_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(search_style)
                    .title(if search_active {
                        "搜索 · 实时过滤"
                    } else {
                        "搜索"
                    }),
            )
            .style(search_style),
        summary_parts[1],
    );

    let title = if state.backup_scan_pending() {
        format!("备份列表 ({count_label}) · 扫描中…")
    } else {
        format!("备份列表 ({count_label})")
    };

    if visible_count == 0 {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .title_style(secondary());
        let inner = block.inner(backup_parts[1]);
        frame.render_widget(block, backup_parts[1]);
        let (heading, message, hint) = if state.workspace_filter_active() {
            (
                "没有匹配记录",
                "当前搜索条件没有匹配任何备份。",
                "继续输入可实时更新；清空搜索词后恢复全部备份。",
            )
        } else {
            (
                "暂无备份记录",
                "先在“设备”页面选择目标 U 盘，然后按 b 创建只读备份。",
                "备份创建完成后，这里会自动刷新。",
            )
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    heading,
                    secondary().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(message),
                Line::from(hint),
            ])
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
            inner,
        );
    } else {
        let window = visible_window(state.selected(), visible_count, backup_parts[1].height);
        let window_start = window.start;
        let rows = window
            .filter_map(|position| state.backup_at_visible(position))
            .map(|backup| {
                let (health, health_style) = backup_health(backup);
                TableRow::new(vec![
                    Cell::from(backup.index.to_string()).style(accent()),
                    Cell::from(safe(&backup.display_time)),
                    Cell::from(if backup.is_nopwd {
                        "免密状态"
                    } else {
                        "加密原盘"
                    })
                    .style(if backup.is_nopwd { success() } else { accent() }),
                    Cell::from(
                        backup
                            .user
                            .as_deref()
                            .map(safe)
                            .unwrap_or_else(|| "—".into()),
                    ),
                    Cell::from(
                        backup
                            .dept
                            .as_deref()
                            .map(safe)
                            .unwrap_or_else(|| "—".into()),
                    ),
                    Cell::from(health).style(health_style),
                ])
            });
        let header = TableRow::new(["#", "时间", "状态", "姓名", "部门", "健康"]).style(accent());
        let table = Table::new(
            rows,
            [
                Constraint::Length(4),
                Constraint::Length(17),
                Constraint::Length(10),
                Constraint::Length(12),
                Constraint::Min(22),
                Constraint::Length(11),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_style(secondary()),
        )
        .row_highlight_style(selected());
        let mut table_state = TableState::default();
        table_state.select(Some(state.selected().saturating_sub(window_start)));
        frame.render_stateful_widget(table, backup_parts[1], &mut table_state);
    }

    if let Some((detail_area, animation_area)) = sidebar {
        let detail = if let Some(backup) = state.selected_backup() {
            let (health, health_style) = backup_health(backup);
            let content_width = detail_area.width.saturating_sub(2) as usize;
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("时间  ", muted()),
                    Span::raw(safe(&backup.display_time)),
                ]),
                Line::from(vec![
                    Span::styled("状态  ", muted()),
                    Span::styled(
                        if backup.is_nopwd {
                            "免密状态"
                        } else {
                            "加密原盘"
                        },
                        if backup.is_nopwd { success() } else { accent() },
                    ),
                ]),
                Line::from(format!(
                    "姓名  {}",
                    backup
                        .user
                        .as_deref()
                        .map(safe)
                        .unwrap_or_else(|| "—".into())
                )),
            ];
            lines.extend(wrapped_field_lines(
                "部门  ",
                backup.dept.as_deref().unwrap_or("—"),
                content_width,
            ));
            lines.extend([
                Line::from(format!(
                    "onlyid  {}",
                    backup
                        .onlyid
                        .as_deref()
                        .map(safe)
                        .unwrap_or_else(|| "—".into())
                )),
                Line::from(vec![
                    Span::styled("健康  ", muted()),
                    Span::styled(health, health_style.add_modifier(Modifier::BOLD)),
                ]),
            ]);
            lines.extend(wrapped_field_lines(
                "文件  ",
                &backup.file_name,
                content_width,
            ));
            lines.extend([
                Line::from(""),
                Line::from(if backup.is_nopwd {
                    "提示：这是免密状态快照；还原后不会回到加密原盘。"
                } else {
                    "提示：这是加密原盘备份，可用于恢复原始状态。"
                }),
                Line::from(""),
                Line::from(Span::styled(
                    "可用操作",
                    secondary().add_modifier(Modifier::BOLD),
                )),
                Line::from(vec![
                    Span::styled("i", accent()),
                    Span::raw(" Inspect   "),
                    Span::styled("v", success()),
                    Span::raw(" 校验"),
                ]),
                Line::from(vec![
                    Span::styled("R", warning()),
                    Span::raw(" 恢复      "),
                    Span::styled("D", danger()),
                    Span::raw(" 删除"),
                ]),
                Line::from(vec![
                    Span::styled("b", accent()),
                    Span::raw(" 新建备份   "),
                    Span::styled("r", success()),
                    Span::raw(" 刷新"),
                ]),
            ]);
            Paragraph::new(lines)
        } else {
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "备份详情",
                    secondary().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("选择一条备份后，这里会显示身份、健康状态和安全操作。"),
            ])
        }
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("备份详情")
                .title_style(secondary()),
        )
        .wrap(Wrap { trim: false });
        frame.render_widget(detail, detail_area);
        draw_workspace_animation(frame, animation_area, state, "BACKUP WORKSPACE");
    }
}

fn draw_command_palette(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let commands = [
        "devices  切到设备",
        "backups  切到备份",
        "inspect  打开 Inspect",
        "apply    Apply 安全向导",
        "restore  Restore 安全向导",
        "backup-create  备份当前设备",
        "backup-verify  校验当前备份",
        "backup-delete  删除当前备份",
        "refresh  刷新当前工作区",
        "help     帮助",
        "quit/q   退出",
    ];
    let mut lines = vec![
        Line::from(format!(":{}", safe(state.input_buffer()))),
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
        Line::from(format!("来源: {}", safe(&workspace.source))),
        Line::from(format!("LBA{} · {}", view.lba, safe(&view.method))),
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
                        .map(|value| format!("{} · ", safe(value)))
                        .unwrap_or_default();
                    lines.push(Line::from(format!(
                        "{group}{}  {}",
                        safe(&field.label),
                        safe(&field.value)
                    )));
                    for child in &field.children {
                        lines.push(Line::from(format!(
                            "  └─ {}  {}",
                            safe(&child.label),
                            safe(&child.value)
                        )));
                    }
                }
            }
            for note in &view.notes {
                lines.push(Line::from(format!("注: {}", safe(note))));
            }
        }
        InspectMode::DecodedHex => lines.extend(plain_hex_lines(&view.decoded)),
        InspectMode::RawHex => lines.extend(plain_hex_lines(&view.raw)),
    }
    let mode_index = match mode {
        InspectMode::Fields => 0,
        InspectMode::DecodedHex => 1,
        InspectMode::RawHex => 2,
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Inspect · h/l 切换 Tab · Ctrl-d/u 滚动");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let inspect_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    let tabs = Tabs::new(["字段", "Decoded Hex", "Raw Hex"])
        .select(mode_index)
        .style(muted())
        .highlight_style(selected())
        .divider(Span::styled(" │ ", muted()));
    frame.render_widget(tabs, inspect_chunks[0]);

    let scroll = state.inspect_scroll().unwrap_or(0).min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        inspect_chunks[1],
    );
}

fn draw_backup_delete(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(delete) = state.backup_delete() else {
        return;
    };
    let mut lines = vec![
        Line::from(Span::styled("删除备份", danger())),
        Line::from(vec![
            Span::styled("文件: ", accent()),
            Span::raw(safe(&delete.path.display().to_string())),
        ]),
        Line::from("安全规则：固定选中时 SHA-256 → 删除前重新扫描 → 内容复核 → 至少保留该盘 1 份备份 → 删除单文件 .edpb"),
    ];
    match delete.stage {
        WizardStage::Confirm => {
            lines.push(Line::from(Span::styled(
                "这是不可撤销操作。请输入 YES 确认删除：",
                warning(),
            )));
            lines.push(Line::from(format!("> {}", safe(&delete.confirmation))));
        }
        WizardStage::Running => {
            lines.push(Line::from(
                "正在复核并删除；q / Esc / Ctrl-C 将延迟到安全结束点。",
            ));
        }
        WizardStage::Result => {
            lines.push(Line::from("操作已结束；Esc 返回备份列表。"));
        }
    }
    if let Some(message) = &delete.message {
        lines.push(Line::from(safe(message)));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(danger())
                    .title("危险操作 · 删除备份")
                    .title_style(danger()),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// 把类型化写盘事件映射为向导 Running 阶段的单行显示文本。
/// 直接从事件类型映射，不经 ANSI 文本反解析；调用方负责经 `safe` 消毒。
fn write_progress_text(event: &crate::application::WriteEvent) -> String {
    use crate::application::WriteEvent;
    use crate::sectors::ConvertReport;
    match event {
        WriteEvent::ApplyDeviceHeader {
            disk,
            size_text,
            vid,
            pid,
        } => format!("已选定 disk{disk}（{size_text}，USB {vid}:{pid}），读取元数据…"),
        WriteEvent::ExistingBackupsHeader { count } => {
            format!("本盘已有 {count} 份备份，写入时会自动再备份")
        }
        WriteEvent::ExistingBackupsMenu { .. } => "已列出本盘既有备份清单".to_string(),
        WriteEvent::NoExistingBackups => "尚无备份；写入时自动创建首个备份".to_string(),
        WriteEvent::AlreadyNopwdHint => "该盘已是免密盘（再次写入内容相同）".to_string(),
        WriteEvent::DryRunPreview { .. } => "dry-run 预览完成，未写盘".to_string(),
        WriteEvent::ForceRewriteNotice => "--force 继续重写；自动备份将标记免密状态".to_string(),
        WriteEvent::BackupCreated { path } => format!(
            "写前备份完成：{}",
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned())
        ),
        WriteEvent::BackupCreatedIsNopwd => {
            "本份备份为免密状态快照（还原不会回到加密原盘）".to_string()
        }
        WriteEvent::RestoreCommandHint { .. } => {
            "备份完成；可用 edpcli backup restore 还原".to_string()
        }
        WriteEvent::ApplyWriteCompleted => {
            "已写入，读回校验通过；请拔出重插后格式化数据区".to_string()
        }
        WriteEvent::RestoreMatchesHeader { onlyid, count, .. } => {
            format!("onlyid={onlyid} 匹配 {count} 个备份")
        }
        WriteEvent::RestoreMatchRow {
            index,
            time,
            is_nopwd,
            ..
        } => format!(
            "[{index}] {time} {}",
            if *is_nopwd {
                "免密状态"
            } else {
                "加密原盘"
            }
        ),
        WriteEvent::RestoreSelectionRetry { message } => message.clone(),
        WriteEvent::BackupShaVerified { .. } => "备份 SHA-256 校验通过".to_string(),
        WriteEvent::RestoreSnapshotNopwdWarning => {
            "该备份为免密状态快照；dry-run 不作还原".to_string()
        }
        WriteEvent::RestoreDryRunNotice { .. } => "[dry-run] 还原预览完成，未写入".to_string(),
        WriteEvent::RestoreTargetHeader { path } => format!(
            "还原目标已确认：{}",
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned())
        ),
        WriteEvent::RestoreWriteCompleted => "已还原，读回校验通过；请拔出重插".to_string(),
        WriteEvent::Convert(ConvertReport::Identity { crc, .. }) => {
            format!("已解出盘标识（CRC32 0x{crc:08X}）")
        }
        WriteEvent::Convert(ConvertReport::Layout { .. }) => {
            "已计算 Share/Encrypt 布局".to_string()
        }
        WriteEvent::Convert(ConvertReport::SectorPlan { clears_lba9, .. }) => {
            if *clears_lba9 {
                "扇区写入计划就绪（LBA9 将清零）".to_string()
            } else {
                "扇区写入计划就绪（LBA9 已为零）".to_string()
            }
        }
    }
}

fn draw_wizard(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(wizard) = state.wizard() else {
        return;
    };
    let operation = match wizard.kind {
        WriteKind::Apply => "Apply 免密转换",
        WriteKind::Restore => "Restore 备份还原",
        WriteKind::BackupCreate => "Create Backup 只读备份",
    };
    let mut lines = vec![
        Line::from(Span::styled(
            operation,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("目标: disk{}", wizard.disk)),
    ];
    if let Some(path) = &wizard.backup {
        lines.push(Line::from(format!(
            "备份: {}",
            safe(&path.display().to_string())
        )));
    }
    lines.push(Line::from(match wizard.kind {
        WriteKind::BackupCreate => {
            "只读链：系统盘/USB整盘检查 → selector pinning → 读取协议/分区元数据/盘尾证据 → 单文件 Metadata EDPB 内部校验 → fsync；不会卸载或写 U 盘"
        }
        WriteKind::Apply | WriteKind::Restore => {
            "安全链：系统盘/USB整盘检查 → selector pinning → 写前保护 → 卸载/锁卷 → reopen复核 → atomic write → sync/readback/rollback"
        }
    }));
    match wizard.stage {
        WizardStage::Confirm => {
            lines.push(Line::from("确认后进入关键写盘阶段。请输入 YES："));
            lines.push(Line::from(format!("> {}", wizard.confirmation)));
            if let Some(message) = &wizard.message {
                lines.push(Line::from(safe(message)));
            }
        }
        WizardStage::Running => {
            lines.push(Line::from(
                "关键写盘阶段进行中；q / Esc / Ctrl-C 不会中断当前事务。",
            ));
            // 类型化事件映射为单行；尚无事件时回退到进入 Running 的初始提示。
            let progress_line = wizard.progress.as_ref().map(write_progress_text);
            if let Some(text) = progress_line.or_else(|| wizard.message.clone()) {
                lines.push(Line::from(safe(&text)));
            }
        }
        WizardStage::Result => {
            lines.push(Line::from("操作已到达安全结束点；Esc 返回。"));
            if let Some(message) = &wizard.message {
                lines.push(Line::from(safe(message)));
            }
        }
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
    let (core_mode, core_activity) = if state.is_critical_operation() {
        (CoreMode::Guard, "SAFE TRANSACTION")
    } else if state.inspect_pending() {
        (CoreMode::Busy, "READ LBA0-12")
    } else if state.active_scan_pending() {
        (CoreMode::Busy, "BACKGROUND SCAN")
    } else if state.wizard().is_some() {
        (CoreMode::Busy, "USER FLOW")
    } else {
        (CoreMode::Stable, "INTERACTIVE")
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(area);

    let title = Paragraph::new(Line::from(vec![
        Span::styled("edpcli", accent()),
        Span::styled("  TUI", secondary().add_modifier(Modifier::BOLD)),
        Span::styled("  ·  管理员模式", success()),
        animation::compact_indicator(state.animation_frame(), core_mode),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(accent()),
    );
    frame.render_widget(title, chunks[0]);

    let workspace_index = match state.workspace() {
        Workspace::Devices => 0,
        Workspace::Backups => 1,
    };
    let workspace_tabs = Tabs::new(["设备", "备份"])
        .select(workspace_index)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(accent())
                .title("页面 · Tab / h / l 切换"),
        )
        .style(muted())
        .highlight_style(selected())
        .divider(Span::styled(" │ ", muted()))
        .padding("  ", "  ");
    frame.render_widget(workspace_tabs, chunks[1]);

    let body = chunks[2];
    let overlay_active = state.inspect_data().is_some()
        || state.backup_delete().is_some()
        || state.wizard().is_some()
        || matches!(state.input_mode(), InputMode::Command | InputMode::Help);
    let (content_area, animation_area) = if overlay_active && body.width >= 118 && body.height >= 14
    {
        let parts = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(82), Constraint::Length(30)])
            .split(body);
        (parts[0], Some(parts[1]))
    } else {
        (body, None)
    };

    if state.inspect_data().is_some() {
        draw_inspect(frame, content_area, state);
    } else if state.backup_delete().is_some() {
        draw_backup_delete(frame, content_area, state);
    } else if state.wizard().is_some() {
        draw_wizard(frame, content_area, state);
    } else {
        match state.input_mode() {
            InputMode::Command => {
                draw_command_palette(frame, content_area, state);
            }
            InputMode::Help => {
                let help = Paragraph::new(vec![
                    Line::from(Span::styled("Vim 键位", accent())),
                    Line::from("Tab 切换页面   j/k/h/l 移动   gg/G 首/尾   Ctrl-d/u 半页"),
                    Line::from(vec![
                        Span::styled("/ 搜索/过滤", secondary()),
                        Span::raw("   n/N 搜索结果   "),
                        Span::styled(": 命令", secondary()),
                        Span::raw("   Esc 返回   q 退出   ? 帮助"),
                    ]),
                    Line::from(vec![
                        Span::styled("备份: ", accent()),
                        Span::styled("i 查看", secondary()),
                        Span::raw("   "),
                        Span::styled("v 校验", success()),
                        Span::raw("   "),
                        Span::styled("D 删除", danger()),
                        Span::raw("   "),
                        Span::styled("R 恢复", warning()),
                        Span::raw("   "),
                        Span::styled("b 新建", accent()),
                    ]),
                    Line::from(vec![
                        Span::styled("r", accent()),
                        Span::raw(" 刷新当前工作区"),
                    ]),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("帮助")
                        .title_style(secondary()),
                )
                .wrap(Wrap { trim: true });
                frame.render_widget(help, content_area);
            }
            _ => match state.workspace() {
                Workspace::Devices => draw_devices(frame, content_area, state),
                Workspace::Backups => draw_backups(frame, content_area, state),
            },
        }
    }

    if let Some(animation_area) = animation_area {
        animation::draw(
            frame,
            animation_area,
            state.animation_frame(),
            core_mode,
            core_activity,
        );
    }

    let status = if state.is_critical_operation() && state.backup_delete().is_some() {
        "备份删除正在执行：q / Esc / Ctrl-C 将延迟到安全检查点".to_string()
    } else if state.is_critical_operation() {
        "关键写盘阶段：q / Esc / Ctrl-C 将延迟到安全检查点".to_string()
    } else if state.input_mode() == InputMode::Search {
        if state.inspect_data().is_some() {
            format!(
                "/{}  ·  Enter 搜索  ·  Esc 取消编辑",
                safe(state.input_buffer())
            )
        } else {
            format!(
                "/{}  ·  输入即过滤  ·  Enter 确认  ·  Esc 取消编辑",
                safe(state.input_buffer())
            )
        }
    } else if state.input_mode() == InputMode::Command {
        format!(":{}", safe(state.input_buffer()))
    } else if let Some(message) = state.notice() {
        safe(message)
    } else if let Some(search) = state.search_status() {
        if state.inspect_data().is_some() {
            format!("{}  ·  n/N 下一个/上一个", safe(&search))
        } else {
            safe(&search)
        }
    } else if state.inspect_pending() {
        "后台读取 Inspect 数据中；界面可继续响应".to_string()
    } else if state.active_scan_pending() {
        "后台扫描中；界面可继续操作".to_string()
    } else {
        match state.workspace() {
            Workspace::Devices => {
                "Tab 页面  ·  j/k 移动  ·  i Inspect  ·  b 新建备份  ·  a Apply  ·  r 刷新  ·  / 搜索  ·  ? 帮助  ·  q 退出".to_string()
            }
            Workspace::Backups => {
                "Tab 页面  ·  j/k 移动  ·  i Inspect  ·  v 校验  ·  R 恢复  ·  D 删除  ·  b 新建  ·  / 搜索  ·  q 退出".to_string()
            }
        }
    };
    frame.render_widget(
        Paragraph::new(safe(&status)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(accent()),
        ),
        chunks[3],
    );
}

#[cfg(test)]
mod tests {
    use super::{hard_wrap_value, visible_window, wrapped_field_lines};

    #[test]
    fn hard_wrap_breaks_unspaced_values_by_terminal_display_width() {
        assert_eq!(
            hard_wrap_value("abcdefghijkl", 5),
            vec!["abcde", "fghij", "kl"]
        );
        assert_eq!(hard_wrap_value("江苏省电力", 6), vec!["江苏省", "电力"]);
    }

    #[test]
    fn wrapped_field_keeps_the_first_value_chunk_on_the_label_line() {
        let lines = wrapped_field_lines("文件  ", "abcdefghijkl", 10);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans.len(), 2);
        assert_eq!(lines[0].spans[0].content.as_ref(), "文件  ");
        assert_eq!(lines[0].spans[1].content.as_ref(), "abcd");
        assert_eq!(lines[1].spans[1].content.as_ref(), "efgh");
        assert_eq!(lines[2].spans[1].content.as_ref(), "ijkl");
    }

    #[test]
    fn large_tables_only_build_the_rows_visible_in_the_viewport() {
        let window = visible_window(50_000, 100_000, 24);
        assert!(window.contains(&50_000));
        assert_eq!(window.len(), 21);
    }
}
