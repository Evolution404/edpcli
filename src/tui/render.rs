//! Ratatui rendering for the top-level shell.

use std::collections::HashMap;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row as TableRow, Table, TableState, Tabs, Wrap},
    Frame,
};

use super::state::{
    AppState, InputMode, InspectMode, ProvisionBarKind, ProvisionKind, ProvisionPrepared,
    ProvisionStage, WizardStage, Workspace, WriteKind,
};
use super::{animation, animation::CoreMode};

fn safe(value: &str) -> String {
    crate::ui::sanitize_terminal_text(value)
}

fn fit_display_width(value: &str, width: usize) -> String {
    let value = safe(value);
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "│".into();
    }
    let current = crate::ui::disp_width(&value);
    if current <= width {
        return format!("{value}{}", " ".repeat(width - current));
    }

    let target = width.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0usize;
    for ch in value.chars() {
        let ch_width = crate::ui::disp_width(&ch.to_string()).max(1);
        if used + ch_width > target {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    if width > 0 {
        out.push('…');
        used += 1;
    }
    if used < width {
        out.push_str(&" ".repeat(width - used));
    }
    out
}

fn input_value_window(value: &str, cursor: usize, width: usize, secret: bool) -> String {
    if width == 0 {
        return String::new();
    }
    let sanitized = safe(value);
    let chars = if secret {
        vec!['•'; sanitized.chars().count()]
    } else {
        sanitized.chars().collect::<Vec<_>>()
    };
    let cursor = cursor.min(chars.len());
    let content_budget = width.saturating_sub(2).max(1);
    let mut start = cursor;
    let mut end = cursor;
    let mut used = 1usize; // cursor marker
    loop {
        let mut progressed = false;
        if start > 0 {
            let candidate = chars[start - 1];
            let w = crate::ui::disp_width(&candidate.to_string()).max(1);
            if used + w <= content_budget {
                start -= 1;
                used += w;
                progressed = true;
            }
        }
        if end < chars.len() {
            let candidate = chars[end];
            let w = crate::ui::disp_width(&candidate.to_string()).max(1);
            if used + w <= content_budget {
                end += 1;
                used += w;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }

    let mut out = String::new();
    if start > 0 {
        out.push('‹');
    }
    for ch in &chars[start..cursor] {
        out.push(*ch);
    }
    out.push('│');
    for ch in &chars[cursor..end] {
        out.push(*ch);
    }
    if end < chars.len() {
        out.push('›');
    }
    out
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

fn provision_kind_style(kind: ProvisionKind) -> Style {
    let color = match kind {
        ProvisionKind::Mode0 => Color::LightCyan,
        ProvisionKind::Mode1 => Color::LightMagenta,
        ProvisionKind::Mode2 => Color::LightYellow,
        ProvisionKind::Mode3 => Color::LightGreen,
        ProvisionKind::Plain => Color::LightBlue,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn device_status_style(row: &crate::disk_scan::Row) -> Style {
    if row.proto != "USB" || row.denied {
        warning()
    } else if row.probe_error.is_some() {
        danger()
    } else if row.provision_kind == crate::provision::DiskProvisionKind::Plain {
        muted()
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
    } else {
        row.provision_kind.full_name().into()
    }
}

fn device_ven_prod(device_id: Option<&str>) -> String {
    let mut ven = None;
    let mut prod = None;
    for part in device_id.unwrap_or_default().split('&') {
        ven = ven.or_else(|| part.strip_prefix("ven_"));
        prod = prod.or_else(|| part.strip_prefix("prod_"));
    }
    match (ven, prod) {
        (Some(ven), Some(prod)) => safe(&format!("{ven}_{prod}")),
        (Some(ven), None) => safe(ven),
        (None, Some(prod)) => safe(prod),
        _ => "—".into(),
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
    let (list_area, sidebar) = if area.width >= 150 {
        workspace_sidebar_layout(area)
    } else {
        (area, None)
    };
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
                    Cell::from(device_ven_prod(row.device_id.as_deref())),
                    Cell::from(
                        row.onlyid
                            .as_deref()
                            .map(safe)
                            .unwrap_or_else(|| "—".into()),
                    ),
                    Cell::from(row.user.as_deref().map(safe).unwrap_or_else(|| "—".into())),
                    Cell::from(row.dept.as_deref().map(safe).unwrap_or_else(|| "—".into())),
                    Cell::from(device_status(row)).style(device_status_style(row)),
                ])
            });
        let header = TableRow::new([
            "设备", "容量", "总线", "VID:PID", "ven_prod", "onlyid", "姓名", "部门", "盘型",
        ])
        .style(accent());
        let table = Table::new(
            rows,
            [
                Constraint::Length(9),
                Constraint::Length(9),
                Constraint::Length(5),
                Constraint::Length(9),
                Constraint::Length(17),
                Constraint::Length(12),
                Constraint::Length(10),
                Constraint::Min(10),
                Constraint::Length(17),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_style(secondary()),
        )
        .row_highlight_style(selected())
        .highlight_symbol("▶ ");
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
                    Span::styled("盘型  ", muted()),
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
                Line::from(vec![Span::styled("r", success()), Span::raw(" 刷新")]),
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

    let summary_parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(38)])
        .split(backup_parts[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("总计 ", muted()),
            Span::styled(state.backups().len().to_string(), accent()),
            Span::styled("  ·  已选 ", muted()),
            Span::styled(state.backup_selection_count().to_string(), warning()),
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
                    Cell::from(if state.backup_is_selected(&backup.path) {
                        "✓"
                    } else {
                        ""
                    })
                    .style(if state.backup_is_selected(&backup.path) {
                        warning()
                    } else {
                        muted()
                    }),
                    Cell::from(backup.index.to_string()).style(accent()),
                    Cell::from(safe(&backup.display_time)),
                    Cell::from(backup.provision_kind.short_name()).style(accent()),
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
        let header =
            TableRow::new(["选", "#", "时间", "盘型", "姓名", "部门", "健康"]).style(accent());
        let table = Table::new(
            rows,
            [
                Constraint::Length(3),
                Constraint::Length(4),
                Constraint::Length(17),
                Constraint::Length(23),
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
        .row_highlight_style(selected())
        .highlight_symbol("▶ ");
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
                    Span::styled("盘型  ", muted()),
                    Span::styled(backup.provision_kind.full_name(), accent()),
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
                Line::from("提示：EDPB 保存协议与选定元数据范围，不保证包含普通分区全部用户文件。"),
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

fn draw_provision(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let provision = state.provision();
    let (main_area, sidebar) = workspace_sidebar_layout(area);

    let target_lines = if let Some(row) = if provision.stage == ProvisionStage::SelectDisk {
        state.provision_device_at(state.selected())
    } else {
        state.selected_device()
    } {
        vec![
            Line::from(vec![
                Span::styled(format!("disk{}", row.disk), accent()),
                Span::raw(format!(
                    "  {:.2} GiB",
                    row.size as f64 / 1024.0 / 1024.0 / 1024.0
                )),
            ]),
            Line::from(vec![
                Span::styled("接口  ", muted()),
                Span::styled(safe(&row.proto), secondary()),
                Span::raw("   "),
                Span::styled(format!("{}:{}", safe(&row.vid), safe(&row.pid)), muted()),
            ]),
            Line::from(vec![
                Span::styled("盘型  ", muted()),
                Span::styled(device_status(row), device_status_style(row)),
            ]),
            Line::from(vec![
                Span::styled("标签  ", muted()),
                Span::raw(safe(row.onlyid.as_deref().unwrap_or("未读取"))),
            ]),
            Line::from(vec![
                Span::styled("用户  ", muted()),
                Span::raw(safe(row.user.as_deref().unwrap_or("未读取"))),
            ]),
        ]
    } else {
        vec![
            Line::from(Span::styled("未固定目标 USB", danger())),
            Line::from("请在左侧列表选择可用 USB 目标盘。"),
        ]
    };

    if let Some((side_top, side_bottom)) = sidebar {
        frame.render_widget(
            Paragraph::new(target_lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("固定目标")
                        .title_style(accent()),
                )
                .wrap(Wrap { trim: true }),
            side_top,
        );
        if let Some(side_bottom) = side_bottom {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("安全不变量", warning())),
                    Line::from("• 仅允许 USB 整盘目标"),
                    Line::from("• LBA3 厂商数据原样保留"),
                    Line::from("• 写前固定硬件身份/容量"),
                    Line::from("• MBR 最后提交"),
                    Line::from("• 协议写入失败回滚；格式化失败保留制盘"),
                    Line::from("• 保留分区保持原位置与密钥材料"),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(warning())
                        .title("写盘保护"),
                )
                .wrap(Wrap { trim: true }),
                side_bottom,
            );
        }
    }

    match provision.stage {
        ProvisionStage::SelectDisk => {
            let rows = (0..state.item_count()).filter_map(|index| {
                let row = state.provision_device_at(index)?;
                Some(TableRow::new(vec![
                    Cell::from(format!("disk{}", row.disk)),
                    Cell::from(format!("{:.2} GiB", row.size as f64 / 1_073_741_824.0)),
                    Cell::from(format!("{}:{}", safe(&row.vid), safe(&row.pid))),
                    Cell::from(device_status(row)),
                    Cell::from(safe(row.onlyid.as_deref().unwrap_or("—"))),
                ]))
            });
            let table = Table::new(
                rows,
                [
                    Constraint::Length(9),
                    Constraint::Length(12),
                    Constraint::Length(13),
                    Constraint::Min(16),
                    Constraint::Length(15),
                ],
            )
            .header(TableRow::new(["设备", "容量", "USB 身份", "盘型", "onlyid"]).style(accent()))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("制盘 · 先选择 USB 目标"),
            )
            .row_highlight_style(selected())
            .highlight_symbol("▶ ");
            let mut table_state = ratatui::widgets::TableState::default();
            if state.item_count() > 0 {
                table_state.select(Some(state.selected()));
            }
            frame.render_stateful_widget(table, main_area, &mut table_state);
        }
        ProvisionStage::BackupPrompt => {
            let choices = ["先保存当前盘，再选择制盘模式", "不保存，直接选择制盘模式"];
            let mut lines = vec![
                Line::from(Span::styled("制盘前是否保存当前盘？", secondary())),
                Line::from(""),
                Line::from(Span::styled(
                    safe(&state.provision_backup_summary()),
                    warning(),
                )),
                Line::from("保存会创建当前盘的 EDPB 元数据备份，不会修改 U 盘。"),
                Line::from(""),
            ];
            for (index, choice) in choices.iter().enumerate() {
                lines.push(Line::from(if index == state.selected() {
                    vec![
                        Span::styled("▶ ", selected()),
                        Span::styled(*choice, selected()),
                    ]
                } else {
                    vec![Span::raw("  "), Span::raw(*choice)]
                }));
            }
            lines.extend([
                Line::from(""),
                Line::from(vec![
                    Span::styled("↑/↓", accent()),
                    Span::raw(" 选择   "),
                    Span::styled("Enter", success()),
                    Span::raw(" 确认   "),
                    Span::styled("Esc", warning()),
                    Span::raw(" 返回设备页"),
                ]),
            ]);
            if let Some(message) = &provision.message {
                lines.push(Line::from(Span::styled(safe(message), danger())));
            }
            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(secondary())
                            .title("制盘前保存"),
                    )
                    .wrap(Wrap { trim: true }),
                main_area,
            );
        }
        ProvisionStage::BackupSaving => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("◈ 正在保存当前盘", secondary())),
                    Line::from(""),
                    Line::from(safe(
                        provision
                            .message
                            .as_deref()
                            .unwrap_or("正在创建制盘前 EDPB 备份…"),
                    )),
                    Line::from("完成前不会进入制盘模式选择。"),
                ])
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(secondary())
                        .title("保存当前盘"),
                ),
                main_area,
            );
        }
        ProvisionStage::Menu => {
            let rows = ProvisionKind::ALL
                .into_iter()
                .enumerate()
                .map(|(index, kind)| {
                    TableRow::new(vec![
                        Cell::from(Span::styled(format!("{index}"), provision_kind_style(kind))),
                        Cell::from(Span::styled(kind.title(), provision_kind_style(kind))),
                        Cell::from(kind.description()),
                    ])
                });
            let table = Table::new(
                rows,
                [
                    Constraint::Length(4),
                    Constraint::Length(30),
                    Constraint::Min(28),
                ],
            )
            .header(
                TableRow::new(["#", "制盘方案", "布局 / 行为"])
                    .style(accent())
                    .bottom_margin(1),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(secondary())
                    .title("制盘中心 · 选择方案")
                    .title_style(secondary()),
            )
            .row_highlight_style(selected())
            .highlight_symbol("▶ ");
            let mut table_state = TableState::default();
            table_state.select(Some(state.selected()));
            frame.render_stateful_widget(table, main_area, &mut table_state);
        }
        ProvisionStage::Form => {
            let wide = main_area.width >= 96;
            let (form_area, layout_area) = if wide {
                let areas =
                    Layout::horizontal([Constraint::Percentage(56), Constraint::Percentage(44)])
                        .split(main_area);
                (areas[0], areas[1])
            } else {
                let areas =
                    Layout::vertical([Constraint::Percentage(62), Constraint::Percentage(38)])
                        .split(main_area);
                (areas[0], areas[1])
            };
            let content_width = form_area.width.saturating_sub(2) as usize;
            let separator = " │ ";
            let separator_width = crate::ui::disp_width(separator);

            let fields = state.provision_visible_fields();
            let rows = state.provision_compact_field_rows();
            let mut section_metrics: HashMap<&str, (usize, usize, usize, usize)> = HashMap::new();
            for (section, indexes) in &rows {
                let entry = section_metrics.entry(*section).or_insert((0, 0, 0, 0));
                for (position, index) in indexes.iter().copied().enumerate() {
                    let (label, value, secret) = &fields[index];
                    let label_width = crate::ui::disp_width(label);
                    let shown_width = if value.is_empty() {
                        crate::ui::disp_width("〈请输入〉")
                    } else if *secret {
                        value.chars().count()
                    } else {
                        crate::ui::disp_width(&safe(value))
                    }
                    .clamp(6, 24);
                    if position == 0 {
                        entry.0 = entry.0.max(label_width);
                        entry.2 = entry.2.max(shown_width);
                    } else {
                        entry.1 = entry.1.max(label_width);
                        entry.3 = entry.3.max(shown_width);
                    }
                }
            }

            let mut form_lines = vec![Line::from(vec![
                Span::styled(provision.kind.title(), provision_kind_style(provision.kind)),
                Span::raw("  "),
                Span::styled(provision.kind.description(), muted()),
            ])];
            let mut current_section: Option<&str> = None;
            let mut selected_line = 0usize;
            for (section, indexes) in rows {
                if current_section != Some(section) {
                    form_lines.push(Line::from(""));
                    form_lines.push(Line::from(Span::styled(section, secondary())));
                    current_section = Some(section);
                }
                let mut spans = Vec::new();
                let row_selected = indexes.contains(&provision.field_selected);
                if row_selected {
                    selected_line = form_lines.len();
                }
                let two_columns = indexes.len() == 2;
                for (position, index) in indexes.into_iter().enumerate() {
                    let (label, value, secret) = &fields[index];
                    let active = index == provision.field_selected;
                    if position > 0 {
                        spans.push(Span::styled(separator, muted()));
                    }
                    let metrics = section_metrics
                        .get(section)
                        .copied()
                        .unwrap_or((0, 0, 8, 8));
                    let min_right_width = 2 + metrics.1 + 1 + metrics.3.max(8);
                    let desired_left_width = 2 + metrics.0 + 1 + metrics.2.max(8);
                    let max_left_width = content_width
                        .saturating_sub(separator_width)
                        .saturating_sub(min_right_width)
                        .max(8);
                    let section_left_width = desired_left_width.min(max_left_width);
                    let cell_width = if two_columns {
                        if position == 0 {
                            section_left_width
                        } else {
                            content_width
                                .saturating_sub(section_left_width)
                                .saturating_sub(separator_width)
                        }
                    } else {
                        content_width
                    };
                    let label_width = if position == 0 { metrics.0 } else { metrics.1 };
                    let label_width = label_width.min(cell_width.saturating_sub(4));
                    let value_width = cell_width
                        .saturating_sub(2)
                        .saturating_sub(label_width)
                        .saturating_sub(1)
                        .max(1);

                    spans.push(Span::styled(
                        if active { "▶ " } else { "  " },
                        if active { accent() } else { Style::default() },
                    ));
                    spans.push(Span::styled(fit_display_width(label, label_width), muted()));
                    spans.push(Span::raw(" "));

                    let editable_active = active && state.provision_selected_field_is_editable();
                    let shown = if editable_active {
                        input_value_window(
                            value,
                            state.provision_field_cursor(),
                            value_width,
                            *secret,
                        )
                    } else if value.is_empty() {
                        fit_display_width("〈请输入〉", value_width)
                    } else if *secret {
                        fit_display_width(&"•".repeat(value.chars().count()), value_width)
                    } else {
                        fit_display_width(value, value_width)
                    };
                    let shown_width = crate::ui::disp_width(&shown).min(value_width);
                    spans.push(Span::styled(
                        shown,
                        if active { selected() } else { Style::default() },
                    ));
                    if editable_active && shown_width < value_width {
                        spans.push(Span::raw(" ".repeat(value_width - shown_width)));
                    }
                }
                form_lines.push(Line::from(spans));
            }
            if let Some(hint) = state.provision_field_hint(provision.field_selected) {
                form_lines.push(Line::from(""));
                form_lines.push(Line::from(vec![
                    Span::styled("提示  ", secondary()),
                    Span::styled(safe(&hint), muted()),
                ]));
            }
            form_lines.push(Line::from(""));
            let mut shortcuts = vec![Span::styled("↑/↓", accent()), Span::raw(" 字段   ")];
            if state.provision_selected_field_is_editable() {
                shortcuts.extend([
                    Span::styled("←/→", accent()),
                    Span::raw(" 光标   "),
                    Span::styled("输入/Backspace", secondary()),
                    Span::raw(" 编辑   "),
                ]);
            } else {
                shortcuts.extend([Span::styled("Space", secondary()), Span::raw(" 切换   ")]);
            }
            shortcuts.extend([
                Span::styled("Enter", success()),
                Span::raw(" 生成计划   "),
                Span::styled("Esc", warning()),
                Span::raw(" 返回"),
            ]);
            form_lines.push(Line::from(shortcuts));
            if let Some(message) = &provision.message {
                form_lines.push(Line::from(Span::styled(safe(message), danger())));
            }

            let visible_height = form_area.height.saturating_sub(2) as usize;
            let scroll = selected_line.saturating_sub(visible_height.saturating_sub(3));
            frame.render_widget(
                Paragraph::new(form_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(provision_kind_style(provision.kind))
                            .title("参数"),
                    )
                    .scroll((scroll as u16, 0))
                    .wrap(Wrap { trim: false }),
                form_area,
            );

            let bar_width = layout_area.width.saturating_sub(10) as usize;
            let bar = state.provision_layout_bar(bar_width);
            let mut bar_spans = vec![Span::styled("比例 [", muted())];
            let mut run_start = 0usize;
            while run_start < bar.len() {
                let kind = bar[run_start];
                let mut run_end = run_start + 1;
                while run_end < bar.len() && bar[run_end] == kind {
                    run_end += 1;
                }
                let style = match kind {
                    ProvisionBarKind::Free => Style::default().fg(Color::DarkGray),
                    ProvisionBarKind::Plain => Style::default().fg(Color::LightBlue),
                    ProvisionBarKind::Boot => Style::default().fg(Color::Cyan),
                    ProvisionBarKind::Share => Style::default().fg(Color::Green),
                    ProvisionBarKind::Encrypt => Style::default().fg(Color::Magenta),
                    ProvisionBarKind::Compatibility => Style::default().fg(Color::Yellow),
                };
                bar_spans.push(Span::styled("━".repeat(run_end - run_start), style));
                run_start = run_end;
            }
            bar_spans.push(Span::styled("]", muted()));
            let bar_line = Line::from(bar_spans);
            let legend_line = if provision.kind == ProvisionKind::Plain {
                Line::from(vec![
                    Span::styled("■", Style::default().fg(Color::LightBlue)),
                    Span::raw(" 普通分区  "),
                    Span::styled("■", Style::default().fg(Color::DarkGray)),
                    Span::raw(" 空闲"),
                ])
            } else {
                Line::from(vec![
                    Span::styled("■", Style::default().fg(Color::Cyan)),
                    Span::raw(" 启动  "),
                    Span::styled("■", Style::default().fg(Color::Green)),
                    Span::raw(" 交换/二合一  "),
                    Span::styled("■", Style::default().fg(Color::Magenta)),
                    Span::raw(" 保密  "),
                    Span::styled("■", Style::default().fg(Color::Yellow)),
                    Span::raw(" 兼容  "),
                    Span::styled("■", Style::default().fg(Color::DarkGray)),
                    Span::raw(" 空闲"),
                ])
            };
            let raw_layout_lines = state.provision_layout_editor_lines();
            let mut layout_lines = Vec::new();
            for (index, line) in raw_layout_lines.into_iter().enumerate() {
                if index == 3 {
                    layout_lines.push(bar_line.clone());
                    layout_lines.push(legend_line.clone());
                }
                let style = if line.starts_with("✗") {
                    danger()
                } else if line.starts_with("✓") {
                    success()
                } else if line.starts_with("当前:") {
                    accent()
                } else {
                    muted()
                };
                layout_lines.push(Line::from(Span::styled(safe(&line), style)));
            }
            frame.render_widget(
                Paragraph::new(layout_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(secondary())
                            .title("实时布局"),
                    )
                    .wrap(Wrap { trim: false }),
                layout_area,
            );
        }
        ProvisionStage::Planning => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("◈  正在生成精确计划", secondary())),
                    Line::from(""),
                    Line::from(safe(
                        provision
                            .message
                            .as_deref()
                            .unwrap_or("正在只读检查目标盘…"),
                    )),
                    Line::from("此阶段不写盘；正在计算 LCE、分区边界与协议元数据。"),
                ])
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(secondary())
                        .title("只读规划"),
                ),
                main_area,
            );
        }
        ProvisionStage::Review => {
            let mut lines = vec![
                Line::from(Span::styled("计划已通过全部只读校验", success())),
                Line::from(""),
                Line::from(Span::styled(
                    provision.kind.title(),
                    provision_kind_style(provision.kind),
                )),
            ];
            if let Some(prepared) = provision.prepared.as_ref() {
                match prepared {
                    ProvisionPrepared::Plain(prepared) => {
                        let plan = &prepared.plan;
                        lines.push(Line::from(format!(
                            "目标: disk{} · 恢复普通盘 · {} 个 MBR 主分区 · {} sectors",
                            prepared.disk,
                            plan.partitions.len(),
                            plan.total_sectors
                        )));
                        lines.push(Line::from(format!(
                            "来源状态: {}   来源 LCE cleanup: {}",
                            prepared.source_kind.short_name(),
                            prepared
                                .source_lce_start_lba
                                .map(|lba| format!("LBA{lba}..{}", lba + 5))
                                .unwrap_or_else(|| "无".into())
                        )));
                        lines.push(Line::from(format!(
                            "事务触碰: {} sectors   最高写入 LBA: {}",
                            prepared.write_plan.touched_sector_count(),
                            prepared.write_plan.highest_touched_lba().unwrap_or(0)
                        )));
                        lines.push(Line::from("LBA3 已从目标盘捕获并绑定；写入前将再次复核。"));
                        for (index, part) in plan.partitions.iter().enumerate() {
                            lines.push(Line::from(format!(
                                "P{} LBA{}..{} · {} sectors · {} · 卷标:{}",
                                index + 1,
                                part.start_lba,
                                part.end_lba().unwrap_or(part.start_lba),
                                part.sector_count,
                                part.filesystem.windows_format_name(),
                                safe(&part.volume_label)
                            )));
                        }
                        for gap in &plan.gaps {
                            lines.push(Line::from(format!(
                                "空闲 LBA{}..{} · {} sectors",
                                gap.start_lba,
                                gap.end_lba(),
                                gap.sector_count
                            )));
                        }
                        lines.push(Line::from(Span::styled(
                            "将清除 EDP 协议状态并重建上述普通分区；这不是安全擦除。",
                            warning(),
                        )));
                    }
                    ProvisionPrepared::New(prepared) => {
                        lines.extend([
                            Line::from(format!(
                                "目标: disk{}  {}",
                                prepared.disk,
                                safe(&prepared.device_id)
                            )),
                            Line::from(format!(
                                "容量: {} sectors   LCE: LBA{}",
                                prepared.write_image.total_sectors, prepared.lce_start_lba
                            )),
                            Line::from(format!(
                                "事务触碰: {} sectors   最高写入 LBA: {}",
                                prepared.write_image.touched_sector_count(),
                                prepared.write_image.highest_touched_lba().unwrap_or(0)
                            )),
                            Line::from("LBA3 已从目标盘捕获并绑定；写入前将再次复核。"),
                            Line::from("先写协议/LCE 并验证，再对勾选的分区单独格式化并验证。"),
                            Line::from(format!(
                                "初始化密码强制修改: {}",
                                if prepared.force_change_password {
                                    "是"
                                } else {
                                    "否"
                                }
                            )),
                            Line::from(format!(
                                "取消密码复杂性验证: {}",
                                if prepared.pass_info_policy.cancel_password_complexity_check {
                                    "是"
                                } else {
                                    "否"
                                }
                            )),
                            Line::from(format!(
                                "交换区密码最大错误次数: {}",
                                prepared.pass_info_policy.max_share_password_errors
                            )),
                            Line::from(format!(
                                "保密区密码最大错误次数: {}",
                                prepared.pass_info_policy.max_encrypt_password_errors
                            )),
                            Line::from("制盘后格式化:"),
                        ]);
                        for choice in &prepared.format_targets {
                            let target = &choice.target;
                            lines.push(Line::from(format!(
                                "{} {} type{} {} {}{} 卷标:{}",
                                if !target.format_capable {
                                    "—"
                                } else if choice.selected {
                                    "☑"
                                } else {
                                    "☐"
                                },
                                target.role.label(),
                                target.geometry.partition_type.raw(),
                                if !target.format_capable {
                                    "不可格式化"
                                } else if target.physically_encrypted {
                                    "加密"
                                } else {
                                    "明文"
                                },
                                choice
                                    .filesystem
                                    .map(|format| format.windows_format_name())
                                    .unwrap_or("—"),
                                target
                                    .visible_mbr_type
                                    .map(|mbr| format!(" / MBR 0x{mbr:02X}"))
                                    .unwrap_or_default(),
                                if target.format_capable {
                                    choice.volume_label.as_str()
                                } else {
                                    "—"
                                }
                            )));
                        }
                        if let Some(target_plan) = &prepared.target_plan {
                            lines.push(Line::from(""));
                            lines.push(Line::from(format!(
                                "未分配空间: {} sectors",
                                target_plan.unallocated_sectors
                            )));
                            for part in &target_plan.partitions {
                                let end = part.geometry.start_lba + part.geometry.sector_count - 1;
                                let action = match part.action {
                                    crate::provision::PartitionAction::PreserveExact => {
                                        "原数据可保留 · 复用原 FileKey · 不写数据区"
                                    }
                                    crate::provision::PartitionAction::Rebuild => {
                                        "将重建 · 原数据不可原样保留"
                                    }
                                };
                                lines.push(Line::from(format!(
                                    "{} LBA{}..{} ({} sectors): {}",
                                    part.geometry.role.label(),
                                    part.geometry.start_lba,
                                    end,
                                    part.geometry.sector_count,
                                    action
                                )));
                                lines.push(Line::from(format!("  {}", part.reason)));
                            }
                            if target_plan.partitions.iter().all(|part| {
                                part.action == crate::provision::PartitionAction::Rebuild
                            }) {
                                lines.push(Line::from(vec![
                                    Span::styled("E", secondary()),
                                    Span::raw(" 导出与该目标绑定的稀疏制盘镜像"),
                                ]));
                            }
                        }
                    }
                }
            }
            if let Some(message) = &provision.message {
                lines.push(Line::from(Span::styled(safe(message), success())));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Enter", danger()),
                Span::raw(" 进入最终 YES 确认   "),
                Span::styled("Esc", warning()),
                Span::raw(" 返回修改"),
            ]));
            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(success())
                            .title("计划预览"),
                    )
                    .wrap(Wrap { trim: true }),
                main_area,
            );
        }
        ProvisionStage::ExportPath => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("导出目标绑定制盘镜像", secondary())),
                    Line::from(""),
                    Line::from("镜像包含目标盘硬件身份和原始 LBA3，不应写入另一块不同 U 盘。"),
                    Line::from(vec![
                        Span::styled("输出路径  ", muted()),
                        Span::styled(safe(&provision.export_path), selected()),
                    ]),
                    Line::from(""),
                    Line::from("直接输入编辑路径 · Backspace 删除 · Enter 开始导出 · Esc 返回"),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(secondary())
                        .title("镜像导出"),
                )
                .wrap(Wrap { trim: true }),
                main_area,
            );
        }
        ProvisionStage::Exporting => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("◈ 正在导出稀疏制盘镜像", secondary())),
                    Line::from(""),
                    Line::from(safe(
                        provision
                            .message
                            .as_deref()
                            .unwrap_or("正在写入镜像并执行 fsync…"),
                    )),
                    Line::from("导出完成前保持当前计划不变。"),
                ])
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(secondary())
                        .title("镜像导出"),
                ),
                main_area,
            );
        }
        ProvisionStage::Confirm => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("破坏性写盘最终确认", danger())),
                    Line::from(""),
                    Line::from("请重新核对目标盘和计划。此操作会修改真实物理介质。"),
                    Line::from(vec![
                        Span::raw("精确输入 "),
                        Span::styled("YES", danger()),
                        Span::raw(" 后按 Enter： "),
                        Span::styled(safe(&provision.confirmation), selected()),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled("Esc 返回计划页，不会写盘。", warning())),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(danger())
                        .title("最终确认"),
                )
                .wrap(Wrap { trim: true }),
                main_area,
            );
        }
        ProvisionStage::Running => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("◆  安全事务执行中", warning())),
                    Line::from(""),
                    Line::from(safe(
                        provision.message.as_deref().unwrap_or("正在执行事务写盘…"),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "q / Esc / Ctrl-C 不会中断介质事务；退出请求只会在安全检查点生效。",
                        danger(),
                    )),
                ])
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(warning())
                        .title("事务执行"),
                ),
                main_area,
            );
        }
        ProvisionStage::Result => {
            let has_format_failure = provision
                .message
                .as_deref()
                .is_some_and(|message| message.contains("格式化：✗"));
            let mut lines = vec![
                Line::from(Span::styled(
                    "制盘流程已到达安全结束点",
                    if has_format_failure {
                        warning()
                    } else {
                        success()
                    },
                )),
                Line::from(""),
            ];
            lines.extend(
                provision
                    .message
                    .as_deref()
                    .unwrap_or("操作结束")
                    .lines()
                    .map(|line| Line::from(safe(line))),
            );
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Enter / Esc 返回制盘中心",
                accent(),
            )));
            frame.render_widget(
                Paragraph::new(lines).alignment(Alignment::Center).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(success())
                        .title("结果"),
                ),
                main_area,
            );
        }
    }
}

