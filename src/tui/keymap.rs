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
    Open,
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
    BackupCreate,
    Provision,
    Restore,
    Fill,
    Export,
    ViewOrVerify,
    Yank,
    YankRaw,
    PageUp,
    PageDown,
    SectorPrevious,
    SectorNext,
    RowStart,
    RowEnd,
    FocusNext,
    FocusPrevious,
    InspectJump,
    PanelLeft,
    PanelDown,
    PanelUp,
    PanelRight,
    PanelNext,
    PanelPrevious,
    TableColumnLeft,
    TableColumnRight,
    TableMoveColumnLeft,
    TableMoveColumnRight,
    TableColumnFirst,
    TableColumnLast,
    TableScrollLeft,
    TableScrollRight,
    TableSortToggle,
    TableSortClear,
    TableCopyCell,
    TableCopyRow,
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
pub enum WidgetRole {
    Tree,
    Table,
    Input,
    SectorInspector,
    Picker,
    Other,
}

#[path = "keymap/help.rs"]
mod help;
#[path = "keymap/help_workflows.rs"]
mod help_workflows;
pub use help::{
    HelpBinding, BACKUPS_HELP, DEVICES_HELP, GLOBAL_HELP, GUARD_GLOBAL_HELP, INSPECT_HELP,
    SECTOR_INSPECT_HELP, TABLE_HELP,
};
pub use help_workflows::{
    PICKER_HELP, PROVISION_HELP, PROVISION_RESULT_HELP, PROVISION_REVIEW_HELP,
    PROVISION_RUNNING_HELP, RESTORE_RESULT_HELP,
};

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

    pub fn map_for_role(
        &mut self,
        mode: InputMode,
        role: WidgetRole,
        event: KeyEvent,
    ) -> Option<TuiAction> {
        if mode == InputMode::Normal && role == WidgetRole::Table {
            let table_action = match event.code {
                KeyCode::Char('<') => Some(TuiAction::TableMoveColumnLeft),
                KeyCode::Char('>') => Some(TuiAction::TableMoveColumnRight),
                _ => None,
            };
            if table_action.is_some() {
                self.clear_pending();
                return table_action;
            }
        }
        let action = self.map(mode, event)?;
        if mode == InputMode::Normal && role == WidgetRole::Table {
            return Some(match action {
                TuiAction::MoveLeft => TuiAction::TableColumnLeft,
                TuiAction::MoveRight => TuiAction::TableColumnRight,
                TuiAction::RowStart => TuiAction::TableColumnFirst,
                TuiAction::RowEnd => TuiAction::TableColumnLast,
                TuiAction::Yank => TuiAction::TableCopyCell,
                TuiAction::YankRaw => TuiAction::TableCopyRow,
                other => other,
            });
        }
        Some(action)
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

        if let Some(pending) = self.pending.take() {
            return match pending.prefix {
                PendingPrefix::G => match event.code {
                    KeyCode::Char('g') => Some(TuiAction::Top),
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

        if mode != InputMode::Normal
            && event.modifiers.contains(KeyModifiers::CONTROL)
            && event.code == KeyCode::Char('w')
        {
            self.pending = Some(Pending {
                prefix: PendingPrefix::CtrlW,
                since: now,
            });
            return None;
        }

        match mode {
            InputMode::Insert => return self.map_insert(event),
            InputMode::Search | InputMode::Command => return self.map_text_entry(event),
            InputMode::Confirm => return self.map_confirm(event),
            InputMode::Normal => {}
        }

        if event.code == KeyCode::Char('q') && event.modifiers.is_empty() {
            self.pending = None;
            return Some(TuiAction::Quit);
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
            KeyCode::Tab => Some(TuiAction::FocusNext),
            KeyCode::BackTab => Some(TuiAction::FocusPrevious),
            KeyCode::Char('j') | KeyCode::Down => Some(TuiAction::MoveDown),
            KeyCode::Char('k') | KeyCode::Up => Some(TuiAction::MoveUp),
            KeyCode::Char('h') | KeyCode::Left => Some(TuiAction::MoveLeft),
            KeyCode::Char('l') | KeyCode::Right => Some(TuiAction::MoveRight),
            KeyCode::Char('H') => Some(TuiAction::TableScrollLeft),
            KeyCode::Char('L') => Some(TuiAction::TableScrollRight),
            KeyCode::Char('s') => Some(TuiAction::TableSortToggle),
            KeyCode::Char('S') => Some(TuiAction::TableSortClear),
            KeyCode::Home => Some(TuiAction::Top),
            KeyCode::End | KeyCode::Char('G') => Some(TuiAction::Bottom),
            KeyCode::Enter => Some(TuiAction::Activate),
            KeyCode::Char('o') => Some(TuiAction::Open),
            KeyCode::Char('/') => Some(TuiAction::Search),
            KeyCode::Char('n') => Some(TuiAction::NextMatch),
            KeyCode::Char('N') => Some(TuiAction::PreviousMatch),
            KeyCode::Char(':') => Some(TuiAction::Command),
            KeyCode::Char('?') => Some(TuiAction::Help),
            KeyCode::Char('i') => Some(TuiAction::Insert),
            KeyCode::Char('r') => Some(TuiAction::Refresh),
            KeyCode::Char('d') => Some(TuiAction::Delete),
            KeyCode::Char('a') => Some(TuiAction::Add),
            KeyCode::Char('b') => Some(TuiAction::BackupCreate),
            KeyCode::Char('p') => Some(TuiAction::Provision),
            KeyCode::Char('R') => Some(TuiAction::Restore),
            KeyCode::Char('f') => Some(TuiAction::Fill),
            KeyCode::Char('e') => Some(TuiAction::Export),
            KeyCode::Char('v') => Some(TuiAction::ViewOrVerify),
            KeyCode::Char('J') => Some(TuiAction::InspectJump),
            KeyCode::Char(' ') => Some(TuiAction::Toggle),
            KeyCode::Char('y') => Some(TuiAction::Yank),
            KeyCode::Char('Y') => Some(TuiAction::YankRaw),
            KeyCode::PageUp => Some(TuiAction::PageUp),
            KeyCode::PageDown => Some(TuiAction::PageDown),
            KeyCode::Char('[') => Some(TuiAction::SectorPrevious),
            KeyCode::Char(']') => Some(TuiAction::SectorNext),
            KeyCode::Char('0') => Some(TuiAction::RowStart),
            KeyCode::Char('$') => Some(TuiAction::RowEnd),
            KeyCode::Esc => Some(TuiAction::Back),
            _ => None,
        }
    }

    fn map_insert(&mut self, event: KeyEvent) -> Option<TuiAction> {
        self.pending = None;
        if event.modifiers.contains(KeyModifiers::CONTROL) && event.code == KeyCode::Char('c') {
            return Some(TuiAction::Back);
        }
        match event.code {
            KeyCode::Esc => Some(TuiAction::Back),
            KeyCode::Enter => Some(TuiAction::Submit),
            KeyCode::Tab => Some(TuiAction::FocusNext),
            KeyCode::BackTab => Some(TuiAction::FocusPrevious),
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
        if event.modifiers.contains(KeyModifiers::CONTROL) && event.code == KeyCode::Char('c') {
            return Some(TuiAction::Back);
        }
        match event.code {
            KeyCode::Esc => Some(TuiAction::Back),
            KeyCode::Enter => Some(TuiAction::Submit),
            KeyCode::Tab => Some(TuiAction::FocusNext),
            KeyCode::BackTab => Some(TuiAction::FocusPrevious),
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
        if event.modifiers.contains(KeyModifiers::CONTROL) && event.code == KeyCode::Char('c') {
            return Some(TuiAction::Cancel);
        }
        match event.code {
            KeyCode::Esc | KeyCode::Char('n') => Some(TuiAction::Cancel),
            KeyCode::Char('y') => Some(TuiAction::Confirm),
            KeyCode::Enter => Some(TuiAction::Submit),
            KeyCode::Tab => Some(TuiAction::FocusNext),
            KeyCode::BackTab => Some(TuiAction::FocusPrevious),
            KeyCode::Backspace => Some(TuiAction::Backspace),
            KeyCode::Char(ch) if !event.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(TuiAction::Text(ch))
            }
            _ => None,
        }
    }
}
