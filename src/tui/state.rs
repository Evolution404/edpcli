//! Pure TUI state machine.
//!
//! The state layer never performs I/O. That makes navigation and cancellation semantics testable
//! without a real terminal and keeps critical-operation policy independent from crossterm.

#[path = "backups/panel_state.rs"]
mod backup_panel_state;
#[path = "backups/view_state.rs"]
mod backup_view_state;
#[path = "backups/state.rs"]
mod backups_state;
#[path = "devices/state.rs"]
mod devices_state;
#[path = "disk_layout_state.rs"]
mod disk_layout_state;
#[path = "inspect/state.rs"]
mod inspect_state;
#[path = "navigation.rs"]
mod navigation;
#[path = "navigation_state.rs"]
mod navigation_state;
#[path = "shell/notice_state.rs"]
mod notice_state;
#[path = "post_restore_progress_state.rs"]
mod post_restore_progress_state;
#[path = "provision/state.rs"]
mod provision_state;
#[path = "restore_completion_state.rs"]
mod restore_completion_state;
#[path = "restore_followup_state.rs"]
mod restore_followup_state;
#[path = "restore_result_state.rs"]
mod restore_result_state;
#[path = "restore/state.rs"]
mod restore_state;
#[path = "result_partition_table_state.rs"]
mod result_partition_table_state;
#[path = "shell/state.rs"]
mod shell_state;
#[path = "table_state.rs"]
mod table_state;

pub use backups_state::*;
pub use devices_state::*;
pub use inspect_state::*;
pub use navigation::*;
pub use provision_state::*;
pub use restore_state::*;
pub use shell_state::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workspace {
    Devices,
    Inspect,
    Backups,
    Provision,
}

impl Workspace {
    pub const ALL: [Self; 4] = [Self::Devices, Self::Inspect, Self::Backups, Self::Provision];
    pub const TOP_LEVEL: [Self; 2] = [Self::Devices, Self::Backups];

    pub fn shifted(self, reverse: bool) -> Self {
        let Some(index) = Self::TOP_LEVEL.iter().position(|value| *value == self) else {
            return self;
        };
        Self::TOP_LEVEL[if reverse {
            (index + Self::TOP_LEVEL.len() - 1) % Self::TOP_LEVEL.len()
        } else {
            (index + 1) % Self::TOP_LEVEL.len()
        }]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Insert,
    Search,
    Command,
    Confirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavCommand {
    Up,
    Down,
    Top,
    Bottom,
    HalfPageDown,
    HalfPageUp,
    PageUp,
    PageDown,
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
    BeginBackupDelete,
    ToggleBackupSelection,
    BeginBackupBatchDelete,
    BeginBackupPrune,
    VerifyBackup,
    OpenInspect,
    NextWorkspace,
    PreviousWorkspace,
    WorkspaceDevices,
    WorkspaceInspect,
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
    shell: ShellState,
    restore: RestoreState,
    devices: DevicesState,
    inspect: InspectState,
    backups: BackupsState,
    provision: ProvisionState,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            shell: ShellState::default(),
            restore: RestoreState::default(),
            devices: DevicesState::default(),
            inspect: InspectState::default(),
            backups: BackupsState::default(),
            provision: ProvisionState::default(),
        }
    }

    pub const fn animation_frame(&self) -> u64 {
        self.shell.animation_frame
    }

    pub const fn is_demo(&self) -> bool {
        self.shell.demo_mode
    }

    pub fn backup_verify_run(&self) -> Option<&BackupVerifyRunState> {
        self.backups.verify_run.as_ref()
    }

    pub fn set_backup_verify_run(&mut self, run: Option<BackupVerifyRunState>) {
        self.backups.verify_run = run;
    }

    pub fn begin_backup_verify_run(&mut self, path: std::path::PathBuf) {
        use crate::application::progress::{OperationKind, Phase, ProgressEvent, Step};
        let mut event = ProgressEvent::new(Phase::Readback, Step::BackupVerification, 0, 1);
        event.operation = OperationKind::Backup;
        event.detail = Some("正在校验备份大小与 SHA-256".into());
        self.backups.verify_run = Some(BackupVerifyRunState {
            path,
            latest: event.clone(),
            log: std::collections::VecDeque::from([event]),
        });
    }

    pub(crate) fn set_demo_mode(&mut self) {
        self.shell.demo_mode = true;
    }

