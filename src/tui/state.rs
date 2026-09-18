//! Pure TUI state machine.
//!
//! The state layer never performs I/O. That makes navigation and cancellation semantics testable
//! without a real terminal and keeps critical-operation policy independent from crossterm.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workspace {
    Devices,
    Backups,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
    Command,
    Help,
    Refresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavCommand {
    Up,
    Down,
    Left,
    Right,
    Top,
    Bottom,
    HalfPageDown,
    HalfPageUp,
    Search,
    NextMatch,
    PreviousMatch,
    CommandPalette,
    Escape,
    Quit,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateEffect {
    None,
    ExitRequested,
    ExitDeferred,
}

pub struct AppState {
    workspace: Workspace,
    devices: Vec<crate::disk_scan::Row>,
    backups: Vec<crate::application::BackupWorkspaceItem>,
    device_scan_pending: bool,
    backup_scan_pending: bool,
    selected: usize,
    item_count: usize,
    input_mode: InputMode,
    critical_operation: bool,
    exit_pending: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub const fn new() -> Self {
        Self {
            workspace: Workspace::Devices,
            devices: Vec::new(),
            backups: Vec::new(),
            device_scan_pending: false,
            backup_scan_pending: false,
            selected: 0,
            item_count: 0,
            input_mode: InputMode::Normal,
            critical_operation: false,
            exit_pending: false,
        }
    }

    pub const fn workspace(&self) -> Workspace {
        self.workspace
    }

    pub fn devices(&self) -> &[crate::disk_scan::Row] {
        &self.devices
    }

    pub const fn device_scan_pending(&self) -> bool {
        self.device_scan_pending
    }

    pub const fn backup_scan_pending(&self) -> bool {
        self.backup_scan_pending
    }

    pub const fn active_scan_pending(&self) -> bool {
        match self.workspace {
            Workspace::Devices => self.device_scan_pending,
            Workspace::Backups => self.backup_scan_pending,
        }
    }

    pub fn set_device_scan_pending(&mut self, pending: bool) {
        self.device_scan_pending = pending;
    }

    pub fn replace_devices(&mut self, devices: Vec<crate::disk_scan::Row>) {
        self.devices = devices;
        self.device_scan_pending = false;
        if self.workspace == Workspace::Devices {
            self.set_item_count(self.devices.len());
        }
    }

    pub fn backups(&self) -> &[crate::application::BackupWorkspaceItem] {
        &self.backups
    }

    pub fn set_backup_scan_pending(&mut self, pending: bool) {
        self.backup_scan_pending = pending;
    }

    pub fn replace_backups(&mut self, backups: Vec<crate::application::BackupWorkspaceItem>) {
        self.backups = backups;
        self.backup_scan_pending = false;
        if self.workspace == Workspace::Backups {
            self.set_item_count(self.backups.len());
        }
    }

    fn switch_workspace(&mut self, workspace: Workspace) {
        if self.workspace == workspace {
            return;
        }
        self.workspace = workspace;
        self.selected = 0;
        let count = match workspace {
            Workspace::Devices => self.devices.len(),
            Workspace::Backups => self.backups.len(),
        };
        self.set_item_count(count);
    }

    pub const fn selected(&self) -> usize {
        self.selected
    }

    pub const fn item_count(&self) -> usize {
        self.item_count
    }

    pub const fn input_mode(&self) -> InputMode {
        self.input_mode
    }

    pub const fn is_critical_operation(&self) -> bool {
        self.critical_operation
    }

    pub const fn exit_pending(&self) -> bool {
        self.exit_pending
    }

    pub fn set_item_count(&mut self, item_count: usize) {
        self.item_count = item_count;
        if item_count == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(item_count - 1);
        }
    }

    pub fn set_critical_operation(&mut self, critical: bool) {
        self.critical_operation = critical;
    }

    pub fn take_deferred_exit(&mut self) -> StateEffect {
        if !self.critical_operation && self.exit_pending {
            self.exit_pending = false;
            StateEffect::ExitRequested
        } else {
            StateEffect::None
        }
    }

    pub fn navigate(&mut self, command: NavCommand, viewport_height: usize) -> StateEffect {
        if self.critical_operation && matches!(command, NavCommand::Quit | NavCommand::Escape) {
            self.exit_pending = true;
            return StateEffect::ExitDeferred;
        }

        if command == NavCommand::Escape {
            if self.input_mode != InputMode::Normal {
                self.input_mode = InputMode::Normal;
                return StateEffect::None;
            }
            return StateEffect::ExitRequested;
        }

        if command == NavCommand::Quit {
            return StateEffect::ExitRequested;
        }

        match command {
            NavCommand::Up => {
                self.selected = self.selected.saturating_sub(1);
            }
            NavCommand::Down => {
                if self.item_count > 0 {
                    self.selected = (self.selected + 1).min(self.item_count - 1);
                }
            }
            NavCommand::Top => self.selected = 0,
            NavCommand::Bottom => {
                self.selected = self.item_count.saturating_sub(1);
            }
            NavCommand::HalfPageDown => {
                if self.item_count > 0 {
                    let delta = (viewport_height / 2).max(1);
                    self.selected = self
                        .selected
                        .saturating_add(delta)
                        .min(self.item_count - 1);
                }
            }
            NavCommand::HalfPageUp => {
                let delta = (viewport_height / 2).max(1);
                self.selected = self.selected.saturating_sub(delta);
            }
            NavCommand::Search => self.input_mode = InputMode::Search,
            NavCommand::CommandPalette => self.input_mode = InputMode::Command,
            NavCommand::Help => self.input_mode = InputMode::Help,
            NavCommand::Left => self.switch_workspace(Workspace::Devices),
            NavCommand::Right => self.switch_workspace(Workspace::Backups),
            NavCommand::Refresh
            | NavCommand::NextMatch
            | NavCommand::PreviousMatch
            | NavCommand::Escape
            | NavCommand::Quit => {}
        }
        StateEffect::None
    }
}