fn draw_command_palette(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let commands = [
        "devices  切到设备",
        "backups  切到备份",
        "provision 制盘/免密改造",
        "inspect  打开 Inspect",
        "advanced-inspect  任意 LBA / decode / meta / 导出",
        "restore  Restore 安全向导",
        "backup-create  备份当前设备",
        "backup-verify  校验当前备份",
        "backup-delete  删除当前备份",
        "batch-delete  删除空格勾选的多份备份",
        "backup-deep    深度备份当前设备",
        "backup-prune   keep-N 清理旧备份",
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
    let Some(selected_index) = state.inspect_selected_lba() else {
        return;
    };
    let Some(item) = workspace.items.get(selected_index as usize) else {
        return;
    };
    let mode = state.inspect_mode().unwrap_or(InspectMode::Fields);
    let method = item.method.as_deref().unwrap_or("raw");
    let mut lines = vec![
        Line::from(format!("来源: {}", safe(&workspace.source))),
        Line::from(format!("LBA{} · {}", item.lba, safe(method))),
        Line::from(""),
    ];
    match mode {
        InspectMode::Fields => {
            if item.fields.is_empty() {
                lines.push(Line::from("未检测到已知结构化字段。"));
            } else {
                for field in &item.fields {
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
            for note in &item.notes {
                lines.push(Line::from(format!("注: {}", safe(note))));
            }
        }
        InspectMode::DecodedHex => lines.extend(plain_hex_lines(
            item.decoded.as_deref().unwrap_or(item.raw.as_slice()),
        )),
        InspectMode::RawHex => lines.extend(plain_hex_lines(&item.raw)),
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

fn inspect_field_status_style(status: crate::application::inspect::InspectFieldStatus) -> Style {
    match status {
        crate::application::inspect::InspectFieldStatus::Known => accent(),
        crate::application::inspect::InspectFieldStatus::Unknown => warning(),
        crate::application::inspect::InspectFieldStatus::Reserved => danger(),
        crate::application::inspect::InspectFieldStatus::Preserved => success(),
    }
}

fn draw_sector_inspector(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    use super::state::SectorInspectMode;

    let Some(sector) = state.advanced_inspect_sector() else {
        return;
    };
    let item = state.advanced_inspect_sector_item();
    let active_field = state.advanced_inspect_sector_active_field();
    let absolute = (sector.lba as u128) * crate::common::SECTOR as u128 + sector.cursor as u128;
    let decode_issue = item.and_then(|item| item.decode_error.as_deref());
    let header = vec![
        Line::from(vec![
            Span::styled(
                format!("LBA{}  ", sector.lba),
                secondary().add_modifier(Modifier::BOLD),
            ),
            Span::styled(sector.mode.label(), accent().add_modifier(Modifier::BOLD)),
            Span::raw(if sector.pending { "  · 读取中" } else { "" }),
        ]),
        Line::from(format!(
            "byte +0x{:03X} / absolute 0x{:X} / row {:02}/32",
            sector.cursor,
            absolute,
            sector.cursor / 16 + 1
        )),
        Line::from(
            sector
                .error
                .as_deref()
                .or(decode_issue)
                .map(|value| format!("decode: {}", safe(value)))
                .unwrap_or_else(|| "decode: available/raw".into()),
        ),
    ];
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(header).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Sector Inspector"),
        ),
        vertical[0],
    );

    let main = if area.width >= 100 {
        let parts = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
            .split(vertical[1]);
        (parts[0], parts[1])
    } else {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(10)])
            .split(vertical[1]);
        (parts[0], parts[1])
    };

    let mut hex_lines = Vec::new();
    if let Some(item) = item {
        let decoded = item.decoded.as_deref();
        let display = match sector.mode {
            SectorInspectMode::Raw => item.raw.as_slice(),
            SectorInspectMode::Decode | SectorInspectMode::Mixed => {
                decoded.unwrap_or(item.raw.as_slice())
            }
        };
        for row in 0..32usize {
            let offset = row * 16;
            let row_abs = (sector.lba as u128) * crate::common::SECTOR as u128 + offset as u128;
            let mut spans = vec![
                Span::styled(format!("+0x{offset:03X}  "), muted()),
                Span::styled(format!("0x{row_abs:012X}  "), muted()),
            ];
            for column in 0..16usize {
                let index = offset + column;
                let byte = display[index];
                let absolute = sector
                    .lba
                    .saturating_mul(crate::common::SECTOR as u64)
                    .saturating_add(index as u64);
                let active_status = active_field.as_ref().and_then(|field| {
                    (absolute >= field.range.start && absolute < field.range.end_exclusive)
                        .then_some(field.status)
                });
                let style = if index == sector.cursor {
                    selected()
                } else if let Some(status) = active_status {
                    inspect_field_status_style(status)
                } else if sector.mode == SectorInspectMode::Mixed
                    && decoded.is_some_and(|decoded| decoded[index] != item.raw[index])
                {
                    secondary()
                } else {
                    Style::default()
                };
                spans.push(Span::styled(format!("{byte:02X} "), style));
                if column == 7 {
                    spans.push(Span::raw(" "));
                }
            }
            let ascii = display[offset..offset + 16]
                .iter()
                .map(|byte| {
                    if (0x20..=0x7e).contains(byte) {
                        *byte as char
                    } else {
                        '.'
                    }
                })
                .collect::<String>();
            spans.push(Span::styled(format!(" |{ascii}|"), muted()));
            hex_lines.push(Line::from(spans));
        }
    } else {
        hex_lines.push(Line::from(if sector.pending {
            "正在后台读取当前扇区…"
        } else {
            "当前扇区没有可显示的数据。"
        }));
    }
    let visible_rows = main.0.height.saturating_sub(2).max(1) as usize;
    let cursor_row = sector.cursor / 16;
    let max_scroll = 32usize.saturating_sub(visible_rows);
    let scroll = cursor_row
        .saturating_sub(visible_rows / 2)
        .min(max_scroll)
        .min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(hex_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("32 × 16 Hex + ASCII"),
            )
            .scroll((scroll, 0)),
        main.0,
    );

    let mut details = Vec::new();
    if let Some(item) = item {
        let raw = item.raw.get(sector.cursor).copied().unwrap_or_default();
        let decoded = item
            .decoded
            .as_ref()
            .and_then(|bytes| bytes.get(sector.cursor))
            .copied();
        details.push(Line::from(format!("Raw byte: 0x{raw:02X} ({raw})")));
        if let Some(decoded) = decoded {
            details.push(Line::from(format!("Decoded:  0x{decoded:02X} ({decoded})")));
        }
        details.push(Line::from(""));
        if let Some(field) = active_field.as_ref() {
            details.push(Line::from(Span::styled(
                safe(&field.label),
                accent().add_modifier(Modifier::BOLD),
            )));
            details.push(Line::from(format!("Value: {}", safe(&field.value))));
            details.push(Line::from(vec![
                Span::raw(format!("Type: {:?} / Status: ", field.field_type)),
                Span::styled(
                    format!("{:?}", field.status),
                    inspect_field_status_style(field.status).add_modifier(Modifier::BOLD),
                ),
            ]));
            details.push(Line::from(format!(
                "Range: 0x{:X}..0x{:X}",
                field.range.start, field.range.end_exclusive
            )));
            if sector.field_expanded {
                details.push(Line::from(""));
                details.push(Line::from(format!(
                    "bits: b7={} b6={} b5={} b4={} b3={} b2={} b1={} b0={}",
                    (raw >> 7) & 1,
                    (raw >> 6) & 1,
                    (raw >> 5) & 1,
                    (raw >> 4) & 1,
                    (raw >> 3) & 1,
                    (raw >> 2) & 1,
                    (raw >> 1) & 1,
                    raw & 1
                )));
                for child in &field.children {
                    details.push(Line::from(format!(
                        "{} = {}",
                        safe(&child.label),
                        safe(&child.value)
                    )));
                }
            } else {
                details.push(Line::from("o 展开 bit / child 详情"));
            }
        } else {
            details.push(Line::from(Span::styled("Unknown byte", warning())));
            details.push(Line::from("当前 byte 不属于已知 Field；不推测语义。"));
            if sector.field_expanded {
                details.push(Line::from(format!("bits: {:08b}", raw)));
            }
        }
    } else if let Some(error) = sector.error.as_deref() {
        details.push(Line::from(Span::styled(safe(error), danger())));
    }
    if let Some(yank) = state.advanced_inspect_yank_register() {
        details.push(Line::from(""));
        details.push(Line::from(vec![
            Span::styled("Yank  ", muted()),
            Span::styled(safe(yank), secondary()),
        ]));
    }
    frame.render_widget(
        Paragraph::new(details)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Typed / Field"),
            )
            .wrap(Wrap { trim: false }),
        main.1,
    );

    frame.render_widget(
        Paragraph::new(Line::from(
            "←/→ byte · j/k ±16B · PgUp/PgDn sector · r/d/m mode · o bit · y value · Y raw · Esc 返回树",
        ))
        .block(Block::default().borders(Borders::TOP)),
        vertical[2],
    );
}