    pub fn advance_animation(&mut self) {
        self.shell.animation_frame = self.shell.animation_frame.wrapping_add(1);
    }

    pub fn input_buffer(&self) -> &str {
        &self.shell.input_buffer
    }

    pub fn push_input_char(&mut self, ch: char) {
        if matches!(
            self.shell.input_mode,
            InputMode::Search | InputMode::Command
        ) && self.shell.input_buffer.chars().count() < 256
            && !ch.is_control()
        {
            self.shell.input_buffer.push(ch);
            if self.shell.input_mode == InputMode::Search {
                self.rebuild_workspace_filter();
            }
        }
    }

    pub fn backspace_input(&mut self) {
        if matches!(
            self.shell.input_mode,
            InputMode::Search | InputMode::Command
        ) {
            self.shell.input_buffer.pop();
            if self.shell.input_mode == InputMode::Search {
                self.rebuild_workspace_filter();
            }
        }
    }

    pub fn take_input(&mut self) -> String {
        std::mem::take(&mut self.shell.input_buffer)
    }

    pub fn cancel_input(&mut self) {
        let was_search = self.shell.input_mode == InputMode::Search;
        self.shell.input_buffer.clear();
        self.shell.input_mode = InputMode::Normal;
        if was_search {
            self.rebuild_workspace_filter();
        }
    }

    fn clear_search_matches(&mut self) {
        self.shell.search_matches.clear();
        self.shell.search_cursor = 0;
    }

    fn active_search_query(&self) -> &str {
        if self.shell.input_mode == InputMode::Search {
            self.shell.input_buffer.trim()
        } else {
            self.shell.search_query.as_str()
        }
    }

    fn rebuild_workspace_filter(&mut self) {
        let query = self.active_search_query().to_ascii_lowercase();
        self.clear_search_matches();

        if query.is_empty() {
            let count = match self.shell.workspace {
                Workspace::Devices => self.devices.rows.len(),
                Workspace::Inspect => 0,
                Workspace::Backups => self.backup_device_filtered_count(),
                Workspace::Provision => ProvisionKind::ALL.len(),
            };
            self.shell.selected = 0;
            self.set_item_count(count);
            return;
        }

        match self.shell.workspace {
            Workspace::Devices => {
                for (index, text) in self.devices.search_texts.iter().enumerate() {
                    if text.contains(&query) {
                        self.shell.search_matches.push(index);
                    }
                }
            }
            Workspace::Backups => {
                for (index, text) in self.backups.search_texts.iter().enumerate() {
                    if text.contains(&query) {
                        self.shell.search_matches.push(index);
                    }
                }
            }
            Workspace::Inspect => {}
            Workspace::Provision => {}
        }
        self.shell.selected = 0;
        let count = if self.shell.workspace == Workspace::Backups {
            self.visible_backup_indices().len()
        } else {
            self.shell.search_matches.len()
        };
        self.set_item_count(count);
    }

    fn activate_search_match(&mut self, match_index: usize) {
        if self.shell.search_matches.get(match_index).is_none() {
            return;
        }
        self.shell.selected = match_index.min(self.shell.item_count.saturating_sub(1));
    }

    pub fn submit_search(&mut self) -> usize {
        self.shell.search_query = self.shell.input_buffer.trim().to_ascii_lowercase();
        self.shell.input_buffer.clear();
        self.shell.input_mode = InputMode::Normal;
        self.rebuild_workspace_filter();
        self.shell.search_matches.len()
    }

    fn cycle_search(&mut self, reverse: bool) {
        if self.shell.search_matches.is_empty() || !self.workspace_filter_active() {
            return;
        }
        self.shell.selected = if reverse {
            if self.shell.selected == 0 {
                self.shell.item_count.saturating_sub(1)
            } else {
                self.shell.selected - 1
            }
        } else {
            (self.shell.selected + 1) % self.shell.item_count.max(1)
        };
        self.shell.search_cursor = self.shell.selected;
        self.activate_search_match(self.shell.search_cursor);
        if self.shell.workspace == Workspace::Devices {
            self.reconcile_device_info_selection();
        }
    }

    pub fn search_status(&self) -> Option<String> {
        (!self.shell.search_query.is_empty()).then(|| {
            format!(
                "/{}  {} 条结果",
                self.shell.search_query,
                self.shell.search_matches.len()
            )
        })
    }

