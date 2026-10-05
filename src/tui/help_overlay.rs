use ratatui::{
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::{
    help_context,
    keymap::{HelpBinding, TABLE_HELP},
    state::AppState,
    theme, ui,
};

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
    let help = help_context::resolve(state);
    let line_count = help.bindings.len()
        + help.global.len()
        + if help.include_table {
            TABLE_HELP.len()
        } else {
            0
        };
    let height = (line_count + if help.include_table { 8 } else { 6 }) as u16;
    let popup = ui::centered_modal_rect(frame.area(), 92, height.min(30));
    ui::render_modal(
        frame,
        popup,
        &format!("快捷键 · {}", help.title),
        |frame, inner| {
            let mut lines = Vec::new();
            if !help.bindings.is_empty() {
                push_section(&mut lines, "当前上下文", help.bindings);
            }
            if help.include_table {
                push_section(&mut lines, "表格", TABLE_HELP);
            }
            push_section(&mut lines, help.global_title, help.global);
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        },
    );
}
