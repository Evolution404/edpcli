use super::*;

impl AppState {
    pub(crate) fn notice_details(&self) -> Option<&crate::tui::ui::UiMessage> {
        self.shell.notice_details.as_ref()
    }

    pub(crate) fn notice_scroll(&self) -> usize {
        self.shell.notice_scroll
    }

    pub(crate) fn open_notice_details(&mut self) {
        self.shell.notice_details = self.shell.notice.clone();
        self.shell.notice_scroll = 0;
    }

    pub(crate) fn close_notice_details(&mut self) {
        self.shell.notice_details = None;
        self.shell.notice_scroll = 0;
    }

    pub(crate) fn scroll_notice_details(&mut self, action: crate::tui::keymap::TuiAction) {
        use crate::tui::keymap::TuiAction;
        let (count, rows) = crate::tui::notice_overlay::metrics(self);
        let max = count.saturating_sub(rows);
        let offset = self.shell.notice_scroll.min(max);
        self.shell.notice_scroll = match action {
            TuiAction::MoveUp => offset.saturating_sub(1),
            TuiAction::MoveDown => offset.saturating_add(1).min(max),
            TuiAction::PageUp => offset.saturating_sub(rows),
            TuiAction::PageDown => offset.saturating_add(rows).min(max),
            TuiAction::HalfPageUp => offset.saturating_sub((rows / 2).max(1)),
            TuiAction::HalfPageDown => offset.saturating_add((rows / 2).max(1)).min(max),
            TuiAction::Top => 0,
            TuiAction::Bottom => max,
            _ => offset,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expired_toast_remains_available_in_details() {
        let mut state = AppState::new();
        state.set_error_notice("完整错误原因");
        state.shell.notice_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(5));
        assert!(state.notice_message().is_none());
        state.open_notice_details();
        assert_eq!(state.notice_details().unwrap().text(), "完整错误原因");
    }
}