fn draw_advanced_inspect(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(advanced) = state.advanced_inspect() else {
        return;
    };
    use super::state::{AdvancedInspectPanel, AdvancedInspectStage};
    use crate::application::inspect_tree::InspectNodeKind;

    match advanced.stage {
        AdvancedInspectStage::Running => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        "◈ 正在建立全盘结构树",
                        secondary().add_modifier(Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from(safe(
                        advanced
                            .message
                            .as_deref()
                            .unwrap_or("正在读取协议上下文并识别磁盘区域…"),
                    )),
                    Line::from("只读任务不会修改物理盘。"),
                ])
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(secondary())
                        .title("Inspect · 全盘浏览"),
                ),
                area,
            );
        }
        AdvancedInspectStage::Browser => {
            if advanced.sector.is_some() {
                draw_sector_inspector(frame, area, state);
                return;
            }
            let Some(workspace) = advanced.result.as_ref() else {
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::from(Span::styled("全盘结构加载失败", danger())),
                        Line::from(""),
                        Line::from(safe(
                            advanced
                                .message
                                .as_deref()
                                .unwrap_or("未取得 Inspect workspace"),
                        )),
                        Line::from(""),
                        Line::from("Esc 关闭"),
                    ])
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(danger())
                            .title("Inspect · 全盘浏览"),
                    )
                    .wrap(Wrap { trim: false }),
                    area,
                );
                return;
            };

            let rows = state.advanced_inspect_tree_rows();
            let selected_index = advanced.tree_selected.min(rows.len().saturating_sub(1));
            let selected_row = rows.get(selected_index);
            let (tree_area, overview_area, detail_area) = if area.width >= 140 && area.height >= 10
            {
                let parts = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(42),
                        Constraint::Percentage(27),
                        Constraint::Percentage(31),
                    ])
                    .split(area);
                (parts[0], parts[1], parts[2])
            } else if area.width >= 92 && area.height >= 12 {
                let parts = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
                    .split(area);
                let right = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(9), Constraint::Min(4)])
                    .split(parts[1]);
                (parts[0], right[0], right[1])
            } else {
                let parts = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(50),
                        Constraint::Percentage(25),
                        Constraint::Percentage(25),
                    ])
                    .split(area);
                (parts[0], parts[1], parts[2])
            };

            let visible = visible_window(selected_index, rows.len(), tree_area.height);
            let tree_lines = visible.map(|index| {
                let row = &rows[index];
                let indent = "  ".repeat(row.depth);
                let marker = if row.expandable {
                    if row.expanded {
                        "▾ "
                    } else {
                        "▸ "
                    }
                } else {
                    "· "
                };
                let icon = match row.kind {
                    InspectNodeKind::Device => "◆ ",
                    InspectNodeKind::Region => "◇ ",
                    InspectNodeKind::Extent => "▰ ",
                    InspectNodeKind::Sector => "□ ",
                    InspectNodeKind::Structure => "▱ ",
                    InspectNodeKind::Group => "≡ ",
                    InspectNodeKind::Field => "• ",
                    InspectNodeKind::Partition => "▣ ",
                    InspectNodeKind::UnknownRange => "? ",
                };
                let kind_style = match row.kind {
                    InspectNodeKind::Device => secondary().add_modifier(Modifier::BOLD),
                    InspectNodeKind::Region | InspectNodeKind::Partition => accent(),
                    InspectNodeKind::Extent | InspectNodeKind::Structure => success(),
                    InspectNodeKind::Sector | InspectNodeKind::Field => Style::default(),
                    InspectNodeKind::Group => muted(),
                    InspectNodeKind::UnknownRange => warning(),
                };
                let text = format!(
                    "{indent}{marker}{icon}{}  [{}..{})",
                    safe(&row.label),
                    row.range.start_lba,
                    row.range.end_lba_exclusive()
                );
                if index == selected_index {
                    Line::from(Span::styled(text, selected()))
                } else {
                    Line::from(Span::styled(text, kind_style))
                }
            });

            let tree_focus = advanced.panel == AdvancedInspectPanel::Tree;
            frame.render_widget(
                Paragraph::new(tree_lines.collect::<Vec<_>>())
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(if tree_focus { accent() } else { muted() })
                            .title(format!(
                                "结构树  {}/{}",
                                selected_index.saturating_add(1),
                                rows.len()
                            )),
                    )
                    .wrap(Wrap { trim: false }),
                tree_area,
            );

            let mut overview_lines = vec![Line::from(vec![
                Span::styled("来源  ", muted()),
                Span::styled(safe(&workspace.source), accent()),
            ])];
            let mut detail_lines = Vec::new();
            if let Some(row) = selected_row {
                let kind = match row.kind {
                    InspectNodeKind::Device => "Device",
                    InspectNodeKind::Region => "Region",
                    InspectNodeKind::Extent => "Extent",
                    InspectNodeKind::Sector => "Sector",
                    InspectNodeKind::Structure => "Structure",
                    InspectNodeKind::Group => "Group",
                    InspectNodeKind::Field => "Field",
                    InspectNodeKind::Partition => "Partition",
                    InspectNodeKind::UnknownRange => "UnknownRange",
                };
                let status = match row.status {
                    crate::edpb::SemanticStatus::Identified => "identified",
                    crate::edpb::SemanticStatus::Unknown => "unknown",
                };
                overview_lines.extend([
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("节点  ", muted()),
                        Span::styled(safe(&row.label), secondary().add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(format!("类型: {kind}")),
                    Line::from(format!(
                        "LBA: {}..{}",
                        row.range.start_lba,
                        row.range.end_lba_exclusive()
                    )),
                    Line::from(format!("Sector count: {}", row.range.sector_count)),
                    Line::from(format!("状态: {status}")),
                ]);
                if let Some(byte_range) = row.range.byte_range {
                    overview_lines.push(Line::from(format!(
                        "Byte: 0x{:X}..0x{:X}",
                        byte_range.start, byte_range.end_exclusive
                    )));
                }
                if let Some(decoder) = row.decoder {
                    overview_lines.push(Line::from(format!("Decoder: {decoder:?}")));
                }

                match row.kind {
                    InspectNodeKind::Sector => {
                        let lba = row.range.start_lba;
                        if let Some(item) = workspace.items.iter().find(|item| item.lba == lba) {
                            detail_lines.push(Line::from(vec![
                                Span::styled("已缓存  ", success()),
                                Span::raw(format!("LBA{lba}")),
                            ]));
                            detail_lines.push(Line::from(format!(
                                "RAW SHA-256: {}",
                                safe(&item.raw_sha256)
                            )));
                            detail_lines.push(Line::from(
                                "Enter 打开 Sector Inspector；首次进入自动补齐 Decode。",
                            ));
                        } else {
                            detail_lines
                                .push(Line::from(Span::styled("该扇区尚未按需读取。", warning())));
                            detail_lines.push(Line::from(
                                "Enter 打开 Sector Inspector 并后台读取当前 sector。",
                            ));
                        }
                    }
                    InspectNodeKind::Field => {
                        let field = row.range.byte_range.and_then(|range| {
                            workspace
                                .items
                                .iter()
                                .flat_map(|item| item.fields.iter())
                                .find(|field| field.range == range)
                        });
                        if let Some(field) = field {
                            detail_lines.push(Line::from(Span::styled(
                                safe(&field.label),
                                accent().add_modifier(Modifier::BOLD),
                            )));
                            detail_lines.push(Line::from(format!("Value: {}", safe(&field.value))));
                            detail_lines.push(Line::from(format!(
                                "Type: {:?}   Status: {:?}",
                                field.field_type, field.status
                            )));
                            let raw = field
                                .raw
                                .iter()
                                .take(32)
                                .map(|byte| format!("{byte:02X}"))
                                .collect::<Vec<_>>()
                                .join(" ");
                            detail_lines.push(Line::from(format!("Raw: {raw}")));
                        } else {
                            detail_lines.push(Line::from("字段详情尚未 materialize。"));
                        }
                    }
                    InspectNodeKind::Group => {
                        detail_lines.push(Line::from("分页控制节点。"));
                        detail_lines.push(Line::from("Enter / o 切换当前 lazy sector 窗口。"));
                    }
                    _ => {
                        detail_lines.push(Line::from("o 展开/折叠当前节点。"));
                        detail_lines.push(Line::from("Enter 查看或进入当前节点。"));
                    }
                }
            } else {
                overview_lines.push(Line::from("当前没有可选节点。"));
                detail_lines.push(Line::from("当前没有可选节点。"));
            }
            if let Some(message) = advanced.message.as_deref() {
                detail_lines.push(Line::from(""));
                detail_lines.push(Line::from(Span::styled(safe(message), danger())));
            }

            frame.render_widget(
                Paragraph::new(overview_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(if advanced.panel == AdvancedInspectPanel::Overview {
                                accent()
                            } else {
                                muted()
                            })
                            .title("节点概览"),
                    )
                    .wrap(Wrap { trim: false }),
                overview_area,
            );

            let detail_focus = advanced.panel == AdvancedInspectPanel::Detail;
            let scroll = advanced.detail_scroll.min(u16::MAX as usize) as u16;
            frame.render_widget(
                Paragraph::new(detail_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(if detail_focus { accent() } else { muted() })
                            .title("节点详情"),
                    )
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0)),
                detail_area,
            );
        }
    }
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

