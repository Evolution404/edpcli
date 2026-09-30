use ratatui::{
    style::Modifier,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use crate::tui::theme;

use super::{centered_modal_rect, render_modal};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationTone {
    Neutral,
    Warning,
    Destructive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaWriteConfirmationKind {
    Provision,
    Restore,
    Format,
    EncryptedFormat,
    Reinitialize,
}

pub struct WriteConfirmationSpec<'a> {
    pub kind: MediaWriteConfirmationKind,
    pub title: &'a str,
    pub warning: String,
    pub details: Vec<Line<'a>>,
    pub confirmation: &'a str,
    pub message: Option<&'a str>,
}

pub struct ActionConfirmationSpec<'a> {
    pub title: &'a str,
    pub headline: &'a str,
    pub details: Vec<Line<'a>>,
    pub tone: ConfirmationTone,
}

pub fn render_write_confirmation_modal(frame: &mut Frame, spec: WriteConfirmationSpec<'_>) {
    let height = (10 + spec.details.len() as u16 + u16::from(spec.message.is_some())).min(28);
    let area = centered_modal_rect(frame.area(), 82, height);
    render_modal(frame, area, spec.title, |frame, inner| {
        let theme = theme::current();
        let action_label = match spec.kind {
            MediaWriteConfirmationKind::Provision => "开始制盘",
            MediaWriteConfirmationKind::Restore => "开始恢复",
            MediaWriteConfirmationKind::Format => "开始格式化",
            MediaWriteConfirmationKind::EncryptedFormat => "开始加密格式化",
            MediaWriteConfirmationKind::Reinitialize => "开始重建",
        };
        let mut lines = spec.details;
        lines.extend([
            Line::from(""),
            Line::from(Span::styled(
                format!("⚠ {}", spec.warning),
                theme.danger().add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "输入 YES 确认写入",
                theme.warning().add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("> {}", spec.confirmation),
                theme.input_focused(),
            )),
        ]);
        if let Some(message) = spec.message {
            lines.push(Line::from(Span::styled(message, theme.danger())));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Enter", theme.accent().add_modifier(Modifier::BOLD)),
            Span::raw(format!(" {action_label}    ")),
            Span::styled("Esc", theme.muted()),
            Span::raw(" 取消"),
        ]));
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    });
}

pub fn render_action_confirmation_modal(frame: &mut Frame, spec: ActionConfirmationSpec<'_>) {
    let area = centered_modal_rect(frame.area(), 76, (8 + spec.details.len() as u16).min(22));
    render_modal(frame, area, spec.title, |frame, inner| {
        let theme = theme::current();
        let headline_style = match spec.tone {
            ConfirmationTone::Neutral => theme.secondary_text().add_modifier(Modifier::BOLD),
            ConfirmationTone::Warning => theme.warning().add_modifier(Modifier::BOLD),
            ConfirmationTone::Destructive => theme.danger().add_modifier(Modifier::BOLD),
        };
        let mut lines = vec![Line::from(Span::styled(spec.headline, headline_style))];
        if !spec.details.is_empty() {
            lines.push(Line::from(""));
            lines.extend(spec.details);
        }
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Enter", theme.accent().add_modifier(Modifier::BOLD)),
            Span::raw(" 确认    "),
            Span::styled("Esc", theme.muted()),
            Span::raw(" 取消"),
        ]));
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    });
}
