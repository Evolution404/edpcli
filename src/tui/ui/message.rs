use std::ops::Deref;

use ratatui::style::Style;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMessageTone {
    Info,
    Progress,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiMessage {
    text: String,
    tone: UiMessageTone,
}

impl UiMessage {
    pub fn info(text: impl Into<String>) -> Self {
        Self::new(text, UiMessageTone::Info)
    }

    pub fn progress(text: impl Into<String>) -> Self {
        Self::new(text, UiMessageTone::Progress)
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self::new(text, UiMessageTone::Success)
    }

    pub fn warning(text: impl Into<String>) -> Self {
        Self::new(text, UiMessageTone::Warning)
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self::new(text, UiMessageTone::Error)
    }

    pub fn new(text: impl Into<String>, tone: UiMessageTone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn tone(&self) -> UiMessageTone {
        self.tone
    }

    pub fn style(&self) -> Style {
        let theme = crate::tui::theme::current();
        match self.tone {
            UiMessageTone::Info => theme.secondary_text(),
            UiMessageTone::Progress => theme.secondary_accent(),
            UiMessageTone::Success => theme.success(),
            UiMessageTone::Warning => theme.warning(),
            UiMessageTone::Error => theme.danger(),
        }
    }

    pub const fn result_tone(&self) -> super::ResultTone {
        match self.tone {
            UiMessageTone::Info => super::ResultTone::Primary,
            UiMessageTone::Progress => super::ResultTone::Accent,
            UiMessageTone::Success => super::ResultTone::Success,
            UiMessageTone::Warning => super::ResultTone::Warning,
            UiMessageTone::Error => super::ResultTone::Danger,
        }
    }

    pub const fn is_error(&self) -> bool {
        matches!(self.tone, UiMessageTone::Error)
    }

    pub const fn marker(&self) -> &'static str {
        match self.tone {
            UiMessageTone::Info => "●",
            UiMessageTone::Progress => "◌",
            UiMessageTone::Success => "✓",
            UiMessageTone::Warning => "⚠",
            UiMessageTone::Error => "✗",
        }
    }
}

impl Deref for UiMessage {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.text()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_tone_controls_marker_style_and_result_tone() {
        let success = UiMessage::success("已完成");
        assert_eq!(success.marker(), "✓");
        assert_eq!(success.tone(), UiMessageTone::Success);
        assert_eq!(success.style(), crate::tui::theme::current().success());
        assert_eq!(success.result_tone(), super::super::ResultTone::Success);
        assert!(!success.is_error());

        let error = UiMessage::error("失败");
        assert_eq!(error.marker(), "✗");
        assert_eq!(error.tone(), UiMessageTone::Error);
        assert_eq!(error.style(), crate::tui::theme::current().danger());
        assert_eq!(error.result_tone(), super::super::ResultTone::Danger);
        assert!(error.is_error());
    }
}