    pub fn notice(&self) -> Option<&str> {
        self.shell
            .notice_at
            .filter(|at| at.elapsed() < std::time::Duration::from_secs(4))
            .and(self.shell.notice.as_deref())
    }

    pub fn notice_message(&self) -> Option<&crate::tui::ui::UiMessage> {
        self.shell
            .notice_at
            .filter(|at| at.elapsed() < std::time::Duration::from_secs(4))
            .and(self.shell.notice.as_ref())
    }

    pub fn set_notice(&mut self, message: impl Into<String>) {
        self.shell.notice = Some(crate::tui::ui::UiMessage::info(message));
        self.shell.notice_at = Some(std::time::Instant::now());
    }

    pub fn set_progress_notice(&mut self, message: impl Into<String>) {
        self.shell.notice = Some(crate::tui::ui::UiMessage::progress(message));
        self.shell.notice_at = Some(std::time::Instant::now());
    }

    pub fn set_success_notice(&mut self, message: impl Into<String>) {
        self.shell.notice = Some(crate::tui::ui::UiMessage::success(message));
        self.shell.notice_at = Some(std::time::Instant::now());
    }

    pub fn set_warning_notice(&mut self, message: impl Into<String>) {
        self.shell.notice = Some(crate::tui::ui::UiMessage::warning(message));
        self.shell.notice_at = Some(std::time::Instant::now());
    }

    pub fn set_error_notice(&mut self, message: impl Into<String>) {
        self.shell.notice = Some(crate::tui::ui::UiMessage::error(message));
        self.shell.notice_at = Some(std::time::Instant::now());
    }

    pub fn clear_notice(&mut self) {
        self.shell.notice = None;
        self.shell.notice_at = None;
    }

    pub fn devices(&self) -> &[crate::disk_scan::Row] {
        &self.devices.rows
    }

    pub const fn device_scan_pending(&self) -> bool {
        self.devices.scan_pending
    }

    pub const fn backup_scan_pending(&self) -> bool {
        self.backups.scan_pending
    }

    pub const fn active_scan_pending(&self) -> bool {
        match self.shell.workspace {
            Workspace::Devices => self.devices.scan_pending,
            Workspace::Backups => self.backups.scan_pending,
            Workspace::Inspect => false,
            Workspace::Provision => false,
        }
    }

    pub fn set_device_scan_pending(&mut self, pending: bool) {
        self.devices.scan_pending = pending;
    }

    pub fn replace_devices(&mut self, devices: Vec<crate::disk_scan::Row>) {
        let selected_disk = (self.shell.workspace == Workspace::Devices)
            .then(|| self.selected_device_disk())
            .flatten();
        let preserve_provision_result = self.shell.workspace == Workspace::Provision
            && self.provision.stage == ProvisionStage::Result;
        if !preserve_provision_result
            && self
                .shell
                .pinned_disk
                .is_some_and(|disk| !devices.iter().any(|row| row.disk == disk))
        {
            self.shell.pinned_disk = None;
        }
        if !preserve_provision_result
            && self
                .provision
                .target_disk
                .is_some_and(|disk| !devices.iter().any(|row| row.disk == disk))
        {
            self.provision.target_disk = None;
        }
        self.devices.rows = devices;
        let (view, searches) = super::table_layout::device_table_view_with_search(
            &self.devices.rows,
            self.devices.table_view.generation.wrapping_add(1),
        );
        self.devices.table_view = view;
        self.devices.search_texts = searches;
        self.devices.scan_pending = false;
        if self.shell.workspace == Workspace::Provision
            && self.provision.stage != ProvisionStage::Result
            && (self.provision.target_disk.is_none() || self.selected_device().is_none())
        {
            self.provision.target_disk = None;
            self.shell.pinned_disk = None;
            self.provision_reset();
            self.restore_workspace_frame();
            self.set_warning_notice("制盘目标设备已断开，已安全返回设备列表。");
        }
        if self.shell.workspace == Workspace::Devices {
            self.rebuild_workspace_filter();
            if let Some(disk) = selected_disk {
                let source_index = self.devices.rows.iter().position(|row| row.disk == disk);
                let visible = self.visible_device_indices();
                self.shell.selected = source_index
                    .and_then(|index| visible.iter().position(|value| *value == index))
                    .unwrap_or(0);
            }
            self.reconcile_device_info_selection();
        }
    }

