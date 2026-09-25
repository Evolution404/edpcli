//! Centralized Vim-style keymap for the TUI.
//!
//! Physical key events are translated into UI actions here. Workspace code consumes
//! actions and decides whether they are meaningful in the current context.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::state::InputMode;

const PREFIX_TIMEOUT: Duration = Duration::from_millis(900);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiAction {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Top,
    Bottom,
    HalfPageUp,
    HalfPageDown,
    Activate,
    Toggle,
    Back,
    Quit,
    Help,
    Search,
    NextMatch,
    PreviousMatch,
    Command,
    Insert,
    Refresh,
    Delete,
    Add,
    Fill,
    Plan,
    Write,
    Export,
    ViewOrVerify,
    Yank,
    YankRaw,
    PageUp,
    PageDown,
    RowStart,
    RowEnd,
    WorkspaceNext,
    WorkspacePrevious,
    WorkspaceDevices,
    WorkspaceBackups,
    WorkspaceProvision,
    WorkspaceInspect,
    InspectJump,
    PanelLeft,
    PanelDown,
    PanelUp,
    PanelRight,
    PanelNext,
    PanelPrevious,
    Text(char),
    Backspace,
    DeleteChar,
    CursorLeft,
    CursorRight,
    CursorHome,
    CursorEnd,
    Submit,
    Confirm,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingPrefix {
    G,
    CtrlW,
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    prefix: PendingPrefix,
    since: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelpBinding {
    pub keys: &'static str,
    pub label: &'static str,
    pub action: TuiAction,
}

pub const NORMAL_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "j/k",
        label: "Move",
        action: TuiAction::MoveDown,
    },
    HelpBinding {
        keys: "Enter/o",
        label: "Open",
        action: TuiAction::Activate,
    },
    HelpBinding {
        keys: "/",
        label: "Search",
        action: TuiAction::Search,
    },
    HelpBinding {
        keys: "r",
        label: "Refresh",
        action: TuiAction::Refresh,
    },
    HelpBinding {
        keys: "?",
        label: "Help",
        action: TuiAction::Help,
    },
];

pub const INSPECT_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "j/k",
        label: "Move",
        action: TuiAction::MoveDown,
    },
    HelpBinding {
        keys: "h/l",
        label: "Fold",
        action: TuiAction::MoveLeft,
    },
    HelpBinding {
        keys: "Enter/o",
        label: "Open",
        action: TuiAction::Activate,
    },
    HelpBinding {
        keys: "/",
        label: "Search",
        action: TuiAction::Search,
    },
    HelpBinding {
        keys: "gl",
        label: "Goto",
        action: TuiAction::InspectJump,
    },
    HelpBinding {
        keys: "?",
        label: "Help",
        action: TuiAction::Help,
    },
];

pub fn is_actionable_key(event: &KeyEvent) -> bool {
    !matches!(event.kind, KeyEventKind::Release)
}

#[derive(Debug, Default)]
pub struct KeyMapper {
    pending: Option<Pending>,
}

impl KeyMapper {
    pub const fn new() -> Self {
        Self { pending: None }
    }

    pub fn clear_pending(&mut self) {
        self.pending = None;
    }

    pub fn map(&mut self, mode: InputMode, event: KeyEvent) -> Option<TuiAction> {
        self.map_at(mode, event, Instant::now())
    }

