use ratatui::{
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::{
    keymap::{
        HelpBinding, BACKUPS_HELP, DEVICES_HELP, GLOBAL_HELP, GUARD_GLOBAL_HELP, INSPECT_HELP,
        PICKER_HELP, PROVISION_HELP, PROVISION_RESULT_HELP, PROVISION_REVIEW_HELP,
        PROVISION_RUNNING_HELP, RESTORE_RESULT_HELP, SECTOR_INSPECT_HELP, TABLE_HELP,
    },
    state::{AppState, Workspace},
    theme, ui,
};

fn context_bindings(state: &AppState) -> (&'static str, &'static [HelpBinding]) {
    if state.workspace() == Workspace::Provision
        && state.provision().stage == super::state::ProvisionStage::Running
    {
        return ("制盘执行", PROVISION_RUNNING_HELP);
    }
    if state.is_critical_operation() {
        return ("关键操作执行中", &[]);
    }
    if state.provision_scheme_picker_open() {
        return ("选择制盘方案", PICKER_HELP);
    }
    if state
        .wizard()
        .is_some_and(|wizard| wizard.stage == super::state::WizardStage::PostRestore)
    {
        return ("恢复结果", RESTORE_RESULT_HELP);
    }
    if state.workspace() == Workspace::Provision
        && state.provision().stage == super::state::ProvisionStage::Review
    {
        return ("计划确认", PROVISION_REVIEW_HELP);
    }
    if state.workspace() == Workspace::Provision
        && state.provision().stage == super::state::ProvisionStage::Result
    {
        return ("制盘结果", PROVISION_RESULT_HELP);
    }
    if state.workspace() == Workspace::Inspect
        && state.advanced_inspect().is_some_and(|advanced| {
            advanced.view_mode == super::state::InspectViewMode::Hex && advanced.sector.is_some()
        })
    {
        return ("Hex 详情", SECTOR_INSPECT_HELP);
    }
    match state.workspace() {
        Workspace::Devices => ("设备", DEVICES_HELP),
        Workspace::Backups => ("备份", BACKUPS_HELP),
        Workspace::Inspect => ("检查", INSPECT_HELP),
        Workspace::Provision => ("制盘", PROVISION_HELP),
    }
}

fn binding_line(binding: &HelpBinding) -> Line<'static> {
    let theme = theme::current();
    Line::from(vec![
        Span::styled(format!("{:<22}", binding.keys), theme.accent()),
        Span::styled(binding.label, theme.text()),
    ])
}

fn push_section(lines: &mut Vec<Line<'static>>, title: &str, bindings: &[HelpBinding]) {
    let theme = theme::current();
    if !lines.is_empty() {
        lines.push(Line::default());
    }
    lines.push(Line::from(Span::styled(
        title.to_string(),
        theme.secondary_accent(),
    )));
    lines.extend(bindings.iter().map(binding_line));
}

pub(super) fn draw_help_overlay(frame: &mut Frame, state: &AppState) {
    let (context_title, context) = context_bindings(state);
    let sector_hex = state.workspace() == Workspace::Inspect
        && state.advanced_inspect().is_some_and(|advanced| {
            advanced.view_mode == super::state::InspectViewMode::Hex && advanced.sector.is_some()
        });
    let include_table =
        state.active_table_kind().is_some() && !state.provision_scheme_picker_open() && !sector_hex;
    let global = if state.is_critical_operation() {
        GUARD_GLOBAL_HELP
    } else {
        GLOBAL_HELP
    };
    let line_count =
        context.len() + global.len() + if include_table { TABLE_HELP.len() } else { 0 };
    let height = (line_count + if include_table { 8 } else { 6 }) as u16;
    let popup = ui::centered_modal_rect(frame.area(), 92, height.min(30));
    ui::render_modal(
        frame,
        popup,
        &format!("快捷键 · {context_title}"),
        |frame, inner| {
            let mut lines = Vec::new();
            if !context.is_empty() {
                push_section(&mut lines, "当前上下文", context);
            }
            if include_table {
                push_section(&mut lines, "表格", TABLE_HELP);
            }
            push_section(
                &mut lines,
                if state.is_critical_operation() {
                    "安全操作"
                } else {
                    "全局"
                },
                global,
            );
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        },
    );
}
