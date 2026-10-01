use ratatui::{
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::{
    keymap::{
        HelpBinding, BACKUPS_HELP, DEVICES_HELP, GLOBAL_HELP, INSPECT_HELP, PICKER_HELP,
        PROVISION_HELP, TABLE_HELP,
    },
    state::{AppState, Workspace},
    theme, ui,
};

fn context_bindings(state: &AppState) -> (&'static str, &'static [HelpBinding]) {
    if state.provision_scheme_picker_open() {
        return ("选择制盘方案", PICKER_HELP);
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
    let include_table =
        state.active_table_kind().is_some() && !state.provision_scheme_picker_open();
    let line_count =
        context.len() + GLOBAL_HELP.len() + if include_table { TABLE_HELP.len() } else { 0 };
    let height = (line_count + if include_table { 8 } else { 6 }) as u16;
    let popup = ui::centered_modal_rect(frame.area(), 92, height.min(30));
    ui::render_modal(
        frame,
        popup,
        &format!("快捷键 · {context_title}"),
        |frame, inner| {
            let mut lines = Vec::new();
            push_section(&mut lines, "当前上下文", context);
            if include_table {
                push_section(&mut lines, "表格", TABLE_HELP);
            }
            push_section(&mut lines, "全局", GLOBAL_HELP);
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        },
    );
}
