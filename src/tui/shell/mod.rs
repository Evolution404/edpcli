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
        InputMode::Help => ("HELP", theme.muted()),
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" edpcli", theme.accent()),
            Span::styled(
                format!(" v{}  TUI", env!("CARGO_PKG_VERSION")),
                theme.muted(),
            ),
            Span::styled(format!("  [{mode}]"), mode_style),
            Span::styled("  ·  管理员模式", theme.success()),
            animation::compact_indicator(state.animation_frame(), core_mode),
        ]))
        .style(theme.surface()),
        area,
    );
}

pub fn navigation(frame: &mut Frame, area: Rect, workspace: Workspace) {
    let theme = theme::current();
    let labels = if ui::ViewportClass::for_width(area.width) == ui::ViewportClass::Compact {
        ["设备", "检查", "制盘", "备份"]
    } else {
        ["设备", "Inspect", "制盘", "备份"]
    };
    let index = Workspace::ALL
        .iter()
        .position(|candidate| *candidate == workspace)
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
    if area.width >= 70 {
        let help = Rect::new(area.right().saturating_sub(8), area.y, 8, area.height);
        frame.render_widget(Paragraph::new("? 帮助").style(theme.muted()), help);
    }
}

pub fn footer(frame: &mut Frame, area: Rect, text: &str) {
    frame.render_widget(
        Paragraph::new(crate::ui::sanitize_terminal_text(text))
            .style(theme::current().secondary_text()),
        area,
    );
}
