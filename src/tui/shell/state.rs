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
    pub(super) selected: usize,
    pub(super) item_count: usize,
    pub(super) input_mode: InputMode,
    pub(super) input_buffer: String,
    pub(super) search_query: String,
    pub(super) search_matches: Vec<usize>,
    pub(super) search_cursor: usize,
    pub(super) wizard: Option<WizardState>,
    pub(super) pinned_disk: Option<u32>,
    pub(super) disk_layout_tail: crate::tui::disk_layout::TailExpansion,
    pub(super) disk_layout_selected: usize,
    pub(super) horizontal_scroll: std::collections::BTreeMap<
        crate::tui::table_layout::TableKind,
        crate::tui::table_layout::HorizontalScrollState,
    >,
    pub(super) table_column_order:
        std::collections::BTreeMap<crate::tui::table_layout::TableKind, Vec<usize>>,
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
            selected: 0,
            item_count: 0,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            search_query: String::new(),
            search_matches: Vec::new(),
            search_cursor: 0,
            wizard: None,
            pinned_disk: None,
            disk_layout_tail: crate::tui::disk_layout::TailExpansion::Collapsed,
            disk_layout_selected: 0,
            horizontal_scroll: std::collections::BTreeMap::new(),
            table_column_order: std::collections::BTreeMap::new(),
        }
    }
}
