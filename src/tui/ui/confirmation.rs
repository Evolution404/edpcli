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
    pub target: String,
    pub detail_scroll: usize,
    pub details: Vec<Line<'a>>,
    pub confirmation: &'a str,
    pub message: Option<&'a super::UiMessage>,
}

pub struct ActionConfirmationSpec<'a> {
    pub title: &'a str,
    pub headline: &'a str,
    pub details: Vec<Line<'a>>,
    pub tone: ConfirmationTone,
}

/// Shared geometry gate used by both rendering and every write-intent constructor.
/// Even a previously entered YES cannot authorize a write after a terminal shrink.
pub fn write_confirmation_fits(size: ratatui::layout::Size) -> bool {
    size.width >= 40 && size.height >= 18
}

pub fn render_write_confirmation_modal(frame: &mut Frame, spec: WriteConfirmationSpec<'_>) {
    use ratatui::layout::Rect;
    let area = centered_modal_rect(frame.area(), 82, 28);
    render_modal(frame, area, spec.title, |frame, inner| {
        let theme = theme::current();
        if !write_confirmation_fits(frame.area().as_size()) {
            frame.render_widget(
                Paragraph::new("窗口过小，禁止写入。请扩大至 40×18。Esc 取消")
                    .style(theme.warning())
                    .wrap(Wrap { trim: false }),
                inner,
            );
            return;
        }
        let action_label = match spec.kind {
            MediaWriteConfirmationKind::Provision => "确认写入",
            MediaWriteConfirmationKind::Restore => "开始恢复",
            MediaWriteConfirmationKind::Format => "开始格式化",
            MediaWriteConfirmationKind::EncryptedFormat => "加密格式化",
            MediaWriteConfirmationKind::Reinitialize => "开始重建",
        };
        // The target and authorization controls never share the detail viewport.
        let header = Rect::new(inner.x, inner.y, inner.width, 2);
        let footer = Rect::new(inner.x, inner.bottom().saturating_sub(8), inner.width, 8);
        let body = Rect::new(
            inner.x,
            header.bottom(),
            inner.width,
            footer.y.saturating_sub(header.bottom()),
        );
        frame.render_widget(
            Paragraph::new(format!("目标  {}", spec.target))
                .style(theme.secondary_text())
                .wrap(Wrap { trim: false }),
            header,
        );
        let mut details = spec.details;
        if let Some(message) = spec.message {
            details.insert(
                0,
                Line::from(Span::styled(
                    format!("{} {}", message.marker(), message.text()),
                    message.style(),
                )),
            );
        }
        let lines = super::text::wrap_lines(details, body.width);
        let count = lines.len();
        let paragraph = Paragraph::new(lines);
        let offset = spec
            .detail_scroll
            .min(count.saturating_sub(usize::from(body.height)));
        frame.render_widget(
            paragraph.scroll((offset.min(u16::MAX as usize) as u16, 0)),
            body,
        );
        let warning_area = Rect::new(footer.x, footer.y, footer.width, 4);
        frame.render_widget(
            Paragraph::new(format!("⚠ {}", spec.warning))
                .style(theme.danger().add_modifier(Modifier::BOLD))
                .wrap(Wrap { trim: false }),
            warning_area,
        );
        let controls = Rect::new(footer.x, footer.y + 4, footer.width, 4);
        let lines = vec![
            Line::from(Span::styled("输入 YES 确认写入", theme.warning())),
            Line::from(Span::styled(
                format!("> {}", spec.confirmation),
                theme.input_focused(),
            )),
            Line::from(format!("Enter {action_label} · Esc 取消")),
            Line::from(Span::styled("PgUp/PgDn 滚动详情", theme.muted())),
        ];
        frame.render_widget(Paragraph::new(lines), controls);
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
