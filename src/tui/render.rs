//! Ratatui rendering for the top-level shell.

use std::collections::HashMap;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row as TableRow, Table, TableState, Tabs, Wrap},
    Frame,
};

use super::state::{
    AppState, InputMode, InspectMode, ProvisionBarKind, ProvisionKind, ProvisionPrepared,
    ProvisionStage, WizardStage, Workspace, WriteKind,
};
use super::{animation, animation::CoreMode};

#[path = "backups/render.rs"]
mod backups_render;
#[path = "devices/render.rs"]
mod devices_render;
#[path = "inspect/render.rs"]
mod inspect_render;
#[path = "provision/render.rs"]
mod provision_render;

use backups_render::{
    draw_backup_batch_delete, draw_backup_create_choice, draw_backup_delete, draw_backup_prune,
    draw_backups, write_progress_text,
};
use devices_render::draw_devices;
use inspect_render::{draw_advanced_inspect, draw_inspect};
use provision_render::draw_provision;

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
    super::theme::current().accent()
}

fn secondary() -> Style {
    super::theme::current().secondary_accent()
}

fn success() -> Style {
    super::theme::current().success()
}

fn warning() -> Style {
    super::theme::current().warning()
}

fn danger() -> Style {
    super::theme::current().danger()
}

fn muted() -> Style {
    super::theme::current().muted()
}

fn selected() -> Style {
    super::theme::current().selection()
}

fn selection_marker() -> Style {
    super::theme::current().selection_marker()
}

fn panel() -> Style {
    super::theme::current().panel()
}

fn focused_panel() -> Style {
    super::theme::current().focused_panel()
}

fn tab() -> Style {
    super::theme::current().tab()
}

fn active_tab() -> Style {
    super::theme::current().active_tab()
}

fn input() -> Style {
    super::theme::current().input()
}

fn input_focused() -> Style {
    super::theme::current().input_focused()
}

fn provision_kind_style(kind: ProvisionKind) -> Style {
    super::theme::current().provision_kind(kind)
}

fn partition_style(kind: ProvisionBarKind) -> Style {
    super::theme::current().partition(kind)
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
    frame.render_widget(
        Block::default().style(super::theme::current().background()),
        area,
    );
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
    .block(Block::default().borders(Borders::ALL).border_style(panel()));
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
                .border_style(focused_panel())
                .title("工作区"),
        )
        .style(tab())
        .highlight_style(active_tab())
        .divider(Span::styled(" │ ", muted()))
        .padding("  ", "  ");
    frame.render_widget(workspace_tabs, chunks[1]);

    let body = chunks[2];
    let overlay_active = state.advanced_inspect().is_some()
        || state.inspect_data().is_some()
        || state.backup_delete().is_some()
        || state.backup_batch_delete().is_some()
        || state.backup_create_choice().is_some()
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
    } else if state.backup_create_choice().is_some() {
        draw_backup_create_choice(frame, content_area, state);
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
                            "四种官方模式 + Plain 普通盘；物理写盘先预览再 YES",
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
                    let unit_key = if state
                        .provision_field_hint(state.provision().field_selected)
                        .is_some_and(|hint| hint.starts_with("Space 切换 MiB / GiB / sector"))
                    {
                        " · Space 单位 · f 填满"
                    } else {
                        ""
                    };
                    format!("↑/↓ 字段 · ←/→ 光标 · Home/End 首尾 · 输入/Backspace 编辑{unit_key} · Tab/Shift-Tab 页面 · Enter 预览 · Esc 返回")
                }
                ProvisionStage::Form => "↑/↓ 字段 · Space 切换 · Tab/Shift-Tab/←/→ 页面 · Enter 预览 · Esc 返回".to_string(),
                ProvisionStage::Planning => "正在生成只读计划…".to_string(),
                ProvisionStage::Review => {
                    "Enter 最终确认  ·  E 导出镜像  ·  Esc 返回修改".to_string()
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
        Paragraph::new(safe(&status))
            .block(Block::default().borders(Borders::ALL).border_style(panel())),
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