    pub fn backups(&self) -> &[crate::application::BackupWorkspaceItem] {
        &self.backups.rows
    }

    pub fn table_view_data(
        &self,
        kind: super::table_layout::TableKind,
    ) -> Option<&super::table_layout::TableViewData> {
        match kind {
            super::table_layout::TableKind::Devices => Some(&self.devices.table_view),
            super::table_layout::TableKind::Backups => Some(&self.backups.table_view),
            _ => None,
        }
    }

    pub fn device_source_index_at_visible(&self, position: usize) -> Option<usize> {
        self.device_view_snapshot().get(position).copied()
    }

    pub fn backup_source_index_at_visible(&self, position: usize) -> Option<usize> {
        self.backup_view_snapshot().indices.get(position).copied()
    }

    pub fn visible_device_indices(&self) -> Vec<usize> {
        self.device_view_snapshot().as_ref().clone()
    }

    pub fn visible_device_count(&self) -> usize {
        self.device_view_snapshot().len()
    }

    pub fn visible_backup_indices(&self) -> Vec<usize> {
        self.backup_view_snapshot().indices.clone()
    }

    pub fn visible_backup_count(&self) -> usize {
        self.backup_view_snapshot().indices.len()
    }

    pub fn backup_at_visible(
        &self,
        position: usize,
    ) -> Option<&crate::application::BackupWorkspaceItem> {
        let index = self.backup_source_index_at_visible(position)?;
        self.backups.rows.get(index)
    }

    pub fn workspace_filter_active(&self) -> bool {
        !self.active_search_query().is_empty()
    }

    pub fn selected_device(&self) -> Option<&crate::disk_scan::Row> {
        match self.shell.workspace {
            Workspace::Devices => {
                let index = self.device_source_index_at_visible(self.shell.selected)?;
                self.devices.rows.get(index)
            }
            Workspace::Backups | Workspace::Provision | Workspace::Inspect => self
                .shell
                .pinned_disk
                .and_then(|disk| self.devices.rows.iter().find(|row| row.disk == disk)),
        }
    }

    pub fn begin_provision_for_selected_device(&mut self) -> Result<u32, String> {
        if self.shell.workspace != Workspace::Devices
            || self.devices_focused_pane() != crate::tui::pane::PaneId::DevicesList
        {
            return Err("请先在设备列表选中目标 USB 盘。".into());
        }
        let row = self
            .selected_device()
            .ok_or_else(|| "请先选择目标 USB 盘。".to_string())?;
        if row.proto != "USB" || row.denied || row.probe_error.is_some() {
            return Err("制盘需要可读取的 USB 整盘目标。".into());
        }
        if row.confirmed_provision_kind().is_none() {
            return Err("当前盘型未确认；为避免把未知/损坏介质误当普通盘，拒绝进入制盘。".into());
        }
        let disk = row.disk;
        self.provision.target_disk = Some(disk);
        self.provision_transition_enter_form();
        self.shell.pinned_disk = Some(disk);
        self.provision.message = None;
        self.provision.scheme_picker_open = true;
        Ok(disk)
    }

    pub fn provision_enter_form_workspace(&mut self) {
        if self.shell.workspace == Workspace::Devices {
            self.push_navigation_frame(NavigationLocation::Devices);
            self.switch_workspace(Workspace::Provision);
        }
        self.provision.scheme_picker_open = false;
    }

    pub fn selected_device_disk(&self) -> Option<u32> {
        self.selected_device().map(|row| row.disk)
    }

    pub fn selected_backup_path(&self) -> Option<std::path::PathBuf> {
        self.selected_backup().map(|row| row.path.clone())
    }

    pub fn selected_backup(&self) -> Option<&crate::application::BackupWorkspaceItem> {
        if self.shell.workspace != Workspace::Backups {
            return None;
        }
        let index = self.backup_source_index_at_visible(self.shell.selected)?;
        self.backups.rows.get(index)
    }

    pub fn selected_backup_delete_target(&self) -> Option<(std::path::PathBuf, String)> {
        let row = self.selected_backup()?;
        Some((row.path.clone(), row.content_sha256.clone()?))
    }

    pub fn set_backup_scan_pending(&mut self, pending: bool) {
        self.backups.scan_pending = pending;
    }