fn draw_backup_batch_delete(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(batch) = state.backup_batch_delete() else {
        return;
    };
    use super::state::BackupBatchDeleteStage;

    let planned = batch
        .prepared
        .as_ref()
        .map(|plan| plan.targets.len())
        .unwrap_or_else(|| state.backup_selection_count());
    let mut lines = vec![
        Line::from(Span::styled("批量删除备份", danger())),
        Line::from(format!("当前勾选: {} 份", state.backup_selection_count())),
        Line::from("安全规则：新鲜扫描逐项固定路径 + SHA-256 → 一次性保留底线检查 → 固定 DeletePlan → 执行时逐条复核。"),
        Line::from(""),
    ];
    match batch.stage {
        BackupBatchDeleteStage::Planning => {
            lines.push(Line::from(Span::styled(
                "正在生成固定批量删除计划…",
                secondary(),
            )));
        }
        BackupBatchDeleteStage::Review => {
            lines.extend([
                Line::from(Span::styled(
                    format!("计划已固定：将删除 {planned} 份备份。"),
                    warning(),
                )),
                Line::from("Enter 进入最终 YES 确认；Esc 取消计划并保留勾选。"),
            ]);
        }
        BackupBatchDeleteStage::Confirm => {
            lines.extend([
                Line::from(Span::styled(
                    format!("不可撤销：即将删除 {planned} 份备份。"),
                    danger(),
                )),
                Line::from(vec![
                    Span::raw("精确输入 "),
                    Span::styled("YES", danger()),
                    Span::raw(" 后按 Enter： "),
                    Span::styled(safe(&batch.confirmation), selected()),
                ]),
            ]);
        }
        BackupBatchDeleteStage::Running => {
            lines.push(Line::from(Span::styled(
                "正在按固定计划逐条复核并删除；退出请求延迟到安全结束点。",
                warning(),
            )));
        }
        BackupBatchDeleteStage::Result => {
            lines.push(Line::from(Span::styled(
                safe(batch.message.as_deref().unwrap_or("批量删除流程结束")),
                success(),
            )));
            lines.push(Line::from("Enter / Esc 返回备份列表。"));
        }
    }
    if batch.stage != BackupBatchDeleteStage::Result {
        if let Some(message) = &batch.message {
            lines.push(Line::from(Span::styled(safe(message), muted())));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(danger())
                    .title("危险操作 · 批量删除")
                    .title_style(danger()),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_backup_prune(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(prune) = state.backup_prune() else {
        return;
    };
    use super::state::BackupPruneStage;

    let mut lines = vec![
        Line::from(Span::styled("备份保留策略清理", warning())),
        Line::from("按同盘组执行 keep-N；原始盘备份与保留底线由 application 层统一保护。"),
        Line::from(""),
    ];
    match prune.stage {
        BackupPruneStage::Input => {
            lines.extend([
                Line::from(vec![
                    Span::styled("每组保留最近 N 份快照: ", accent()),
                    Span::styled(safe(&prune.keep_input), selected()),
                ]),
                Line::from("仅输入正整数；Enter 生成只读清理计划，Esc 取消。"),
            ]);
        }
        BackupPruneStage::Planning => {
            lines.push(Line::from(Span::styled(
                "正在扫描备份并生成固定候选快照…",
                secondary(),
            )));
        }
        BackupPruneStage::Review => {
            if let Some(prepared) = prune.prepared.as_ref() {
                lines.extend([
                    Line::from(format!("keep-N: {}", prepared.keep)),
                    Line::from(format!("原盘备份: {} 份", prepared.originals)),
                    Line::from(format!(
                        "计划删除: {} 份   清理后快照: {} 份",
                        prepared.plan.targets.len(),
                        prepared.retained_snapshots
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Enter 进入 YES 确认；执行时逐条按固定 SHA-256 复核。",
                        warning(),
                    )),
                ]);
            }
        }
        BackupPruneStage::Confirm => {
            let count = prune
                .prepared
                .as_ref()
                .map(|prepared| prepared.plan.targets.len())
                .unwrap_or(0);
            lines.extend([
                Line::from(Span::styled(
                    format!("即将删除 {count} 份旧备份，这是不可撤销操作。"),
                    danger(),
                )),
                Line::from(vec![
                    Span::raw("精确输入 "),
                    Span::styled("YES", danger()),
                    Span::raw(" 后按 Enter： "),
                    Span::styled(safe(&prune.confirmation), selected()),
                ]),
            ]);
        }
        BackupPruneStage::Running => {
            lines.push(Line::from(Span::styled(
                "正在逐条摘要复核并删除；退出请求会延迟到安全结束点。",
                warning(),
            )));
        }
        BackupPruneStage::Result => {
            lines.push(Line::from(Span::styled(
                safe(prune.message.as_deref().unwrap_or("清理流程结束")),
                success(),
            )));
            lines.push(Line::from("Enter / Esc 返回备份列表。"));
        }
    }
    if prune.stage != BackupPruneStage::Result {
        if let Some(message) = &prune.message {
            lines.push(Line::from(Span::styled(safe(message), danger())));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(warning())
                    .title("备份清理 · keep-N")
                    .title_style(warning()),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// 把类型化写盘事件映射为向导 Running 阶段的单行显示文本。
/// 直接从事件类型映射，不经 ANSI 文本反解析；调用方负责经 `safe` 消毒。
fn write_progress_text(event: &crate::application::WriteEvent) -> String {
    use crate::application::WriteEvent;
    match event {
        WriteEvent::BackupCreated { path } => format!(
            "备份完成：{}",
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned())
        ),
        WriteEvent::BackupCreatedIsNopwd => {
            "本份备份为免密状态快照（还原不会回到加密原盘）".to_string()
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
    }
}

fn draw_wizard(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let Some(wizard) = state.wizard() else {
        return;
    };
    let operation = match wizard.kind {
        WriteKind::Restore => "Restore 备份还原",
        WriteKind::BackupCreate => "Create Backup 只读备份",
        WriteKind::BackupCreateDeep => "Deep Backup 深度备份",
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
        WriteKind::BackupCreateDeep => {
            "只读链：完整读取可验证分区/文件系统证据并写入 Deep EDPB；耗时更长，但不会卸载或写 U 盘"
        }
        WriteKind::Restore => {
            "安全链：系统盘/USB整盘检查 → selector pinning → 写前保护 → 卸载/锁卷 → reopen复核 → atomic write → sync/readback/rollback"
        }
    }));
    match wizard.stage {
        WizardStage::Confirm => {
            lines.push(Line::from(if wizard.kind == WriteKind::Restore {
                "确认后进入关键写盘阶段。请输入 YES："
            } else {
                "确认后开始只读备份。请输入 YES："
            }));
            lines.push(Line::from(format!("> {}", wizard.confirmation)));
            if let Some(message) = &wizard.message {
                lines.push(Line::from(safe(message)));
            }
        }
        WizardStage::Running => {
            lines.push(Line::from(if wizard.kind == WriteKind::Restore {
                "关键写盘阶段进行中；q / Esc / Ctrl-C 不会中断当前事务。"
            } else {
                "只读备份进行中；q / Esc / Ctrl-C 不会中断当前事务。"
            }));
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
    } else if state.advanced_inspect().is_some() {
        (CoreMode::Busy, "高级检查")
    } else if state.inspect_pending() {
        (CoreMode::Busy, "READ LBA0-12")
    } else if state.active_scan_pending() {
        (CoreMode::Busy, "BACKGROUND SCAN")
    } else if state.wizard().is_some()
        || (state.workspace() == Workspace::Provision
            && state.provision().stage != ProvisionStage::Menu)
    {
        (CoreMode::Busy, "USER FLOW")
    } else {
        (CoreMode::Stable, "INTERACTIVE")
    };
    let has_notice = state.notice().is_some();
    let mut constraints = vec![
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(4),
    ];
    if has_notice {
        constraints.push(Constraint::Length(3));
    }
    constraints.push(Constraint::Length(3));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let title = Paragraph::new(Line::from(vec![
        Span::styled("edpcli", accent()),
        Span::styled(format!(" v{}", env!("CARGO_PKG_VERSION")), muted()),
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
        Workspace::Devices | Workspace::Provision => 0,
        Workspace::Backups => 1,
    };
    let workspace_tabs = Tabs::new(["设备", "备份"])
        .select(workspace_index)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(accent())
                .title("页面 · Tab / ← / → / h / l 切换"),
        )
        .style(muted())
        .highlight_style(selected())
        .divider(Span::styled(" │ ", muted()))
        .padding("  ", "  ");
    frame.render_widget(workspace_tabs, chunks[1]);

    let body = chunks[2];
    let overlay_active = state.advanced_inspect().is_some()
        || state.inspect_data().is_some()
        || state.backup_delete().is_some()
        || state.backup_batch_delete().is_some()
        || state.backup_prune().is_some()
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

    if state.advanced_inspect().is_some() {
        draw_advanced_inspect(frame, content_area, state);
    } else if state.inspect_data().is_some() {
        draw_inspect(frame, content_area, state);
    } else if state.backup_delete().is_some() {
        draw_backup_delete(frame, content_area, state);
    } else if state.backup_batch_delete().is_some() {
        draw_backup_batch_delete(frame, content_area, state);
    } else if state.backup_prune().is_some() {
        draw_backup_prune(frame, content_area, state);
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
                    Line::from("Tab/Shift-Tab/h/l/←/→ 切换页面   ↑/↓/j/k 移动条目   gg/G 首/尾   Ctrl-d/u 半页"),
                    Line::from(vec![
                        Span::styled("/ 搜索/过滤", secondary()),
                        Span::raw("   "),
                        Span::styled("I 高级 Inspect", warning()),
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
                        Span::styled("Space 勾选 / X 批删", danger()),
                        Span::raw("   "),
                        Span::styled("R 恢复", warning()),
                        Span::raw("   "),
                        Span::styled("b 新建", accent()),
                        Span::raw("   "),
                        Span::styled("B 深度备份", secondary()),
                        Span::raw("   "),
                        Span::styled("P 清理旧备份", warning()),
                    ]),
                    Line::from(vec![
                        Span::styled("制盘: ", secondary()),
                        Span::raw(
                            "四种官方模式 + 离线快照转换；物理写盘先预览再 YES",
                        ),
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
                Workspace::Provision => draw_provision(frame, content_area, state),
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
        "备份删除正在执行：Esc 不退出；q / Ctrl-C 将延迟到安全检查点".to_string()
    } else if state.is_critical_operation() && state.backup_batch_delete().is_some() {
        "批量备份删除正在执行：Esc 不退出；q / Ctrl-C 将延迟到安全检查点".to_string()
    } else if state.is_critical_operation() && state.backup_prune().is_some() {
        "备份清理正在执行：Esc 不退出；q / Ctrl-C 将延迟到安全检查点".to_string()
    } else if state.is_critical_operation() {
        "关键写盘阶段：Esc 不退出；q / Ctrl-C 将延迟到安全检查点".to_string()
    } else if let Some(advanced) = state.advanced_inspect() {
        use super::state::AdvancedInspectStage;
        match advanced.stage {
            AdvancedInspectStage::Running => "全盘检查后台只读建立结构树…".to_string(),
            AdvancedInspectStage::Browser => {
                "全盘检查：j/k 移动 · o 展开 · Enter 查看 · Tab 面板 · Esc 返回".to_string()
            }
        }
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
        format!(
            ":{}  ·  Enter 执行  ·  Backspace 删除  ·  Esc 取消",
            safe(state.input_buffer())
        )
    } else if state.input_mode() == InputMode::Help {
        "Esc 返回  ·  q 退出".to_string()
    } else if state.wizard().is_some() {
        match state.wizard().unwrap().stage {
            WizardStage::Confirm => {
                "输入 YES  ·  Backspace 删除  ·  Enter 执行  ·  Esc 返回".to_string()
            }
            WizardStage::Running => "q / Ctrl-C 延迟退出".to_string(),
            WizardStage::Result => "Enter / Esc 关闭".to_string(),
        }
    } else if let Some(delete) = state.backup_delete() {
        match delete.stage {
            WizardStage::Confirm => "输入 YES · Backspace 删除 · Enter 删除 · Esc 取消".to_string(),
            WizardStage::Running => "q / Ctrl-C 延迟退出".to_string(),
            WizardStage::Result => "Enter / Esc 关闭".to_string(),
        }
    } else if let Some(batch) = state.backup_batch_delete() {
        use super::state::BackupBatchDeleteStage;
        match batch.stage {
            BackupBatchDeleteStage::Planning => "正在生成删除计划…".to_string(),
            BackupBatchDeleteStage::Review => "Enter 确认 · Esc 取消".to_string(),
            BackupBatchDeleteStage::Confirm => {
                "输入 YES · Backspace 删除 · Enter 执行 · Esc 返回".to_string()
            }
            BackupBatchDeleteStage::Running => "q / Ctrl-C 延迟退出".to_string(),
            BackupBatchDeleteStage::Result => "Enter / Esc 关闭".to_string(),
        }
    } else if let Some(prune) = state.backup_prune() {
        use super::state::BackupPruneStage;
        match prune.stage {
            BackupPruneStage::Input => {
                "输入保留份数 · Backspace 删除 · Enter 预览 · Esc 取消".to_string()
            }
            BackupPruneStage::Planning => "正在生成清理计划…".to_string(),
            BackupPruneStage::Review => "Enter 确认 · Esc 取消".to_string(),
            BackupPruneStage::Confirm => {
                "输入 YES · Backspace 删除 · Enter 执行 · Esc 返回".to_string()
            }
            BackupPruneStage::Running => "q / Ctrl-C 延迟退出".to_string(),
            BackupPruneStage::Result => "Enter / Esc 关闭".to_string(),
        }
    } else if state.inspect_data().is_some() {
        "↑/↓/j/k LBA  ·  ←/→ 视图  ·  / 搜索  ·  Esc 返回".to_string()
    } else {
        match state.workspace() {
            Workspace::Devices => {
                if state.selected_device().is_some() {
                    "Tab/Shift-Tab/←/→ 页面  ·  ↑/↓/j/k 移动  ·  Enter 制盘  ·  i/I 检查  ·  b/B 备份  ·  r 刷新  ·  q 退出".to_string()
                } else {
                    "Tab/Shift-Tab/←/→ 页面  ·  r 刷新  ·  q 退出".to_string()
                }
            }
            Workspace::Backups => {
                if state.selected_backup().is_some() {
                    "Tab/Shift-Tab/←/→ 页面 · ↑/↓/j/k 移动 · Space 勾选 · X 批删 · i/I 检查 · v 校验 · R 恢复 · D 单删 · q 退出".to_string()
                } else {
                    "Tab/Shift-Tab/←/→ 页面 · r 刷新 · q 退出".to_string()
                }
            }
            Workspace::Provision => match state.provision().stage {
                ProvisionStage::SelectDisk => "制盘选盘：↑/↓/j/k 选择 USB 盘  ·  Tab/Shift-Tab/h/l/←/→ 切页面  ·  Enter 固定目标  ·  Esc 返回设备页".to_string(),
                ProvisionStage::BackupPrompt => {
                    "制盘前保存：↑/↓/j/k 选择  ·  Tab/Shift-Tab/h/l/←/→ 切页面  ·  Enter 确认  ·  Esc 返回设备".to_string()
                }
                ProvisionStage::BackupSaving => "正在保存当前盘…".to_string(),
                ProvisionStage::Menu => {
                    "Tab/Shift-Tab/h/l/←/→ 页面  ·  ↑/↓/j/k 选择方案  ·  Enter 打开  ·  r 刷新目标  ·  :provision 直达  ·  ? 帮助  ·  q 退出".to_string()
                }
                ProvisionStage::Form if state.provision_selected_field_is_editable() => {
                    let unit_key = state
                        .provision_field_hint(state.provision().field_selected)
                        .is_some_and(|hint| hint.starts_with("Space 切换 MiB / GiB / sector"))
                        .then_some(" · Space 单位 · f 填满")
                        .unwrap_or("");
                    format!("↑/↓ 字段 · ←/→ 光标 · Home/End 首尾 · 输入/Backspace 编辑{unit_key} · Tab/Shift-Tab 页面 · Enter 预览 · Esc 返回")
                }
                ProvisionStage::Form => "↑/↓ 字段 · Space 切换 · Tab/Shift-Tab/←/→ 页面 · Enter 预览 · Esc 返回".to_string(),
                ProvisionStage::Planning => "正在生成只读计划…".to_string(),
                ProvisionStage::Review => {
                    "Enter 最终确认  ·  E 导出镜像（新盘计划）  ·  Esc 返回修改".to_string()
                }
                ProvisionStage::ExportPath => "输入导出路径  ·  Enter 导出  ·  Esc 返回计划".to_string(),
                ProvisionStage::Exporting => "镜像正在后台导出…".to_string(),
                ProvisionStage::Confirm => "输入 YES + Enter 执行  ·  Esc 返回计划".to_string(),
                ProvisionStage::Running => "安全事务执行中；Esc 不退出，q / Ctrl-C 的退出请求延迟到安全检查点".to_string(),
                ProvisionStage::Result => "Enter / Esc 返回制盘中心".to_string(),
            },
        }
    };
    frame.render_widget(
        Paragraph::new(safe(&status)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(accent()),
        ),
        chunks[usize::from(has_notice) + 3],
    );
    if let Some(message) = state.notice() {
        frame.render_widget(
            Paragraph::new(safe(message)).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(warning())
                    .title("提示"),
            ),
            chunks[3],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{hard_wrap_value, input_value_window, visible_window, wrapped_field_lines};

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
    fn active_input_window_does_not_pad_selected_background_to_cell_width() {
        assert_eq!(input_value_window("abc", 3, 12, false), "abc│");
        assert_eq!(input_value_window("secret", 6, 12, true), "••••••│");
    }

    #[test]
    fn large_tables_only_build_the_rows_visible_in_the_viewport() {
        let window = visible_window(50_000, 100_000, 24);
        assert!(window.contains(&50_000));
        assert_eq!(window.len(), 21);
    }
}
