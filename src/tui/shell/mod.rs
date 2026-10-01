//! Application chrome shared by every workspace.

use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Paragraph, Tabs},
    Frame,
};

use crate::tui::{
    animation::{self, CoreMode},
    state::{AppState, InputMode, Workspace},
    theme, ui,
};

pub fn header(frame: &mut Frame, area: Rect, state: &AppState, core_mode: CoreMode) {
    let theme = theme::current();
    let (mode, mode_style) = match state.input_mode() {
        InputMode::Normal => ("NORMAL", theme.muted()),
        InputMode::Insert => ("INSERT", theme.accent()),
        InputMode::Search => ("SEARCH", theme.secondary_accent()),
        InputMode::Command => ("COMMAND", theme.secondary_accent()),
        InputMode::Confirm => ("CONFIRM", theme.warning()),
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" edpcli", theme.accent()),
            Span::styled(
                format!(" v{}  TUI", env!("CARGO_PKG_VERSION")),
                theme.muted(),
            ),
            Span::styled(format!("  [{mode}]"), mode_style),
            if state.is_demo() {
                Span::styled("  ·  演示模式 / 不访问真实介质", theme.warning())
            } else {
                Span::styled("  ·  管理员模式", theme.success())
            },
            animation::compact_indicator(state.animation_frame(), core_mode),
        ]))
        .style(theme.surface()),
        area,
    );
}

pub fn navigation(frame: &mut Frame, area: Rect, state: &AppState) {
    let theme = theme::current();
    let class = ui::ViewportClass::for_width(area.width);
    let labels = ["设备", "备份"];
    let active = match state.workspace() {
        Workspace::Devices | Workspace::Provision => Workspace::Devices,
        Workspace::Backups => Workspace::Backups,
        Workspace::Inspect => match state.advanced_inspect().map(|inspect| &inspect.source) {
            Some(crate::tui::state::AdvancedInspectSource::Backup(_)) => Workspace::Backups,
            _ => Workspace::Devices,
        },
    };
    let index = Workspace::TOP_LEVEL
        .iter()
        .position(|candidate| *candidate == active)
        .unwrap_or(0);
    frame.render_widget(
        Tabs::new(labels)
            .select(index)
            .style(theme.tab())
            .highlight_style(theme.active_tab().add_modifier(Modifier::BOLD))
            .divider(Span::styled("  ·  ", theme.muted()))
            .padding("  ", "  "),
        area,
    );
    if class != ui::ViewportClass::Compact {
        let help = Rect::new(area.right().saturating_sub(8), area.y, 8, area.height);
        frame.render_widget(Paragraph::new("? 帮助").style(theme.muted()), help);
    }
}

pub fn message_bar(
    frame: &mut Frame,
    area: Rect,
    notice: Option<&crate::tui::ui::UiMessage>,
    status: Option<&str>,
) {
    let theme = theme::current();
    let mut spans = Vec::new();
    if let Some(notice) = notice {
        spans.push(Span::styled(
            format!("{} ", notice.marker()),
            notice.style(),
        ));
        spans.push(Span::styled(
            crate::ui::sanitize_terminal_text(notice.text()),
            notice.style(),
        ));
    }
    if let Some(status) = status {
        if !spans.is_empty() {
            spans.push(Span::styled("  ·  ", theme.muted()));
        }
        spans.push(Span::styled(
            crate::ui::sanitize_terminal_text(status),
            theme.secondary_text(),
        ));
    }
    if spans.is_empty() {
        spans.push(Span::styled("就绪", theme.muted()));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme.surface()),
        area,
    );
}
