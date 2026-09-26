//! Pure TUI state machine.
//!
//! The state layer never performs I/O. That makes navigation and cancellation semantics testable
//! without a real terminal and keeps critical-operation policy independent from crossterm.

#[path = "backups/state.rs"]
mod backups_state;
#[path = "inspect/state.rs"]
mod inspect_state;
#[path = "navigation.rs"]
mod navigation;
#[path = "provision/state.rs"]
mod provision_state;

pub use backups_state::*;
pub use inspect_state::*;
pub use navigation::*;
pub use provision_state::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteKind {
    Restore,
    BackupCreate,
    BackupCreateDeep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardStage {
    Confirm,
    Running,
    Result,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedIdentity {
    pub onlyid: Option<String>,
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteIntent {
    pub kind: WriteKind,
    pub disk: u32,
    pub backup: Option<std::path::PathBuf>,
    pub expected_identity: Option<ExpectedIdentity>,
}

#[derive(Debug, Clone)]
pub struct WizardState {
    pub stage: WizardStage,
    pub kind: WriteKind,
    pub disk: u32,
    pub backup: Option<std::path::PathBuf>,
    pub expected_identity: Option<ExpectedIdentity>,
    pub confirmation: String,
    pub message: Option<String>,
    /// Running 阶段最新收到的类型化进度事件；渲染层映射为单行显示。
    pub progress: Option<crate::application::WriteEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workspace {
    Devices,
    Backups,
    Provision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Insert,
    Search,
    Command,
    Confirm,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavCommand {
    Up,
    Down,
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
    Refresh,
    BeginRestore,
    BeginBackupCreate,
    BeginBackupCreateDeep,
    BeginBackupDelete,
    ToggleBackupSelection,
    BeginBackupBatchDelete,
    BeginBackupPrune,
    VerifyBackup,
    OpenInspect,
    NextWorkspace,
    PreviousWorkspace,
    WorkspaceDevices,
    WorkspaceBackups,
    WorkspaceProvision,
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
    device_table_view: super::table_layout::TableViewData,
    backup_table_view: super::table_layout::TableViewData,
    device_scan_pending: bool,
    backup_scan_pending: bool,
    selected: usize,
    item_count: usize,
    input_mode: InputMode,
    critical_operation: bool,
    exit_pending: bool,
    wizard: Option<WizardState>,
    backup_delete: Option<BackupDeleteState>,
    backup_batch_delete: Option<BackupBatchDeleteState>,
    backup_selection: std::collections::BTreeSet<std::path::PathBuf>,
    backup_create_choice: Option<BackupCreateChoiceState>,
    backup_prune: Option<BackupPruneState>,
    provision: ProvisionState,
    pinned_disk: Option<u32>,
    advanced_inspect: Option<AdvancedInspectState>,
    navigation: NavigationStack,
    horizontal_scroll: std::collections::BTreeMap<
        super::table_layout::TableKind,
        super::table_layout::HorizontalScrollState,
    >,
    notice: Option<String>,
    notice_at: Option<std::time::Instant>,
    input_buffer: String,
    search_query: String,
    search_matches: Vec<usize>,
    search_cursor: usize,
    animation_frame: u64,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            workspace: Workspace::Devices,
            devices: Vec::new(),
            backups: Vec::new(),
            device_table_view: super::table_layout::TableViewData::default(),
            backup_table_view: super::table_layout::TableViewData::default(),
            device_scan_pending: false,
            backup_scan_pending: false,
            selected: 0,
            item_count: 0,
            input_mode: InputMode::Normal,
            critical_operation: false,
            exit_pending: false,
            wizard: None,
            backup_delete: None,
            backup_batch_delete: None,
            backup_selection: std::collections::BTreeSet::new(),
            backup_create_choice: None,
            backup_prune: None,
            provision: ProvisionState::default(),
            pinned_disk: None,
            advanced_inspect: None,
            navigation: NavigationStack::default(),
            horizontal_scroll: std::collections::BTreeMap::new(),
            notice: None,
            notice_at: None,
            input_buffer: String::new(),
            search_query: String::new(),
            search_matches: Vec::new(),
            search_cursor: 0,
            animation_frame: 0,
        }
    }

    pub const fn animation_frame(&self) -> u64 {
        self.animation_frame
    }

    pub fn advance_animation(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);
    }

    pub fn input_buffer(&self) -> &str {
        &self.input_buffer
    }

    pub fn push_input_char(&mut self, ch: char) {
        if matches!(self.input_mode, InputMode::Search | InputMode::Command)
            && self.input_buffer.chars().count() < 256
            && !ch.is_control()
        {
            self.input_buffer.push(ch);
            if self.input_mode == InputMode::Search {
                self.rebuild_workspace_filter();
            }
        }
    }

    pub fn backspace_input(&mut self) {
        if matches!(self.input_mode, InputMode::Search | InputMode::Command) {
            self.input_buffer.pop();
            if self.input_mode == InputMode::Search {
                self.rebuild_workspace_filter();
            }
        }
    }

    pub fn take_input(&mut self) -> String {
        std::mem::take(&mut self.input_buffer)
    }

    pub fn cancel_input(&mut self) {
        let was_search = self.input_mode == InputMode::Search;
        self.input_buffer.clear();
        self.input_mode = InputMode::Normal;
        if was_search {
            self.rebuild_workspace_filter();
        }
    }

    fn clear_search_matches(&mut self) {
        self.search_matches.clear();
        self.search_cursor = 0;
    }

    fn active_search_query(&self) -> &str {
        if self.input_mode == InputMode::Search {
            self.input_buffer.trim()
        } else {
            self.search_query.as_str()
        }
    }

    fn device_matches_query(row: &crate::disk_scan::Row, query: &str) -> bool {
        let identity = crate::application::identity::WorkspaceIdentity::from_device(row);
        let text = format!(
            "disk{} {} {} {}",
            row.disk,
            identity.search_text(),
            row.proto,
            identity
                .provision_kind
                .map(|kind| kind.full_name())
                .unwrap_or_default(),
        );
        text.to_ascii_lowercase().contains(query)
    }

    fn backup_matches_query(row: &crate::application::BackupWorkspaceItem, query: &str) -> bool {
        let identity = crate::application::identity::WorkspaceIdentity::from_backup(row);
        let text = format!(
            "{} {} {} {}",
            row.file_name,
            row.display_time,
            identity.search_text(),
            identity
                .provision_kind
                .map(|kind| kind.full_name())
                .unwrap_or_default(),
        );
        text.to_ascii_lowercase().contains(query)
    }

    fn rebuild_workspace_filter(&mut self) {
        let query = self.active_search_query().to_ascii_lowercase();
        self.clear_search_matches();

        if query.is_empty() {
            let count = match self.workspace {
                Workspace::Devices => self.devices.len(),
                Workspace::Backups => self.backups.len(),
                Workspace::Provision => ProvisionKind::ALL.len(),
            };
            self.selected = 0;
            self.set_item_count(count);
            return;
        }

        match self.workspace {
            Workspace::Devices => {
                for (index, row) in self.devices.iter().enumerate() {
                    if Self::device_matches_query(row, &query) {
                        self.search_matches.push(index);
                    }
                }
            }
            Workspace::Backups => {
                for (index, row) in self.backups.iter().enumerate() {
                    if Self::backup_matches_query(row, &query) {
                        self.search_matches.push(index);
                    }
                }
            }
            Workspace::Provision => {}
        }
        self.selected = 0;
        self.set_item_count(self.search_matches.len());
    }

    fn activate_search_match(&mut self, match_index: usize) {
        if self.search_matches.get(match_index).is_none() {
            return;
        }
        self.selected = match_index.min(self.item_count.saturating_sub(1));
    }

    pub fn submit_search(&mut self) -> usize {
        self.search_query = self.input_buffer.trim().to_ascii_lowercase();
        self.input_buffer.clear();
        self.input_mode = InputMode::Normal;
        self.rebuild_workspace_filter();
        self.search_matches.len()
    }

    fn cycle_search(&mut self, reverse: bool) {
        if self.search_matches.is_empty() || !self.workspace_filter_active() {
            return;
        }
        self.selected = if reverse {
            if self.selected == 0 {
                self.item_count.saturating_sub(1)
            } else {
                self.selected - 1
            }
        } else {
            (self.selected + 1) % self.item_count.max(1)
        };
        self.search_cursor = self.selected;
        self.activate_search_match(self.search_cursor);
    }

    pub fn search_status(&self) -> Option<String> {
        (!self.search_query.is_empty()).then(|| {
            format!(
                "/{}  {} 条结果",
                self.search_query,
                self.search_matches.len()
            )
        })
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice_at
            .filter(|at| at.elapsed() < std::time::Duration::from_secs(4))
            .and(self.notice.as_deref())
    }

    pub fn set_notice(&mut self, message: impl Into<String>) {
        self.notice = Some(message.into());
        self.notice_at = Some(std::time::Instant::now());
    }

    pub fn clear_notice(&mut self) {
        self.notice = None;
        self.notice_at = None;
    }

    pub fn wizard(&self) -> Option<&WizardState> {
        self.wizard.as_ref()
    }

    pub fn begin_write_wizard(
        &mut self,
        kind: WriteKind,
        disk: u32,
        backup: Option<std::path::PathBuf>,
    ) -> bool {
        self.begin_write_wizard_for_identity(kind, disk, backup, None)
    }

    pub fn begin_write_wizard_for_identity(
        &mut self,
        kind: WriteKind,
        disk: u32,
        backup: Option<std::path::PathBuf>,
        expected_identity: Option<ExpectedIdentity>,
    ) -> bool {
        if self.critical_operation {
            self.set_notice("关键操作仍在执行，完成前不能启动其他任务。".to_string());
            return false;
        }
        self.input_mode = InputMode::Confirm;
        self.wizard = Some(WizardState {
            stage: WizardStage::Confirm,
            kind,
            disk,
            backup,
            expected_identity,
            confirmation: String::new(),
            message: None,
            progress: None,
        });
        true
    }

    pub fn push_wizard_confirmation(&mut self, ch: char) {
        if let Some(wizard) = self.wizard.as_mut() {
            if wizard.stage == WizardStage::Confirm && wizard.confirmation.len() < 16 {
                wizard.confirmation.push(ch);
                wizard.message = None;
            }
        }
    }

    pub fn backspace_wizard_confirmation(&mut self) {
        if let Some(wizard) = self.wizard.as_mut() {
            if wizard.stage == WizardStage::Confirm {
                wizard.confirmation.pop();
                wizard.message = None;
            }
        }
    }

    pub fn clear_wizard_confirmation(&mut self) {
        if let Some(wizard) = self.wizard.as_mut() {
            wizard.confirmation.clear();
            wizard.message = None;
        }
    }

    pub fn submit_wizard_confirmation(&mut self) -> Option<WriteIntent> {
        let wizard = self.wizard.as_mut()?;
        if wizard.stage != WizardStage::Confirm {
            return None;
        }
        if wizard.confirmation != "YES" {
            wizard.message = Some("必须精确输入 YES 才会进入写盘阶段".to_string());
            return None;
        }
        let intent = WriteIntent {
            kind: wizard.kind,
            disk: wizard.disk,
            backup: wizard.backup.clone(),
            expected_identity: wizard.expected_identity.clone(),
        };
        wizard.stage = WizardStage::Running;
        wizard.message = Some("关键写盘阶段进行中，不可中断".to_string());
        self.input_mode = InputMode::Normal;
        self.critical_operation = true;
        Some(intent)
    }

    pub fn set_write_progress(&mut self, event: crate::application::WriteEvent) {
        if let Some(wizard) = self.wizard.as_mut() {
            if wizard.stage == WizardStage::Running {
                wizard.progress = Some(event);
            }
        }
    }

    pub fn finish_write(&mut self, result: Result<(), String>) {
        self.critical_operation = false;
        if let Some(wizard) = self.wizard.as_mut() {
            wizard.stage = WizardStage::Result;
            wizard.progress = None;
            wizard.message = Some(match result {
                Ok(())
                    if matches!(
                        wizard.kind,
                        WriteKind::BackupCreate | WriteKind::BackupCreateDeep
                    ) =>
                {
                    "备份创建完成；备份列表已刷新".to_string()
                }
                Ok(()) => "操作完成，安全链全部通过".to_string(),
                Err(message) => message,
            });
        }
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
            Workspace::Provision => false,
        }
    }

    pub fn set_device_scan_pending(&mut self, pending: bool) {
        self.device_scan_pending = pending;
    }

    pub fn replace_devices(&mut self, devices: Vec<crate::disk_scan::Row>) {
        let selected_disk = (self.workspace == Workspace::Devices)
            .then(|| self.selected_device_disk())
            .flatten();
        if self
            .pinned_disk
            .is_some_and(|disk| !devices.iter().any(|row| row.disk == disk))
        {
            self.pinned_disk = None;
        }
        if self
            .provision
            .target_disk
            .is_some_and(|disk| !devices.iter().any(|row| row.disk == disk))
        {
            self.provision.target_disk = None;
        }
        self.devices = devices;
        self.device_table_view = super::table_layout::device_table_view(
            &self.devices,
            self.device_table_view.generation.wrapping_add(1),
        );
        self.device_scan_pending = false;
        if self.workspace == Workspace::Provision {
            if self.pinned_disk.is_none() {
                self.provision.stage = ProvisionStage::SelectDisk;
                self.set_item_count(self.provision_selectable_devices().count());
            } else if self.selected_device().is_none() {
                self.pinned_disk = None;
                self.provision.stage = ProvisionStage::SelectDisk;
                self.set_item_count(self.provision_selectable_devices().count());
            }
        }
        if self.workspace == Workspace::Devices {
            self.rebuild_workspace_filter();
            if let Some(disk) = selected_disk {
                let source_index = self.devices.iter().position(|row| row.disk == disk);
                self.selected = source_index
                    .and_then(|index| {
                        if self.workspace_filter_active() {
                            self.search_matches.iter().position(|value| *value == index)
                        } else {
                            Some(index)
                        }
                    })
                    .unwrap_or(0);
            }
        }
    }

    pub fn backups(&self) -> &[crate::application::BackupWorkspaceItem] {
        &self.backups
    }

    pub fn table_view_data(
        &self,
        kind: super::table_layout::TableKind,
    ) -> Option<&super::table_layout::TableViewData> {
        match kind {
            super::table_layout::TableKind::Devices => Some(&self.device_table_view),
            super::table_layout::TableKind::Backups => Some(&self.backup_table_view),
            _ => None,
        }
    }

    pub fn device_source_index_at_visible(&self, position: usize) -> Option<usize> {
        let index =
            if self.workspace == Workspace::Devices && !self.active_search_query().is_empty() {
                *self.search_matches.get(position)?
            } else {
                position
            };
        (index < self.devices.len()).then_some(index)
    }

    pub fn backup_source_index_at_visible(&self, position: usize) -> Option<usize> {
        let index =
            if self.workspace == Workspace::Backups && !self.active_search_query().is_empty() {
                *self.search_matches.get(position)?
            } else {
                position
            };
        (index < self.backups.len()).then_some(index)
    }

    pub fn visible_device_indices(&self) -> Vec<usize> {
        if self.workspace == Workspace::Devices && !self.active_search_query().is_empty() {
            self.search_matches.clone()
        } else {
            (0..self.devices.len()).collect()
        }
    }

    pub fn visible_device_count(&self) -> usize {
        if self.workspace == Workspace::Devices && !self.active_search_query().is_empty() {
            self.search_matches.len()
        } else {
            self.devices.len()
        }
    }

    pub fn device_at_visible(&self, position: usize) -> Option<&crate::disk_scan::Row> {
        let index =
            if self.workspace == Workspace::Devices && !self.active_search_query().is_empty() {
                *self.search_matches.get(position)?
            } else {
                position
            };
        self.devices.get(index)
    }

    pub fn visible_backup_indices(&self) -> Vec<usize> {
        if self.workspace == Workspace::Backups && !self.active_search_query().is_empty() {
            self.search_matches.clone()
        } else {
            (0..self.backups.len()).collect()
        }
    }

    pub fn visible_backup_count(&self) -> usize {
        if self.workspace == Workspace::Backups && !self.active_search_query().is_empty() {
            self.search_matches.len()
        } else {
            self.backups.len()
        }
    }

    pub fn backup_at_visible(
        &self,
        position: usize,
    ) -> Option<&crate::application::BackupWorkspaceItem> {
        let index =
            if self.workspace == Workspace::Backups && !self.active_search_query().is_empty() {
                *self.search_matches.get(position)?
            } else {
                position
            };
        self.backups.get(index)
    }

    pub fn workspace_filter_active(&self) -> bool {
        !self.active_search_query().is_empty()
    }

    pub fn selected_device(&self) -> Option<&crate::disk_scan::Row> {
        match self.workspace {
            Workspace::Devices => {
                let index = if self.workspace_filter_active() {
                    *self.search_matches.get(self.selected)?
                } else {
                    self.selected
                };
                self.devices.get(index)
            }
            Workspace::Backups | Workspace::Provision => self
                .pinned_disk
                .and_then(|disk| self.devices.iter().find(|row| row.disk == disk)),
        }
    }

    fn provision_selectable_devices(&self) -> impl Iterator<Item = &crate::disk_scan::Row> {
        self.devices
            .iter()
            .filter(|row| row.proto == "USB" && !row.denied && row.probe_error.is_none())
    }

    pub fn provision_device_at(&self, index: usize) -> Option<&crate::disk_scan::Row> {
        self.provision_selectable_devices().nth(index)
    }

    pub fn provision_select_disk(&mut self) -> Option<u32> {
        if self.workspace != Workspace::Provision
            || self.provision.stage != ProvisionStage::SelectDisk
        {
            return None;
        }
        let disk = self.provision_device_at(self.selected)?.disk;
        self.pinned_disk = Some(disk);
        self.provision.target_disk = Some(disk);
        self.provision.stage = ProvisionStage::BackupPrompt;
        self.provision.message = None;
        self.selected = 0;
        self.set_item_count(2);
        Some(disk)
    }

    pub fn begin_provision_for_selected_device(&mut self) -> Result<u32, String> {
        if self.workspace != Workspace::Devices {
            return Err("请先在设备页选择目标 USB 盘。".into());
        }
        let row = self
            .selected_device()
            .ok_or_else(|| "请先选择目标 USB 盘。".to_string())?;
        if row.proto != "USB" || row.denied || row.probe_error.is_some() {
            return Err("制盘需要可读取的 USB 整盘目标。".into());
        }
        let disk = row.disk;
        self.push_navigation_frame(NavigationLocation::Devices);
        self.provision.target_disk = Some(disk);
        self.switch_workspace(Workspace::Provision);
        self.pinned_disk = Some(disk);
        self.provision.stage = ProvisionStage::BackupPrompt;
        self.provision.message = None;
        self.selected = 0;
        self.set_item_count(2);
        Ok(disk)
    }

    pub fn selected_device_disk(&self) -> Option<u32> {
        self.selected_device().map(|row| row.disk)
    }

    pub fn selected_backup_path(&self) -> Option<std::path::PathBuf> {
        self.selected_backup().map(|row| row.path.clone())
    }

    pub fn selected_backup(&self) -> Option<&crate::application::BackupWorkspaceItem> {
        if self.workspace != Workspace::Backups {
            return None;
        }
        let index = if self.workspace_filter_active() {
            *self.search_matches.get(self.selected)?
        } else {
            self.selected
        };
        self.backups.get(index)
    }

    pub fn selected_backup_delete_target(&self) -> Option<(std::path::PathBuf, String)> {
        let row = self.selected_backup()?;
        Some((row.path.clone(), row.content_sha256.clone()?))
    }

    pub fn set_backup_scan_pending(&mut self, pending: bool) {
        self.backup_scan_pending = pending;
    }

    pub fn replace_backups(&mut self, backups: Vec<crate::application::BackupWorkspaceItem>) {
        let selected_path = (self.workspace == Workspace::Backups)
            .then(|| self.selected_backup_path())
            .flatten();
        self.backups = backups;
        self.backup_table_view = super::table_layout::backup_table_view(
            &self.backups,
            self.backup_table_view.generation.wrapping_add(1),
        );
        let selectable = self
            .backups
            .iter()
            .filter(|row| row.content_sha256.is_some())
            .map(|row| row.path.clone())
            .collect::<std::collections::BTreeSet<_>>();
        self.backup_selection
            .retain(|path| selectable.contains(path));
        self.backup_scan_pending = false;
        if self.workspace == Workspace::Backups {
            self.rebuild_workspace_filter();
            if let Some(path) = selected_path {
                let source_index = self.backups.iter().position(|row| row.path == path);
                self.selected = source_index
                    .and_then(|index| {
                        if self.workspace_filter_active() {
                            self.search_matches.iter().position(|value| *value == index)
                        } else {
                            Some(index)
                        }
                    })
                    .unwrap_or(0);
            }
        }
    }

    fn switch_workspace(&mut self, workspace: Workspace) {
        if self.workspace == workspace {
            return;
        }
        if self.workspace == Workspace::Devices && workspace == Workspace::Backups {
            self.pinned_disk = self.selected_device().map(|row| row.disk);
        }
        if workspace == Workspace::Provision {
            if let Some(disk) = self.provision.target_disk {
                self.pinned_disk = Some(disk);
            } else {
                self.pinned_disk = None;
                self.provision.stage = ProvisionStage::SelectDisk;
                self.provision.message = None;
            }
        }
        self.clear_search_matches();
        self.search_query.clear();
        self.input_buffer.clear();
        if self.input_mode == InputMode::Search {
            self.input_mode = InputMode::Normal;
        }
        self.workspace = workspace;
        self.selected = 0;
        if workspace == Workspace::Devices {
            if let Some(disk) = self.provision.target_disk {
                self.selected = self
                    .devices
                    .iter()
                    .position(|row| row.disk == disk)
                    .unwrap_or(0);
            }
        }
        let count = match workspace {
            Workspace::Devices => self.devices.len(),
            Workspace::Backups => self.backups.len(),
            Workspace::Provision => match self.provision.stage {
                ProvisionStage::SelectDisk => self.provision_selectable_devices().count(),
                ProvisionStage::BackupPrompt | ProvisionStage::BackupSaving => 2,
                ProvisionStage::Menu => ProvisionKind::ALL.len(),
                ProvisionStage::Form
                | ProvisionStage::Planning
                | ProvisionStage::Review
                | ProvisionStage::ExportPath
                | ProvisionStage::Exporting
                | ProvisionStage::Confirm
                | ProvisionStage::Running
                | ProvisionStage::Result => 0,
            },
        };
        self.set_item_count(count);
    }

    pub const fn selected(&self) -> usize {
        self.selected
    }

    pub fn navigation(&self) -> &NavigationStack {
        &self.navigation
    }

    pub fn table_scroll_offset(&self, kind: super::table_layout::TableKind) -> usize {
        self.horizontal_scroll
            .get(&kind)
            .copied()
            .unwrap_or_default()
            .offset()
    }

    pub fn scroll_table(&mut self, kind: super::table_layout::TableKind, reverse: bool) -> bool {
        let layout = super::table_layout::layout_for(kind);
        let scroll = self.horizontal_scroll.entry(kind).or_default();
        if reverse {
            scroll.left()
        } else {
            scroll.right(&layout)
        }
    }

    pub fn pane_viewport(&self, pane: crate::tui::pane::PaneId) -> &crate::tui::pane::PaneViewport {
        if pane.is_inspect() {
            self.advanced_inspect
                .as_ref()
                .expect("Inspect pane requested without Inspect state")
                .pane_focus
                .viewport(pane)
        } else {
            self.provision.pane_focus.viewport(pane)
        }
    }

    pub fn pane_viewport_mut(
        &mut self,
        pane: crate::tui::pane::PaneId,
    ) -> &mut crate::tui::pane::PaneViewport {
        if pane.is_inspect() {
            self.advanced_inspect
                .as_mut()
                .expect("Inspect pane requested without Inspect state")
                .pane_focus
                .viewport_mut(pane)
        } else {
            self.provision.pane_focus.viewport_mut(pane)
        }
    }

    pub fn push_navigation_frame(&mut self, location: NavigationLocation) {
        let table_kind = match location {
            NavigationLocation::Devices => Some(super::table_layout::TableKind::Devices),
            NavigationLocation::Backups => Some(super::table_layout::TableKind::Backups),
            NavigationLocation::Provision => Some(super::table_layout::TableKind::ProvisionDevices),
            NavigationLocation::Inspect | NavigationLocation::SectorInspector => None,
        };
        let table_scroll = table_kind.map(|kind| (kind, self.table_scroll_offset(kind)));
        let pane_focus = match location {
            NavigationLocation::Provision => Some(self.provision.pane_focus.clone()),
            NavigationLocation::Inspect | NavigationLocation::SectorInspector => self
                .advanced_inspect
                .as_ref()
                .map(|state| state.pane_focus.clone()),
            NavigationLocation::Devices | NavigationLocation::Backups => None,
        };
        self.navigation.push(NavigationFrame {
            location,
            selection: self.selected,
            item_count: self.item_count,
            panel: None,
            tree_selection: 0,
            pane_focus,
            table_scroll,
        });
    }

    pub fn pop_navigation_frame(&mut self) -> Option<NavigationFrame> {
        self.navigation.pop()
    }

    fn restore_workspace_frame(&mut self) {
        if let Some(frame) = self.pop_navigation_frame() {
            let workspace = match frame.location {
                NavigationLocation::Devices => Workspace::Devices,
                NavigationLocation::Backups => Workspace::Backups,
                NavigationLocation::Provision => Workspace::Provision,
                NavigationLocation::Inspect | NavigationLocation::SectorInspector => return,
            };
            self.switch_workspace(workspace);
            self.selected = frame.selection.min(self.item_count.saturating_sub(1));
            if workspace == Workspace::Provision {
                if let Some(pane_focus) = frame.pane_focus {
                    self.provision.pane_focus = pane_focus;
                }
            }
            if let Some((kind, offset)) = frame.table_scroll {
                self.horizontal_scroll
                    .entry(kind)
                    .or_default()
                    .set_offset(offset, &super::table_layout::layout_for(kind));
            }
        } else {
            self.switch_workspace(Workspace::Devices);
        }
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

    /// Enforce the one global command policy used while a destructive or otherwise
    /// critical worker owns the operation slot. Every command entry point (keys,
    /// command palette and direct dispatch) must pass through this guard.
    pub fn guard_critical_command(&mut self, command: NavCommand) -> Option<StateEffect> {
        if !self.critical_operation {
            return None;
        }
        if command == NavCommand::Quit {
            self.exit_pending = true;
            Some(StateEffect::ExitDeferred)
        } else if command == NavCommand::Escape {
            self.set_notice(
                "关键操作仍在执行，当前不能返回；操作完成后再按 Esc 返回。".to_string(),
            );
            Some(StateEffect::None)
        } else {
            self.set_notice("关键操作仍在执行，完成前不能切换页面或启动其他任务。".to_string());
            Some(StateEffect::None)
        }
    }

    pub fn navigate(&mut self, command: NavCommand, viewport_height: usize) -> StateEffect {
        if let Some(effect) = self.guard_critical_command(command) {
            return effect;
        }

        if command == NavCommand::Escape {
            if let Some(advanced) = self.advanced_inspect.as_ref() {
                if advanced.stage == AdvancedInspectStage::Running {
                    self.set_notice("全盘检查正在后台读取结构，请等待完成。");
                } else if advanced.prompt.is_some() {
                    self.advanced_inspect_cancel_prompt();
                } else if !self.advanced_inspect_close_sector() {
                    self.close_advanced_inspect();
                }
                return StateEffect::None;
            }
            if self.workspace == Workspace::Provision {
                match self.provision.stage {
                    ProvisionStage::SelectDisk => {
                        self.restore_workspace_frame();
                    }
                    ProvisionStage::BackupPrompt => {
                        self.restore_workspace_frame();
                    }
                    ProvisionStage::Menu => {
                        self.provision.stage = ProvisionStage::BackupPrompt;
                        self.provision.message = None;
                        self.selected = 0;
                        self.set_item_count(2);
                    }
                    ProvisionStage::Running => {
                        self.set_notice("制盘安全事务正在执行，当前不能返回。");
                    }
                    ProvisionStage::Confirm => {
                        self.provision.stage = ProvisionStage::Review;
                        self.provision.confirmation.clear();
                    }
                    ProvisionStage::Review => {
                        self.provision.stage = ProvisionStage::Form;
                    }
                    ProvisionStage::ExportPath => self.provision_cancel_export(),
                    ProvisionStage::Exporting => {
                        self.set_notice("镜像正在后台导出，请等待完成。");
                    }
                    ProvisionStage::Form | ProvisionStage::Result => {
                        self.provision_reset();
                    }
                    ProvisionStage::Planning => {
                        self.set_notice("制盘计划正在后台生成，请等待完成。");
                    }
                    ProvisionStage::BackupSaving => {
                        self.set_notice("正在保存当前盘，请等待完成。");
                    }
                }
                return StateEffect::None;
            }
            if self.wizard.is_some() {
                self.wizard = None;
                self.input_mode = InputMode::Normal;
                return StateEffect::None;
            }
            if self.backup_delete.is_some() {
                self.backup_delete = None;
                self.input_mode = InputMode::Normal;
                return StateEffect::None;
            }
            if self.input_mode != InputMode::Normal {
                self.cancel_input();
                return StateEffect::None;
            }
            return StateEffect::None;
        }

        if command == NavCommand::Quit {
            return StateEffect::ExitRequested;
        }

        if command == NavCommand::NextMatch {
            self.cycle_search(false);
            return StateEffect::None;
        }
        if command == NavCommand::PreviousMatch {
            self.cycle_search(true);
            return StateEffect::None;
        }

        match command {
            NavCommand::NextWorkspace | NavCommand::PreviousWorkspace => {
                self.switch_workspace(match self.workspace {
                    Workspace::Devices if command == NavCommand::NextWorkspace => {
                        Workspace::Backups
                    }
                    Workspace::Backups if command == NavCommand::NextWorkspace => {
                        Workspace::Devices
                    }
                    Workspace::Provision if command == NavCommand::NextWorkspace => {
                        Workspace::Devices
                    }
                    Workspace::Devices => Workspace::Backups,
                    Workspace::Backups => Workspace::Devices,
                    Workspace::Provision => Workspace::Backups,
                });
            }
            NavCommand::WorkspaceDevices => self.switch_workspace(Workspace::Devices),
            NavCommand::WorkspaceBackups => self.switch_workspace(Workspace::Backups),
            NavCommand::WorkspaceProvision => self.switch_workspace(Workspace::Provision),
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
                    self.selected = self.selected.saturating_add(delta).min(self.item_count - 1);
                }
            }
            NavCommand::HalfPageUp => {
                let delta = (viewport_height / 2).max(1);
                self.selected = self.selected.saturating_sub(delta);
            }
            NavCommand::Search => {
                self.input_buffer = self.search_query.clone();
                self.input_mode = InputMode::Search;
            }
            NavCommand::CommandPalette => {
                self.input_buffer.clear();
                self.input_mode = InputMode::Command;
            }
            NavCommand::Help => self.input_mode = InputMode::Help,
            NavCommand::Refresh
            | NavCommand::BeginRestore
            | NavCommand::BeginBackupCreate
            | NavCommand::BeginBackupCreateDeep
            | NavCommand::BeginBackupDelete
            | NavCommand::ToggleBackupSelection
            | NavCommand::BeginBackupBatchDelete
            | NavCommand::BeginBackupPrune
            | NavCommand::VerifyBackup
            | NavCommand::OpenInspect
            | NavCommand::NextMatch
            | NavCommand::PreviousMatch
            | NavCommand::Escape
            | NavCommand::Quit => {}
        }
        StateEffect::None
    }
}