    pub fn replace_backups(&mut self, backups: Vec<crate::application::BackupWorkspaceItem>) {
        let selected_path = (self.shell.workspace == Workspace::Backups)
            .then(|| self.selected_backup_path())
            .flatten();
        self.backups.overview_counts = crate::tui::overview::ProvisionKindCounts::from_kinds(
            backups.iter().map(|backup| backup.provision_kind),
        );
        self.backups.rows = backups;
        let (view, searches) = super::table_layout::backup_table_view_with_search(
            &self.backups.rows,
            self.backups.table_view.generation.wrapping_add(1),
        );
        self.backups.table_view = view;
        self.backups.search_texts = searches;
        self.backups.group_keys = self
            .backups
            .rows
            .iter()
            .map(Self::backup_strong_group_key)
            .collect();
        self.backups.tree_nodes = self.build_backup_device_tree_nodes();
        let selectable = self
            .backups
            .rows
            .iter()
            .filter(|row| row.content_sha256.is_some())
            .map(|row| row.path.clone())
            .collect::<std::collections::BTreeSet<_>>();
        self.backups
            .selection
            .retain(|path| selectable.contains(path));
        self.backups.scan_pending = false;
        if self.shell.workspace == Workspace::Backups {
            self.rebuild_workspace_filter();
            self.reconcile_backup_device_filter();
            if let Some(path) = selected_path {
                let source_index = self.backups.rows.iter().position(|row| row.path == path);
                let visible = self.visible_backup_indices();
                self.shell.selected = source_index
                    .and_then(|index| visible.iter().position(|value| *value == index))
                    .unwrap_or(0);
                self.set_item_count(visible.len());
            } else {
                self.set_item_count(self.visible_backup_indices().len());
            }
        }
    }

    pub const fn item_count(&self) -> usize {
        self.shell.item_count
    }

    pub const fn input_mode(&self) -> InputMode {
        self.shell.input_mode
    }

    pub const fn help_open(&self) -> bool {
        self.shell.help_open
    }

    pub const fn is_critical_operation(&self) -> bool {
        self.shell.critical_operation
    }

    pub const fn exit_pending(&self) -> bool {
        self.shell.exit_pending
    }

    pub fn set_item_count(&mut self, item_count: usize) {
        self.shell.item_count = item_count;
        if item_count == 0 {
            self.shell.selected = 0;
        } else {
            self.shell.selected = self.shell.selected.min(item_count - 1);
        }
    }

    pub fn set_critical_operation(&mut self, critical: bool) {
        self.shell.critical_operation = critical;
    }

    pub fn take_deferred_exit(&mut self) -> StateEffect {
        if !self.shell.critical_operation && self.shell.exit_pending {
            self.shell.exit_pending = false;
            StateEffect::ExitRequested
        } else {
            StateEffect::None
        }
    }

    /// Enforce the one global command policy used while a destructive or otherwise
    /// critical worker owns the operation slot. Every command entry point (keys,
    /// command palette and direct dispatch) must pass through this guard.
    pub fn guard_critical_command(&mut self, command: NavCommand) -> Option<StateEffect> {
        if !self.shell.critical_operation {
            return None;
        }
        if command == NavCommand::Help {
            return None;
        }
        if matches!(
            command,
            NavCommand::NextWorkspace
                | NavCommand::PreviousWorkspace
                | NavCommand::WorkspaceDevices
                | NavCommand::WorkspaceInspect
                | NavCommand::WorkspaceBackups
                | NavCommand::WorkspaceProvision
        ) {
            return None;
        }
        if command == NavCommand::Quit {
            self.shell.exit_pending = true;
            Some(StateEffect::ExitDeferred)
        } else if command == NavCommand::Escape {
            self.set_warning_notice(
                "关键操作仍在执行，当前不能返回；操作完成后再按 Esc 返回。".to_string(),
            );
            Some(StateEffect::None)
        } else {
            self.set_warning_notice(
                "关键操作仍在执行，完成前不能执行该命令或启动其他任务。".to_string(),
            );
            Some(StateEffect::None)
        }
    }

