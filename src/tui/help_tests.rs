use crate::tui::{keymap::TuiAction, state::NavCommand};

use super::*;
#[test]
fn last_help_binding_remains_reachable_after_narrow_resize() {
    let mut state = AppState::new();
    state.navigate(NavCommand::Help, 24);
    for (width, height) in [(160, 50), (40, 24), (80, 24)] {
        state.set_viewport_size(ratatui::layout::Size::new(width, height));
        state.scroll_help(TuiAction::Bottom);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| draw_help_overlay(frame, &state))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.replace(' ', "").contains("Esc关闭"));
        let (count, rows) = metrics(&state);
        assert_eq!(state.help_scroll(), count.saturating_sub(rows));
        state.scroll_help(TuiAction::Top);
        assert_eq!(state.help_scroll(), 0);
    }
}
