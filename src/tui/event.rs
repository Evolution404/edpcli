//! Crossterm event translation into UI-neutral navigation intents.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::state::NavCommand;

#[derive(Debug, Default)]
pub struct KeyMapper {
    pending_g: bool,
}

impl KeyMapper {
    pub const fn new() -> Self {
        Self { pending_g: false }
    }

    pub fn map(&mut self, event: KeyEvent) -> Option<NavCommand> {
        if matches!(event.kind, KeyEventKind::Release) {
            return None;
        }

        if event.modifiers.contains(KeyModifiers::CONTROL) {
            self.pending_g = false;
            return match event.code {
                KeyCode::Char('d') => Some(NavCommand::HalfPageDown),
                KeyCode::Char('u') => Some(NavCommand::HalfPageUp),
                KeyCode::Char('c') => Some(NavCommand::Quit),
                _ => None,
            };
        }

        if self.pending_g {
            self.pending_g = false;
            if event.code == KeyCode::Char('g') {
                return Some(NavCommand::Top);
            }
        }

        match event.code {
            KeyCode::Char('g') => {
                self.pending_g = true;
                None
            }
            KeyCode::Char('G') => Some(NavCommand::Bottom),
            KeyCode::Char('j') | KeyCode::Down => Some(NavCommand::Down),
            KeyCode::Char('k') | KeyCode::Up => Some(NavCommand::Up),
            KeyCode::Char('h') | KeyCode::Left => Some(NavCommand::Left),
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => Some(NavCommand::Right),
            KeyCode::Char('/') => Some(NavCommand::Search),
            KeyCode::Char('n') => Some(NavCommand::NextMatch),
            KeyCode::Char('N') => Some(NavCommand::PreviousMatch),
            KeyCode::Char(':') => Some(NavCommand::CommandPalette),
            KeyCode::Char('?') => Some(NavCommand::Help),
            KeyCode::Char('q') => Some(NavCommand::Quit),
            KeyCode::Esc => Some(NavCommand::Escape),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn maps_vim_navigation_contract() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            Some(NavCommand::Down)
        );
        assert_eq!(
            mapper.map(key(KeyCode::Char('k'), KeyModifiers::NONE)),
            Some(NavCommand::Up)
        );
        assert_eq!(
            mapper.map(key(KeyCode::Char('G'), KeyModifiers::SHIFT)),
            Some(NavCommand::Bottom)
        );
        assert_eq!(
            mapper.map(key(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            Some(NavCommand::HalfPageDown)
        );
        assert_eq!(
            mapper.map(key(KeyCode::Char('u'), KeyModifiers::CONTROL)),
            Some(NavCommand::HalfPageUp)
        );
    }

    #[test]
    fn double_g_maps_to_top_without_leaking_first_g() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(key(KeyCode::Char('g'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            mapper.map(key(KeyCode::Char('g'), KeyModifiers::NONE)),
            Some(NavCommand::Top)
        );
    }

    #[test]
    fn ctrl_c_uses_same_safe_quit_intent_as_q() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(NavCommand::Quit)
        );
    }
}
