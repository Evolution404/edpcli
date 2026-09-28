//! Ratatui rendering for the top-level shell.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row as TableRow, Table, TableState, Tabs, Wrap},
    Frame,
};

use super::animation::CoreMode;
use super::state::{
    AppState, InputMode, ProvisionKind, ProvisionStage, WizardStage, Workspace, WriteKind,
};

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
use inspect_render::draw_advanced_inspect;
use provision_render::draw_provision;

fn backup_health(backup: &crate::application::BackupWorkspaceItem) -> (&'static str, Style) {
    if !backup.size_ok {
        ("大小异常", danger())
    } else {
        match backup.integrity_status {
            crate::application::BackupIntegrityStatus::Verified => ("EDPB ✓", success()),
            crate::application::BackupIntegrityStatus::Invalid => ("EDPB ✗", danger()),
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
    if width == 1 {
        return "│".into();
    }

    let window_width = |start: usize, end: usize| {
        let content = chars[start..end]
            .iter()
            .map(|ch| crate::ui::disp_width(&ch.to_string()).max(1))
            .sum::<usize>();
        1 + content + usize::from(start > 0) + usize::from(end < chars.len())
    };

    let mut start = cursor;
    let mut end = cursor;
    loop {
        let mut progressed = false;
        if start > 0 && window_width(start - 1, end) <= width {
            start -= 1;
            progressed = true;
        }
        if end < chars.len() && window_width(start, end + 1) <= width {
            end += 1;
            progressed = true;
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

fn device_status_style(row: &crate::disk_scan::Row) -> Style {
    if row.proto != "USB" || row.denied {
        warning()
    } else if row.probe_error.is_some() {
        danger()
    } else {
        match row.confirmed_provision_kind() {
            Some(crate::provision::DiskProvisionKind::Plain) => muted(),
            Some(_) => accent(),
            None => warning(),
        }
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
        "可用".into()
    }
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
        "provision 制盘",
        "inspect  全盘结构树 / Sector Inspector",
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
            for event in wizard.progress_log.iter().rev().take(6).rev() {
                lines.push(Line::from(safe(&write_progress_text(event))));
            }
        }
        WizardStage::Result => {
            lines.push(Line::from("操作已到达安全结束点；Esc 返回。"));
            if let Some(message) = &wizard.message {
                lines.push(Line::from(safe(message)));
            }
            for event in wizard.progress_log.iter().rev().take(6).rev() {
                lines.push(Line::from(safe(&write_progress_text(event))));
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
    let core_mode = if state.is_critical_operation() {
        CoreMode::Guard
    } else if (state.workspace() == Workspace::Inspect && state.advanced_inspect().is_some())
        || state.active_scan_pending()
        || state.wizard().is_some()
        || (state.workspace() == Workspace::Provision
            && state.provision().stage != ProvisionStage::Menu)
    {
        CoreMode::Busy
    } else {
        CoreMode::Stable
    };
    let has_notice = state.notice().is_some();
    let mut constraints = vec![
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ];
    if has_notice {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    super::shell::header(frame, chunks[0], state, core_mode);
    super::shell::navigation(frame, chunks[1], state);

    let content_area = chunks[2];

    if state.workspace() == Workspace::Inspect && state.advanced_inspect().is_some() {
        draw_advanced_inspect(frame, content_area, state);
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
                let mut help_lines = vec![Line::from(Span::styled("Vim 键位", accent()))];
                help_lines.extend(
                    super::keymap::NORMAL_HELP
                        .iter()
                        .map(|binding| Line::from(format!("{}  {}", binding.keys, binding.label))),
                );
                help_lines.push(Line::from(
                    "顶层标签：设备 ↔ 备份 · 一级 Tab/Shift-Tab 或 gt/gT 切换 · 二级 Tab/Shift-Tab 切当前页焦点 · Esc 返回上一层",
                ));
                help_lines.push(Line::from(
                    "设备: Enter 从列表进入信息树/从树进入详情 · Ctrl-w 切 Pane · 树内 j/k 选择、gg/G 首尾、o 展开 · 详情内 j/k 滚动、gg/G 顶底 · p 制盘 · i 检查 · b 备份 · 备份页: Enter/i 检查 · v 校验 · R 恢复 · d 删除",
                ));
                help_lines.push(Line::from(
                    "检查: / 搜索 · n/N 匹配 · gl 跳转 · Sector 0/$、gg/G、v",
                ));
                help_lines.push(Line::from(
                    "制盘: Normal 下 i 编辑、Enter 生成计划；Insert 下 Tab/Shift-Tab 完成编辑并移焦点，Enter/Esc 完成编辑；物理写盘保持精确输入 YES 的安全确认",
                ));
                let help = Paragraph::new(help_lines)
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
                Workspace::Inspect => frame.render_widget(
                    Paragraph::new("检查：请在设备或备份页选定对象后按 i 进入。")
                        .block(super::ui::panel("检查", true)),
                    content_area,
                ),
                Workspace::Backups => draw_backups(frame, content_area, state),
                Workspace::Provision => draw_provision(frame, content_area, state),
            },
        }
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
                let escape = state
                    .advanced_inspect_breadcrumb()
                    .map(|model| model.escape_hint())
                    .unwrap_or_else(|| "Esc 返回".into());
                if let Some((query, index, total)) = state.advanced_inspect_search_status() {
                    format!(
                        "检查：1/2/3/4 业务/原始/Hex/布局 · Tab/Shift-Tab 切 Pane · Ctrl-w h/j/k/l Pane · j/k 当前 Pane · o 展开/折叠 · Enter 查看 · {escape} · q 退出 · 当前 {index}/{total}: {}",
                        safe(query)
                    )
                } else {
                    format!("检查：1/2/3/4 业务/原始/Hex/布局 · Tab/Shift-Tab 切 Pane · Ctrl-w h/j/k/l Pane · j/k 当前 Pane · o 展开/折叠 · Enter 查看 · {escape} · q 退出")
                }
            }
        }
    } else if state.input_mode() == InputMode::Search {
        format!(
            "/{}  ·  输入即过滤  ·  Enter 确认  ·  Esc 取消编辑",
            safe(state.input_buffer())
        )
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
    } else {
        match state.workspace() {
            Workspace::Devices => "? 帮助".to_string(),
            Workspace::Inspect => {
                if state.active_table_kind()
                    == Some(crate::tui::table_layout::TableKind::InspectFields)
                {
                    let kind = crate::tui::table_layout::TableKind::InspectFields;
                    let total = crate::tui::table_layout::layout_for(kind).specs().len();
                    format!(
                        "检查字段表：Tab/Shift-Tab 切 Pane · j/k 行 · h/l 激活 · </> 移列 · 0/$ 首尾列 · H/L 视口 · s 排序 · S 默认排序 · {}/{} 列 · o 展开/折叠 · Enter 查看 · Esc 返回 · q 退出",
                        state.table_active_column(kind) + 1,
                        total
                    )
                } else {
                    "检查：Tab/Shift-Tab 切 Pane · j/k 当前 Pane · Ctrl-w 切 Pane · o 展开/折叠 · Enter 查看 · Esc 返回 · q 退出"
                        .to_string()
                }
            }
            Workspace::Backups => {
                if state.selected_backup().is_some() {
                    let kind = crate::tui::table_layout::TableKind::Backups;
                    let total = crate::tui::table_layout::layout_for(kind).specs().len();
                    format!(
                        "Tab/Shift-Tab 或 gt/gT 标签 · j/k 行 · h/l 激活 · </> 移列 · 0/$ 首尾列 · H/L 视口 · s 排序 · S 默认排序 · {}/{} 列 · Space 勾选 · Enter/i 检查 · b 新建 · v 校验 · R 恢复 · d 删除 · Esc 当前标签 · q 退出",
                        state.table_active_column(kind) + 1,
                        total
                    )
                } else {
                    "Tab/Shift-Tab 或 gt/gT 标签 · b 新建 · r 刷新 · Esc 当前标签 · q 退出"
                        .to_string()
                }
            }
            Workspace::Provision => {
                let provision_status = match state.provision().stage {
                ProvisionStage::SelectDisk => {
                    let kind = crate::tui::table_layout::TableKind::ProvisionDevices;
                    let total = crate::tui::table_layout::layout_for(kind).specs().len();
                    format!(
                        "制盘选盘：j/k 行 · h/l 激活 · </> 移列 · 0/$ 首尾列 · H/L 视口 · s 排序 · S 默认排序 · {}/{} 列 · Enter 固定目标 · Esc 返回设备页",
                        state.table_active_column(kind) + 1,
                        total
                    )
                }
                ProvisionStage::Menu => {
                    let kind = crate::tui::table_layout::TableKind::ProvisionMenu;
                    let total = crate::tui::table_layout::layout_for(kind).specs().len();
                    format!(
                        "j/k 行 · h/l 激活 · </> 移列 · 0/$ 首尾列 · H/L 视口 · s 排序 · S 默认排序 · {}/{} 列 · Enter 打开 · Esc 返回 · q 退出",
                        state.table_active_column(kind) + 1,
                        total
                    )
                }
                ProvisionStage::Form if state.input_mode() == InputMode::Insert => {
                    "INSERT · ←/→ 光标 · Home/End 首尾 · 输入/Backspace 编辑 · Tab/Shift-Tab 完成并移焦点 · Enter/Esc 完成编辑"
                        .to_string()
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
                    format!("NORMAL · Tab/Shift-Tab 字段/布局焦点 · j/k 字段 · i 编辑{unit_key} · Enter 生成计划 · Esc 返回")
                }
                ProvisionStage::Form => {
                    "NORMAL · Tab/Shift-Tab 字段/布局焦点 · j/k 字段 · i 编辑 · h/l 或 Space 切换 · Enter 生成计划 · Esc 返回"
                        .to_string()
                }
                ProvisionStage::Planning => "正在生成只读计划…".to_string(),
                ProvisionStage::Review => {
                    "Tab/Shift-Tab 切 Pane  ·  Enter 最终确认  ·  e 导出镜像  ·  Esc 返回修改".to_string()
                }
                ProvisionStage::ExportPath => {
                    "输入导出路径  ·  Enter 导出  ·  Esc 返回计划".to_string()
                }
                ProvisionStage::Exporting => "镜像正在后台导出…".to_string(),
                ProvisionStage::Confirm => "输入 YES + Enter 执行  ·  Esc 返回计划".to_string(),
                ProvisionStage::Running => {
                    "安全事务执行中；Esc 不退出，q / Ctrl-C 的退出请求延迟到安全检查点".to_string()
                }
                ProvisionStage::Result => "Enter / Esc 返回制盘中心".to_string(),
                };
                provision_status
            }
        }
    };
    let status = if state.input_mode() == InputMode::Normal
        && state.active_table_kind().is_some()
        && state.workspace() != Workspace::Devices
        && !state.is_critical_operation()
    {
        format!("y 单元格 · Y 整行 · {status}")
    } else {
        status
    };
    super::shell::footer(frame, chunks[usize::from(has_notice) + 3], &status);
    if let Some(message) = state.notice() {
        frame.render_widget(
            super::ui::notice_banner(safe(message), super::ui::BannerTone::Warning),
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
        assert_eq!(
            input_value_window("1486288249", 10, 12, false),
            "1486288249│"
        );
        assert_eq!(
            crate::ui::disp_width(&input_value_window("1486288249", 10, 10, false)),
            10
        );
    }

    #[test]
    fn large_tables_only_build_the_rows_visible_in_the_viewport() {
        let window = visible_window(50_000, 100_000, 24);
        assert!(window.contains(&50_000));
        assert_eq!(window.len(), 21);
    }
}
