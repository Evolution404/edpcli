use super::*;

#[derive(Debug)]
pub struct ShellState {
    pub(super) demo_mode: bool,
    pub(super) workspace: Workspace,
    pub(super) critical_operation: bool,
    pub(super) exit_pending: bool,
    pub(super) navigation: NavigationStack,
    pub(super) notice: Option<String>,
    pub(super) notice_at: Option<std::time::Instant>,
    pub(super) animation_frame: u64,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            demo_mode: false,
            workspace: Workspace::Devices,
            critical_operation: false,
            exit_pending: false,
            navigation: NavigationStack::default(),
            notice: None,
            notice_at: None,
            animation_frame: 0,
        }
    }
}