    pub fn navigate(&mut self, command: NavCommand, viewport_height: usize) -> StateEffect {
        if let Some(effect) = self.guard_critical_command(command) {
            return effect;
        }

        if command == NavCommand::Escape {
            if self.shell.help_open {
                self.shell.help_open = false;
                return StateEffect::None;
            }
            if self.provision.scheme_picker_open {
                self.provision_close_scheme_picker();
                return StateEffect::None;
            }
            if self.backups.delete.is_some() {
                self.backups.delete = None;
                self.shell.input_mode = InputMode::Normal;
                return StateEffect::None;
            }
            if self.backups.batch_delete.is_some() {
                self.close_backup_batch_delete();
                return StateEffect::None;
            }
            if self.backups.prune.is_some() {
                self.close_backup_prune();
                return StateEffect::None;
            }
            if let Some(stage) = self.restore.wizard.as_ref().map(|wizard| wizard.stage) {
                match stage {
                    WizardStage::Confirm => {
                        self.restore.wizard = None;
                        self.shell.input_mode = InputMode::Normal;
                    }
                    WizardStage::FormatConfirm => self.cancel_post_restore_format(),
                    WizardStage::VolumeLabelInput => self.cancel_post_restore_volume_label(),
                    WizardStage::PasswordInput
                    | WizardStage::EncryptedFormatConfirm
                    | WizardStage::ReinitializePassword
                    | WizardStage::ReinitializePasswordConfirm
                    | WizardStage::ReinitializeConfirm => self.cancel_post_restore_secret_flow(),
                    WizardStage::PostRestore | WizardStage::Result => {
                        self.restore.wizard = None;
                        self.shell.input_mode = InputMode::Normal;
                    }
                    WizardStage::Running
                    | WizardStage::Formatting
                    | WizardStage::Reinitializing => {
                        self.set_warning_notice("关键操作正在执行，当前不能关闭。".to_string());
                    }
                }
                return StateEffect::None;
            }
            if let Some(advanced) = self
                .inspect
                .advanced
                .as_ref()
                .filter(|_| self.shell.workspace == Workspace::Inspect)
            {
                if advanced.stage == AdvancedInspectStage::Running {
                    self.set_progress_notice("全盘检查正在后台读取结构，请等待完成。");
                } else if advanced.prompt.is_some() {
                    self.advanced_inspect_cancel_prompt();
                } else if advanced.sector.is_some() && self.advanced_inspect_close_sector() {
                } else {
                    self.close_advanced_inspect();
                }
                return StateEffect::None;
            }
            if self.shell.workspace == Workspace::Inspect && self.inspect.advanced.is_none() {
                self.restore_workspace_frame();
                return StateEffect::None;
            }
            if self.shell.workspace == Workspace::Provision {
                match self.provision.stage {
                    ProvisionStage::Running => {
                        self.set_warning_notice("制盘安全事务正在执行，当前不能返回。");
                    }
                    ProvisionStage::Confirm => {
                        self.provision_transition_return_to_review();
                        self.provision.confirmation.clear();
                    }
                    ProvisionStage::Review => self.provision_return_review_to_form(),
                    ProvisionStage::ExportPath => self.provision_cancel_export(),
                    ProvisionStage::Exporting => {
                        self.set_progress_notice("镜像正在后台导出，请等待完成。");
                    }
                    ProvisionStage::Form | ProvisionStage::Result => {
                        self.provision_reset();
                        self.restore_workspace_frame();
                    }
                    ProvisionStage::Planning => {
                        self.set_progress_notice("制盘计划正在后台生成，请等待完成。");
                    }
                }
                return StateEffect::None;
            }
            if self.shell.workspace == Workspace::Devices {
                match self.devices.pane_focus.focused() {
                    crate::tui::pane::PaneId::DevicesDetail => {
                        self.devices
                            .pane_focus
                            .focus(crate::tui::pane::PaneId::DevicesTree);
                        return StateEffect::None;
                    }
                    crate::tui::pane::PaneId::DevicesTree => {
                        self.devices
                            .pane_focus
                            .focus(crate::tui::pane::PaneId::DevicesList);
                        return StateEffect::None;
                    }
                    _ => {}
                }
            }
            if self.shell.workspace == Workspace::Backups
                && self.backups.pane_focus.focused() != crate::tui::pane::PaneId::BackupsList
            {
                self.backups
                    .pane_focus
                    .focus(crate::tui::pane::PaneId::BackupsList);
                return StateEffect::None;
            }
            if self.shell.input_mode != InputMode::Normal {
                self.cancel_input();
                return StateEffect::None;
            }
            return StateEffect::None;
        }

        if command == NavCommand::Quit {
            return StateEffect::ExitRequested;
        }

        if self.provision.scheme_picker_open {
            match command {
                NavCommand::Up => self.provision_move_scheme_picker(-1),
                NavCommand::Down => self.provision_move_scheme_picker(1),
                NavCommand::Top => {
                    self.provision_select_scheme_index(0);
                }
                NavCommand::Bottom => {
                    self.provision_select_scheme_index(ProvisionKind::ALL.len().saturating_sub(1));
                }
                _ => {}
            }
            if matches!(
                command,
                NavCommand::Up | NavCommand::Down | NavCommand::Top | NavCommand::Bottom
            ) {
                return StateEffect::None;
            }
        }

        if command == NavCommand::NextMatch {
            self.cycle_search(false);
            return StateEffect::None;
        }
        if command == NavCommand::PreviousMatch {
            self.cycle_search(true);
            return StateEffect::None;
        }

        if self.navigate_device_capacity_regions(command) {
            return StateEffect::None;
        }
        if self.navigate_backup_panel(command) {
            return StateEffect::None;
        }
        match command {
            NavCommand::NextWorkspace | NavCommand::PreviousWorkspace => {
                self.switch_workspace(
                    self.shell
                        .workspace
                        .shifted(command == NavCommand::PreviousWorkspace),
                );
            }
            NavCommand::WorkspaceDevices => self.switch_workspace(Workspace::Devices),
            NavCommand::WorkspaceInspect => self.switch_workspace(Workspace::Inspect),
            NavCommand::WorkspaceBackups => self.switch_workspace(Workspace::Backups),
            NavCommand::WorkspaceProvision => self.switch_workspace(Workspace::Provision),
            NavCommand::Up => {
                if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesDetail
                    && matches!(
                        self.device_info_selected_key(),
                        DeviceInfoNodeKey::Status | DeviceInfoNodeKey::Backups
                    )
                    && !self.device_related_backups().is_empty()
                {
                    self.device_related_backup_move(-1);
                } else if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesTree
                {
                    self.device_info_move_tree(-1);
                } else {
                    let detail_pane = match self.shell.workspace {
                        Workspace::Devices
                            if self.devices_focused_pane()
                                != crate::tui::pane::PaneId::DevicesList =>
                        {
                            Some(self.devices_focused_pane())
                        }
                        Workspace::Backups
                            if self.backups_focused_pane()
                                != crate::tui::pane::PaneId::BackupsList =>
                        {
                            Some(self.backups_focused_pane())
                        }
                        _ => None,
                    };
                    if let Some(pane) = detail_pane {
                        self.pane_viewport_mut(pane).scroll_y.line_up();
                    } else {
                        self.shell.selected = self.shell.selected.saturating_sub(1);
                        if self.shell.workspace == Workspace::Devices {
                            self.reconcile_device_info_selection();
                        }
                    }
                }
            }
            NavCommand::Down => {
                if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesDetail
                    && matches!(
                        self.device_info_selected_key(),
                        DeviceInfoNodeKey::Status | DeviceInfoNodeKey::Backups
                    )
                    && !self.device_related_backups().is_empty()
                {
                    self.device_related_backup_move(1);
                } else if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesTree
                {
                    self.device_info_move_tree(1);
                } else {
                    let detail_pane = match self.shell.workspace {
                        Workspace::Devices
                            if self.devices_focused_pane()
                                != crate::tui::pane::PaneId::DevicesList =>
                        {
                            Some(self.devices_focused_pane())
                        }
                        Workspace::Backups
                            if self.backups_focused_pane()
                                != crate::tui::pane::PaneId::BackupsList =>
                        {
                            Some(self.backups_focused_pane())
                        }
                        _ => None,
                    };
                    if let Some(pane) = detail_pane {
                        let content_len = match pane {
                            crate::tui::pane::PaneId::DevicesDetail => {
                                self.device_info_detail_line_count()
                            }
                            crate::tui::pane::PaneId::BackupCoverage => self
                                .selected_backup()
                                .and_then(|backup| backup.coverage.as_ref())
                                .map(|coverage| coverage.regions.len() * 2 + 5)
                                .unwrap_or(5),
                            _ => 28,
                        };
                        self.pane_viewport_mut(pane)
                            .scroll_y
                            .line_down(content_len, viewport_height);
                    } else if self.shell.item_count > 0 {
                        self.shell.selected =
                            (self.shell.selected + 1).min(self.shell.item_count - 1);
                        if self.shell.workspace == Workspace::Devices {
                            self.reconcile_device_info_selection();
                        }
                    }
                }
            }
            NavCommand::Top
                if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesTree =>
            {
                self.device_info_jump_tree(false);
            }
            NavCommand::Bottom
                if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesTree =>
            {
                self.device_info_jump_tree(true);
            }
            NavCommand::Top
                if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesDetail =>
            {
                if matches!(
                    self.device_info_selected_key(),
                    DeviceInfoNodeKey::Status | DeviceInfoNodeKey::Backups
                ) && !self.device_related_backups().is_empty()
                {
                    self.device_related_backup_jump(false);
                } else {
                    self.pane_viewport_mut(crate::tui::pane::PaneId::DevicesDetail)
                        .scroll_y
                        .top();
                }
            }
            NavCommand::Bottom
                if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesDetail =>
            {
                if matches!(
                    self.device_info_selected_key(),
                    DeviceInfoNodeKey::Status | DeviceInfoNodeKey::Backups
                ) && !self.device_related_backups().is_empty()
                {
                    self.device_related_backup_jump(true);
                } else {
                    let content_len = self.device_info_detail_line_count();
                    self.pane_viewport_mut(crate::tui::pane::PaneId::DevicesDetail)
                        .scroll_y
                        .bottom(content_len, viewport_height);
                }
            }
            NavCommand::HalfPageDown | NavCommand::PageDown
                if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesDetail =>
            {
                let content_len = self.device_info_detail_line_count();
                self.pane_viewport_mut(crate::tui::pane::PaneId::DevicesDetail)
                    .scroll_y
                    .page_down(
                        content_len,
                        if command == NavCommand::PageDown {
                            viewport_height
                        } else {
                            (viewport_height / 2).max(1)
                        },
                    );
            }
            NavCommand::HalfPageUp | NavCommand::PageUp
                if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesDetail =>
            {
                self.pane_viewport_mut(crate::tui::pane::PaneId::DevicesDetail)
                    .scroll_y
                    .page_up(if command == NavCommand::PageUp {
                        viewport_height
                    } else {
                        (viewport_height / 2).max(1)
                    });
            }
            NavCommand::Top => {
                self.shell.selected = 0;
                if self.shell.workspace == Workspace::Devices {
                    self.reconcile_device_info_selection();
                }
            }
            NavCommand::Bottom => {
                self.shell.selected = self.shell.item_count.saturating_sub(1);
                if self.shell.workspace == Workspace::Devices {
                    self.reconcile_device_info_selection();
                }
            }
            NavCommand::HalfPageDown | NavCommand::PageDown => {
                if self.shell.item_count > 0 {
                    let delta = if matches!(command, NavCommand::PageDown | NavCommand::PageUp) {
                        viewport_height.max(1)
                    } else {
                        (viewport_height / 2).max(1)
                    };
                    self.shell.selected = self
                        .shell
                        .selected
                        .saturating_add(delta)
                        .min(self.shell.item_count - 1);
                    if self.shell.workspace == Workspace::Devices {
                        self.reconcile_device_info_selection();
                    }
                }
            }
            NavCommand::HalfPageUp | NavCommand::PageUp => {
                let delta = if matches!(command, NavCommand::PageDown | NavCommand::PageUp) {
                    viewport_height.max(1)
                } else {
                    (viewport_height / 2).max(1)
                };
                self.shell.selected = self.shell.selected.saturating_sub(delta);
                if self.shell.workspace == Workspace::Devices {
                    self.reconcile_device_info_selection();
                }
            }
            NavCommand::Search => {
                self.shell.input_buffer = self.shell.search_query.clone();
                self.shell.input_mode = InputMode::Search;
            }
            NavCommand::CommandPalette => {
                self.shell.input_buffer.clear();
                self.shell.input_mode = InputMode::Command;
            }
            NavCommand::Help => {
                self.shell.help_open = !self.shell.help_open;
                self.shell.help_scroll = 0;
            }
            NavCommand::Refresh
            | NavCommand::BeginRestore
            | NavCommand::BeginBackupCreate
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