    pub fn map_at(&mut self, mode: InputMode, event: KeyEvent, now: Instant) -> Option<TuiAction> {
        if !is_actionable_key(&event) {
            return None;
        }

        if self
            .pending
            .is_some_and(|pending| now.duration_since(pending.since) > PREFIX_TIMEOUT)
        {
            self.pending = None;
        }

        match mode {
            InputMode::Insert => return self.map_insert(event),
            InputMode::Search | InputMode::Command => return self.map_text_entry(event),
            InputMode::Confirm => return self.map_confirm(event),
            InputMode::Help => {
                return match event.code {
                    KeyCode::Esc | KeyCode::Char('q') => Some(TuiAction::Back),
                    _ => None,
                };
            }
            InputMode::Normal => {}
        }

        if let Some(pending) = self.pending.take() {
            return match pending.prefix {
                PendingPrefix::G => match event.code {
                    KeyCode::Char('g') => Some(TuiAction::Top),
                    KeyCode::Char('t') => Some(TuiAction::WorkspaceNext),
                    KeyCode::Char('T') => Some(TuiAction::WorkspacePrevious),
                    KeyCode::Char('d') => Some(TuiAction::WorkspaceDevices),
                    KeyCode::Char('b') => Some(TuiAction::WorkspaceBackups),
                    KeyCode::Char('p') => Some(TuiAction::WorkspaceProvision),
                    KeyCode::Char('i') => Some(TuiAction::WorkspaceInspect),
                    KeyCode::Char('l') => Some(TuiAction::InspectJump),
                    _ => None,
                },
                PendingPrefix::CtrlW => match event.code {
                    KeyCode::Char('h') | KeyCode::Left => Some(TuiAction::PanelLeft),
                    KeyCode::Char('j') | KeyCode::Down => Some(TuiAction::PanelDown),
                    KeyCode::Char('k') | KeyCode::Up => Some(TuiAction::PanelUp),
                    KeyCode::Char('l') | KeyCode::Right => Some(TuiAction::PanelRight),
                    KeyCode::Char('w') => Some(TuiAction::PanelNext),
                    KeyCode::Char('W') => Some(TuiAction::PanelPrevious),
                    _ => None,
                },
            };
        }

        if event.modifiers.contains(KeyModifiers::CONTROL) {
            return match event.code {
                KeyCode::Char('w') => {
                    self.pending = Some(Pending {
                        prefix: PendingPrefix::CtrlW,
                        since: now,
                    });
                    None
                }
                KeyCode::Char('d') => Some(TuiAction::HalfPageDown),
                KeyCode::Char('u') => Some(TuiAction::HalfPageUp),
                KeyCode::Char('c') => Some(TuiAction::Quit),
                _ => None,
            };
        }

        match event.code {
            KeyCode::Char('g') => {
                self.pending = Some(Pending {
                    prefix: PendingPrefix::G,
                    since: now,
                });
                None
            }
            KeyCode::Tab => Some(TuiAction::PanelNext),
            KeyCode::BackTab => Some(TuiAction::PanelPrevious),
            KeyCode::Char('j') | KeyCode::Down => Some(TuiAction::MoveDown),
            KeyCode::Char('k') | KeyCode::Up => Some(TuiAction::MoveUp),
            KeyCode::Char('h') | KeyCode::Left => Some(TuiAction::MoveLeft),
            KeyCode::Char('l') | KeyCode::Right => Some(TuiAction::MoveRight),
            KeyCode::Home => Some(TuiAction::Top),
            KeyCode::End | KeyCode::Char('G') => Some(TuiAction::Bottom),
            KeyCode::Enter | KeyCode::Char('o') => Some(TuiAction::Activate),
            KeyCode::Char('/') => Some(TuiAction::Search),
            KeyCode::Char('n') => Some(TuiAction::NextMatch),
            KeyCode::Char('N') => Some(TuiAction::PreviousMatch),
            KeyCode::Char(':') => Some(TuiAction::Command),
            KeyCode::Char('?') => Some(TuiAction::Help),
            KeyCode::Char('i') => Some(TuiAction::Insert),
            KeyCode::Char('r') => Some(TuiAction::Refresh),
            KeyCode::Char('d') => Some(TuiAction::Delete),
            KeyCode::Char('a') => Some(TuiAction::Add),
            KeyCode::Char('f') => Some(TuiAction::Fill),
            KeyCode::Char('p') => Some(TuiAction::Plan),
            KeyCode::Char('w') => Some(TuiAction::Write),
            KeyCode::Char('e') => Some(TuiAction::Export),
            KeyCode::Char('v') => Some(TuiAction::ViewOrVerify),
            KeyCode::Char(' ') => Some(TuiAction::Toggle),
            KeyCode::Char('y') => Some(TuiAction::Yank),
            KeyCode::Char('Y') => Some(TuiAction::YankRaw),
            KeyCode::PageUp => Some(TuiAction::PageUp),
            KeyCode::PageDown => Some(TuiAction::PageDown),
            KeyCode::Char('0') => Some(TuiAction::RowStart),
            KeyCode::Char('$') => Some(TuiAction::RowEnd),
            KeyCode::Esc | KeyCode::Char('q') => Some(TuiAction::Back),
            _ => None,
        }
    }

    fn map_insert(&mut self, event: KeyEvent) -> Option<TuiAction> {
        self.pending = None;
        match event.code {
            KeyCode::Esc => Some(TuiAction::Back),
            KeyCode::Enter => Some(TuiAction::Submit),
            KeyCode::Left => Some(TuiAction::CursorLeft),
            KeyCode::Right => Some(TuiAction::CursorRight),
            KeyCode::Home => Some(TuiAction::CursorHome),
            KeyCode::End => Some(TuiAction::CursorEnd),
            KeyCode::Backspace => Some(TuiAction::Backspace),
            KeyCode::Delete => Some(TuiAction::DeleteChar),
            KeyCode::Char(ch) if !event.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(TuiAction::Text(ch))
            }
            _ => None,
        }
    }

    fn map_text_entry(&mut self, event: KeyEvent) -> Option<TuiAction> {
        self.pending = None;
        match event.code {
            KeyCode::Esc => Some(TuiAction::Back),
            KeyCode::Enter => Some(TuiAction::Submit),
            KeyCode::Backspace => Some(TuiAction::Backspace),
            KeyCode::Delete => Some(TuiAction::DeleteChar),
            KeyCode::Left => Some(TuiAction::CursorLeft),
            KeyCode::Right => Some(TuiAction::CursorRight),
            KeyCode::Home => Some(TuiAction::CursorHome),
            KeyCode::End => Some(TuiAction::CursorEnd),
            KeyCode::Char(ch) if !event.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(TuiAction::Text(ch))
            }
            _ => None,
        }
    }

    fn map_confirm(&mut self, event: KeyEvent) -> Option<TuiAction> {
        self.pending = None;
        match event.code {
            KeyCode::Esc | KeyCode::Char('n') => Some(TuiAction::Cancel),
            KeyCode::Char('y') => Some(TuiAction::Confirm),
            KeyCode::Enter => Some(TuiAction::Submit),
            KeyCode::Backspace => Some(TuiAction::Backspace),
            KeyCode::Char(ch) if !event.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(TuiAction::Text(ch))
            }
            _ => None,
        }
    }
}
