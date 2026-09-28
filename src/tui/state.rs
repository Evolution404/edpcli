//! Pure TUI state machine.
//!
//! The state layer never performs I/O. That makes navigation and cancellation semantics testable
//! without a real terminal and keeps critical-operation policy independent from crossterm.

#[path = "backups/state.rs"]
mod backups_state;
#[path = "devices/state.rs"]
mod devices_state;
#[path = "inspect/state.rs"]
mod inspect_state;
#[path = "navigation.rs"]
mod navigation;
#[path = "shell/state.rs"]
mod shell_state;
#[path = "provision/state.rs"]
mod provision_state;

pub use backups_state::*;
pub use devices_state::*;
pub use inspect_state::*;
pub use navigation::*;
pub use shell_state::*;
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

pub type ExpectedIdentity = crate::application::media_identity::MediaIdentityResumePin;

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
    pub progress_log: std::collections::VecDeque<crate::application::WriteEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workspace {
    Devices,
    Inspect,
    Backups,
    Provision,
}

impl Workspace {
    pub const ALL: [Self; 4] = [Self::Devices, Self::Inspect, Self::Backups, Self::Provision];

    pub fn shifted(self, reverse: bool) -> Self {
        let index = Self::ALL
            .iter()
            .position(|value| *value == self)
            .unwrap_or(0);
        Self::ALL[if reverse {
            (index + Self::ALL.len() - 1) % Self::ALL.len()
        } else {
            (index + 1) % Self::ALL.len()
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
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceSummarySection {
    Identity,
    Capacity,
    Status,
    Backups,
    Protocol,
}

impl DeviceSummarySection {
    pub const ALL: [Self; 5] = [
        Self::Identity,
        Self::Capacity,
        Self::Status,
        Self::Backups,
        Self::Protocol,
    ];

    const fn bit(self) -> u8 {
        match self {
            Self::Identity => 1 << 0,
            Self::Capacity => 1 << 1,
            Self::Status => 1 << 2,
            Self::Backups => 1 << 3,
            Self::Protocol => 1 << 4,
        }
    }
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
    devices: DevicesState,
    backups: BackupsState,
    wizard: Option<WizardState>,
    provision: ProvisionState,
    pinned_disk: Option<u32>,
    inspect: InspectState,
    disk_layout_tail: super::disk_layout::TailExpansion,
    disk_layout_selected: usize,
    horizontal_scroll: std::collections::BTreeMap<
        super::table_layout::TableKind,
        super::table_layout::HorizontalScrollState,
    >,
    table_column_order: std::collections::BTreeMap<super::table_layout::TableKind, Vec<usize>>,
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
            devices: DevicesState::default(),
            backups: BackupsState::default(),
            wizard: None,
            provision: ProvisionState::default(),
            pinned_disk: None,
            inspect: InspectState::default(),
            disk_layout_tail: super::disk_layout::TailExpansion::Collapsed,
            disk_layout_selected: 0,
            horizontal_scroll: std::collections::BTreeMap::new(),
            table_column_order: std::collections::BTreeMap::new(),
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
        if matches!(self.shell.input_mode, InputMode::Search | InputMode::Command)
            && self.shell.input_buffer.chars().count() < 256
            && !ch.is_control()
        {
            self.shell.input_buffer.push(ch);
            if self.shell.input_mode == InputMode::Search {
                self.rebuild_workspace_filter();
            }
        }
    }

    pub fn backspace_input(&mut self) {
        if matches!(self.shell.input_mode, InputMode::Search | InputMode::Command) {
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
            let count = match self.shell.workspace {
                Workspace::Devices => self.devices.rows.len(),
                Workspace::Inspect => 0,
                Workspace::Backups => self.backups.rows.len(),
                Workspace::Provision => ProvisionKind::ALL.len(),
            };
            self.shell.selected = 0;
            self.set_item_count(count);
            return;
        }

        match self.shell.workspace {
            Workspace::Devices => {
                for (index, row) in self.devices.rows.iter().enumerate() {
                    if Self::device_matches_query(row, &query) {
                        self.shell.search_matches.push(index);
                    }
                }
            }
            Workspace::Backups => {
                for (index, row) in self.backups.rows.iter().enumerate() {
                    if Self::backup_matches_query(row, &query) {
                        self.shell.search_matches.push(index);
                    }
                }
            }
            Workspace::Inspect => {}
            Workspace::Provision => {}
        }
        self.shell.selected = 0;
        self.set_item_count(self.shell.search_matches.len());
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
        if self.shell.search_matches.is_empty() || !self.shell.workspace_filter_active() {
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
        self.shell.notice_at
            .filter(|at| at.elapsed() < std::time::Duration::from_secs(4))
            .and(self.shell.notice.as_deref())
    }

    pub fn set_notice(&mut self, message: impl Into<String>) {
        self.shell.notice = Some(message.into());
        self.shell.notice_at = Some(std::time::Instant::now());
    }

    pub fn clear_notice(&mut self) {
        self.shell.notice = None;
        self.shell.notice_at = None;
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
        if self.shell.critical_operation {
            self.set_notice("关键操作仍在执行，完成前不能启动其他任务。".to_string());
            return false;
        }
        self.shell.input_mode = InputMode::Confirm;
        self.wizard = Some(WizardState {
            stage: WizardStage::Confirm,
            kind,
            disk,
            backup,
            expected_identity,
            confirmation: String::new(),
            message: None,
            progress: None,
            progress_log: std::collections::VecDeque::new(),
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
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = true;
        Some(intent)
    }

    pub fn set_write_progress(&mut self, event: crate::application::WriteEvent) {
        if let Some(wizard) = self.wizard.as_mut() {
            if wizard.stage == WizardStage::Running {
                wizard.progress = Some(event.clone());
                if wizard.progress_log.len() == 200 {
                    wizard.progress_log.pop_front();
                }
                wizard.progress_log.push_back(event);
            }
        }
    }

    pub fn finish_write(&mut self, result: Result<(), String>) {
        self.shell.critical_operation = false;
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
        self.devices.rows = devices;
        self.devices.table_view = super::table_layout::device_table_view(
            &self.devices.rows,
            self.devices.table_view.generation.wrapping_add(1),
        );
        self.devices.scan_pending = false;
        if self.shell.workspace == Workspace::Provision {
            if self.pinned_disk.is_none() {
                self.provision.stage = ProvisionStage::SelectDisk;
                self.set_item_count(self.provision_selectable_devices().count());
            } else if self.selected_device().is_none() {
                self.pinned_disk = None;
                self.provision.stage = ProvisionStage::SelectDisk;
                self.set_item_count(self.provision_selectable_devices().count());
            }
        }
        if self.shell.workspace == Workspace::Devices {
            self.rebuild_workspace_filter();
            if let Some(disk) = selected_disk {
                let source_index = self.devices.rows.iter().position(|row| row.disk == disk);
                self.shell.selected = source_index
                    .and_then(|index| {
                        if self.shell.workspace_filter_active() {
                            self.shell.search_matches.iter().position(|value| *value == index)
                        } else {
                            Some(index)
                        }
                    })
                    .unwrap_or(0);
            }
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
            super::table_layout::TableKind::Devices => Some(&self.devices.rows.table_view),
            super::table_layout::TableKind::Backups => Some(&self.backups.table_view),
            _ => None,
        }
    }

    pub fn device_source_index_at_visible(&self, position: usize) -> Option<usize> {
        self.visible_device_indices().get(position).copied()
    }

    pub fn backup_source_index_at_visible(&self, position: usize) -> Option<usize> {
        self.visible_backup_indices().get(position).copied()
    }

    pub fn visible_device_indices(&self) -> Vec<usize> {
        let indices =
            if self.shell.workspace == Workspace::Devices && !self.active_search_query().is_empty() {
                self.shell.search_matches.clone()
            } else {
                (0..self.devices.rows.len()).collect()
            };
        self.devices.table_view.sorted_indices(
            indices,
            self.table_interaction(super::table_layout::TableKind::Devices),
        )
    }

    pub fn visible_device_count(&self) -> usize {
        self.visible_device_indices().len()
    }

    pub fn device_at_visible(&self, position: usize) -> Option<&crate::disk_scan::Row> {
        let index = self.device_source_index_at_visible(position)?;
        self.devices.rows.get(index)
    }

    pub fn visible_backup_indices(&self) -> Vec<usize> {
        let indices =
            if self.shell.workspace == Workspace::Backups && !self.active_search_query().is_empty() {
                self.shell.search_matches.clone()
            } else {
                (0..self.backups.rows.len()).collect()
            };
        self.backups.table_view.sorted_indices(
            indices,
            self.table_interaction(super::table_layout::TableKind::Backups),
        )
    }

    pub fn visible_backup_count(&self) -> usize {
        self.visible_backup_indices().len()
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
                .pinned_disk
                .and_then(|disk| self.devices.rows.iter().find(|row| row.disk == disk)),
        }
    }

    fn provision_selectable_device_indices(&self) -> Vec<usize> {
        let mut indices = self
            .devices
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                (row.proto == "USB"
                    && !row.denied
                    && row.probe_error.is_none()
                    && row.confirmed_provision_kind().is_some())
                .then_some(index)
            })
            .collect::<Vec<_>>();
        if let Some(sort) = self.table_sort(super::table_layout::TableKind::ProvisionDevices) {
            indices.sort_by(|left, right| {
                let a = &self.devices.rows[*left];
                let b = &self.devices.rows[*right];
                let value = |row: &crate::disk_scan::Row| match sort.column {
                    0 => format!("disk{}", row.disk),
                    1 => row.size.to_string(),
                    2 => format!("{}:{}", row.vid, row.pid),
                    3 => row
                        .confirmed_provision_kind()
                        .map(|kind| kind.full_name().to_string())
                        .unwrap_or_default(),
                    4 => row.onlyid.clone().unwrap_or_default(),
                    _ => String::new(),
                };
                let ordering = super::table_layout::smart_cell_cmp(&value(a), &value(b))
                    .then_with(|| left.cmp(right));
                match sort.direction {
                    super::table_layout::SortDirection::Ascending => ordering,
                    super::table_layout::SortDirection::Descending => ordering.reverse(),
                }
            });
        }
        indices
    }

    fn provision_selectable_devices(&self) -> impl Iterator<Item = &crate::disk_scan::Row> {
        self.provision_selectable_device_indices()
            .into_iter()
            .filter_map(|index| self.devices.rows.get(index))
    }

    pub fn provision_device_at(&self, index: usize) -> Option<&crate::disk_scan::Row> {
        let source = *self.provision_selectable_device_indices().get(index)?;
        self.devices.rows.get(source)
    }

    pub fn provision_menu_order(&self) -> Vec<usize> {
        let mut order = (0..ProvisionKind::ALL.len()).collect::<Vec<_>>();
        if let Some(sort) = self.table_sort(super::table_layout::TableKind::ProvisionMenu) {
            order.sort_by(|left, right| {
                let value = |index: usize| {
                    let kind = ProvisionKind::ALL[index];
                    match sort.column {
                        0 => index.to_string(),
                        1 => kind.title().to_string(),
                        2 => kind.description().to_string(),
                        _ => String::new(),
                    }
                };
                let ordering = super::table_layout::smart_cell_cmp(&value(*left), &value(*right))
                    .then_with(|| left.cmp(right));
                match sort.direction {
                    super::table_layout::SortDirection::Ascending => ordering,
                    super::table_layout::SortDirection::Descending => ordering.reverse(),
                }
            });
        }
        order
    }

    pub fn provision_kind_at_visible(&self, position: usize) -> Option<ProvisionKind> {
        let actual = *self.provision_menu_order().get(position)?;
        ProvisionKind::ALL.get(actual).copied()
    }

    pub fn provision_menu_source_index(&self, position: usize) -> Option<usize> {
        self.provision_menu_order().get(position).copied()
    }

    pub fn provision_menu_source_index_or_default(&self, position: usize) -> usize {
        self.provision_menu_source_index(position)
            .unwrap_or(position)
            .min(ProvisionKind::ALL.len() - 1)
    }

    pub fn provision_menu_visible_position(&self, source_index: usize) -> Option<usize> {
        self.provision_menu_order()
            .iter()
            .position(|index| *index == source_index)
    }

    pub fn provision_select_disk(&mut self) -> Option<u32> {
        if self.shell.workspace != Workspace::Provision
            || self.provision.stage != ProvisionStage::SelectDisk
        {
            return None;
        }
        let disk = self.provision_device_at(self.shell.selected)?.disk;
        self.pinned_disk = Some(disk);
        self.provision.target_disk = Some(disk);
        self.provision.stage = ProvisionStage::Menu;
        self.provision.message = None;
        self.shell.selected = self
            .provision
            .menu_selected
            .min(ProvisionKind::ALL.len().saturating_sub(1));
        self.set_item_count(ProvisionKind::ALL.len());
        Some(disk)
    }

    pub fn begin_provision_for_selected_device(&mut self) -> Result<u32, String> {
        if self.shell.workspace != Workspace::Devices {
            return Err("请先在设备页选择目标 USB 盘。".into());
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
        self.push_navigation_frame(NavigationLocation::Devices);
        self.provision.target_disk = Some(disk);
        self.switch_workspace(Workspace::Provision);
        self.pinned_disk = Some(disk);
        self.provision.stage = ProvisionStage::Menu;
        self.provision.message = None;
        self.shell.selected = self
            .provision
            .menu_selected
            .min(ProvisionKind::ALL.len().saturating_sub(1));
        self.set_item_count(ProvisionKind::ALL.len());
        Ok(disk)
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
        self.backups.rows = backups;
        self.backups.table_view = super::table_layout::backup_table_view(
            &self.backups.rows,
            self.backups.table_view.generation.wrapping_add(1),
        );
        let selectable = self
            .backups
            .iter()
            .filter(|row| row.content_sha256.is_some())
            .map(|row| row.path.clone())
            .collect::<std::collections::BTreeSet<_>>();
        self.backups.selection
            .retain(|path| selectable.contains(path));
        self.backups.scan_pending = false;
        if self.shell.workspace == Workspace::Backups {
            self.rebuild_workspace_filter();
            if let Some(path) = selected_path {
                let source_index = self.backups.rows.iter().position(|row| row.path == path);
                self.shell.selected = source_index
                    .and_then(|index| {
                        if self.shell.workspace_filter_active() {
                            self.shell.search_matches.iter().position(|value| *value == index)
                        } else {
                            Some(index)
                        }
                    })
                    .unwrap_or(0);
            }
        }
    }

    fn switch_workspace(&mut self, workspace: Workspace) {
        if self.shell.workspace == workspace {
            return;
        }
        if self.shell.workspace == Workspace::Devices
            && matches!(workspace, Workspace::Backups | Workspace::Inspect)
        {
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
        self.shell.search_query.clear();
        self.shell.input_buffer.clear();
        if self.shell.input_mode == InputMode::Search {
            self.shell.input_mode = InputMode::Normal;
        }
        self.shell.workspace = workspace;
        self.shell.selected = 0;
        if workspace == Workspace::Devices {
            if let Some(disk) = self.provision.target_disk {
                self.shell.selected = self
                    .devices
                    .iter()
                    .position(|row| row.disk == disk)
                    .unwrap_or(0);
            }
        }
        let count = match workspace {
            Workspace::Devices => self.devices.rows.len(),
            Workspace::Backups => self.backups.rows.len(),
            Workspace::Inspect => 0,
            Workspace::Provision => match self.provision.stage {
                ProvisionStage::SelectDisk => self.provision_selectable_devices().count(),
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
        self.shell.selected
    }

    pub fn navigation(&self) -> &NavigationStack {
        &self.shell.navigation
    }

    pub fn active_table_kind(&self) -> Option<super::table_layout::TableKind> {
        use super::table_layout::TableKind;
        use crate::tui::pane::PaneId;

        match self.shell.workspace {
            Workspace::Devices if self.devices_focused_pane() == PaneId::DevicesList => {
                Some(TableKind::Devices)
            }
            Workspace::Backups if self.backups_focused_pane() == PaneId::BackupsList => {
                Some(TableKind::Backups)
            }
            Workspace::Provision => match self.provision.stage {
                ProvisionStage::SelectDisk => Some(TableKind::ProvisionDevices),
                ProvisionStage::Menu => Some(TableKind::ProvisionMenu),
                _ => None,
            },
            Workspace::Inspect
                if self.inspect.advanced_focused_pane() == Some(PaneId::InspectDetail)
                    && !self.inspect.advanced_detail_rows().is_empty() =>
            {
                Some(TableKind::InspectFields)
            }
            _ => None,
        }
    }

    pub fn table_interaction(
        &self,
        kind: super::table_layout::TableKind,
    ) -> super::table_layout::TableInteractionState {
        self.horizontal_scroll
            .get(&kind)
            .copied()
            .unwrap_or_default()
    }

    pub fn table_active_column(&self, kind: super::table_layout::TableKind) -> usize {
        self.table_interaction(kind).active_column()
    }

    pub fn table_column_order(&self, kind: super::table_layout::TableKind) -> Vec<usize> {
        let count = super::table_layout::layout_for(kind).specs().len();
        self.table_column_order
            .get(&kind)
            .filter(|order| {
                order.len() == count && {
                    let mut sorted = (*order).clone();
                    sorted.sort_unstable();
                    sorted == (0..count).collect::<Vec<_>>()
                }
            })
            .cloned()
            .unwrap_or_else(|| (0..count).collect())
    }

    pub fn table_logical_column(
        &self,
        kind: super::table_layout::TableKind,
        visual_column: usize,
    ) -> usize {
        let order = self.table_column_order(kind);
        order
            .get(visual_column)
            .copied()
            .unwrap_or_else(|| visual_column.min(order.len().saturating_sub(1)))
    }

    fn table_selected_row_values(
        &self,
        kind: super::table_layout::TableKind,
    ) -> Option<Vec<String>> {
        use super::table_layout::TableKind;

        let sanitize = |value: String| crate::ui::sanitize_terminal_text(&value);
        match kind {
            TableKind::Devices => {
                let source = self.device_source_index_at_visible(self.shell.selected)?;
                self.devices.table_view.rows.get(source).cloned()
            }
            TableKind::Backups => {
                let source = self.backup_source_index_at_visible(self.shell.selected)?;
                self.backups.table_view.rows.get(source).cloned()
            }
            TableKind::ProvisionDevices => {
                let row = self.provision_device_at(self.shell.selected)?;
                Some(
                    vec![
                        format!("disk{}", row.disk),
                        format!("{:.2} GiB", row.size as f64 / 1_073_741_824.0),
                        format!("{}:{}", row.vid, row.pid),
                        row.confirmed_provision_kind()
                            .map(|kind| kind.full_name().to_string())
                            .unwrap_or_else(|| "未知 / 未确认".into()),
                        row.onlyid.clone().unwrap_or_else(|| "—".into()),
                    ]
                    .into_iter()
                    .map(sanitize)
                    .collect(),
                )
            }
            TableKind::ProvisionMenu => {
                let source = *self.provision_menu_order().get(self.shell.selected)?;
                let kind = ProvisionKind::ALL.get(source).copied()?;
                Some(
                    vec![
                        source.to_string(),
                        kind.title().to_string(),
                        kind.description().to_string(),
                    ]
                    .into_iter()
                    .map(sanitize)
                    .collect(),
                )
            }
            TableKind::InspectFields => {
                let row = self.inspect.advanced_detail_selected_row()?;
                Some(row.cells.iter().cloned().map(sanitize).collect::<Vec<_>>())
            }
        }
    }

    pub fn table_copy_payload(
        &self,
        kind: super::table_layout::TableKind,
        whole_row: bool,
    ) -> Option<String> {
        let values = self.table_selected_row_values(kind)?;
        let order = self.table_column_order(kind);
        if whole_row {
            Some(super::table_layout::copy_row_values(kind, &order, &values))
        } else {
            let logical = self.table_logical_column(kind, self.table_active_column(kind));
            super::table_layout::copy_cell_value(kind, logical, &values)
        }
    }

    pub fn table_visual_layout(
        &self,
        kind: super::table_layout::TableKind,
    ) -> super::table_layout::AdaptiveTableLayout {
        let base = super::table_layout::layout_for(kind);
        let order = self.table_column_order(kind);
        super::table_layout::AdaptiveTableLayout::new(
            order
                .into_iter()
                .filter_map(|logical| base.specs().get(logical).copied())
                .collect(),
        )
    }

    pub fn table_visual_widths(
        &self,
        kind: super::table_layout::TableKind,
        logical_widths: &[usize],
    ) -> Vec<usize> {
        self.table_column_order(kind)
            .into_iter()
            .map(|logical| logical_widths.get(logical).copied().unwrap_or(0))
            .collect()
    }

    fn ensure_table_column_order(
        &mut self,
        kind: super::table_layout::TableKind,
    ) -> &mut Vec<usize> {
        let count = super::table_layout::layout_for(kind).specs().len();
        let order = self
            .table_column_order
            .entry(kind)
            .or_insert_with(|| (0..count).collect());
        let valid = order.len() == count && {
            let mut sorted = order.clone();
            sorted.sort_unstable();
            sorted == (0..count).collect::<Vec<_>>()
        };
        if !valid {
            *order = (0..count).collect();
        }
        order
    }

    fn table_content_widths(&self, kind: super::table_layout::TableKind) -> Vec<usize> {
        use super::table_layout::{display_width, TableKind};

        match kind {
            TableKind::Devices => self.devices.table_view.content_widths.clone(),
            TableKind::Backups => self.backups.table_view.content_widths.clone(),
            TableKind::ProvisionDevices => {
                let headings = ["设备", "容量", "USB 身份", "盘型", "onlyid"];
                let mut widths = headings
                    .iter()
                    .map(|value| display_width(value))
                    .collect::<Vec<_>>();
                for row in self.provision_selectable_devices() {
                    let values = [
                        format!("disk{}", row.disk),
                        format!("{:.2} GiB", row.size as f64 / 1_073_741_824.0),
                        format!("{}:{}", row.vid, row.pid),
                        row.confirmed_provision_kind()
                            .map(|kind| kind.full_name().to_string())
                            .unwrap_or_else(|| "未知 / 未确认".into()),
                        row.onlyid.clone().unwrap_or_else(|| "—".into()),
                    ];
                    for (index, value) in values.iter().enumerate() {
                        widths[index] = widths[index].max(display_width(value));
                    }
                }
                widths
            }
            TableKind::ProvisionMenu => {
                let headings = ["#", "制盘方案", "布局 / 行为"];
                let mut widths = headings
                    .iter()
                    .map(|value| display_width(value))
                    .collect::<Vec<_>>();
                for (index, kind) in ProvisionKind::ALL.into_iter().enumerate() {
                    let values = [
                        index.to_string(),
                        kind.title().to_string(),
                        kind.description().to_string(),
                    ];
                    for (column, value) in values.iter().enumerate() {
                        widths[column] = widths[column].max(display_width(value));
                    }
                }
                widths
            }
            TableKind::InspectFields => {
                let mut widths = INSPECT_DETAIL_HEADINGS
                    .iter()
                    .map(|value| display_width(value))
                    .collect::<Vec<_>>();
                for row in self.inspect.advanced_detail_rows() {
                    for (index, value) in row.cells.iter().enumerate() {
                        widths[index] = widths[index].max(display_width(value));
                    }
                }
                widths
            }
        }
    }

    fn table_viewport_width(
        &self,
        kind: super::table_layout::TableKind,
        terminal_width: u16,
        terminal_height: usize,
    ) -> u16 {
        use super::table_layout::TableKind;
        match kind {
            TableKind::Devices | TableKind::Backups => terminal_width.saturating_sub(4),
            TableKind::InspectFields => terminal_width.saturating_sub(3),
            TableKind::ProvisionDevices | TableKind::ProvisionMenu => {
                let class = crate::tui::ui::ViewportClass::for_width(terminal_width);
                let content_height = terminal_height.saturating_sub(5);
                let main_width = if matches!(
                    class,
                    crate::tui::ui::ViewportClass::Wide | crate::tui::ui::ViewportClass::UltraWide
                ) && content_height >= 12
                {
                    terminal_width.saturating_sub(40)
                } else {
                    terminal_width
                };
                main_width.saturating_sub(4)
            }
        }
        .max(1)
    }

    fn table_visual_geometry(
        &self,
        kind: super::table_layout::TableKind,
    ) -> (super::table_layout::AdaptiveTableLayout, Vec<usize>) {
        let logical_widths = self.table_content_widths(kind);
        (
            self.table_visual_layout(kind),
            self.table_visual_widths(kind, &logical_widths),
        )
    }

    pub fn reorder_table_column_for_viewport(
        &mut self,
        kind: super::table_layout::TableKind,
        reverse: bool,
        terminal_width: u16,
        terminal_height: usize,
    ) -> bool {
        let active = self.table_interaction(kind).active_column();
        let count = super::table_layout::layout_for(kind).specs().len();
        if count == 0 {
            return false;
        }
        let target = if reverse {
            active.saturating_sub(1)
        } else {
            active.saturating_add(1).min(count - 1)
        };
        if target == active {
            return false;
        }

        {
            let order = self.ensure_table_column_order(kind);
            order.swap(active, target);
        }

        let (layout, widths) = self.table_visual_geometry(kind);
        let viewport_width = self.table_viewport_width(kind, terminal_width, terminal_height);
        let interaction = self.horizontal_scroll.entry(kind).or_default();
        interaction.set_active_column(target);
        interaction.ensure_active_visible_for_layout(&layout, &widths, viewport_width, reverse);
        true
    }

    pub fn move_table_column_for_viewport(
        &mut self,
        kind: super::table_layout::TableKind,
        reverse: bool,
        terminal_width: u16,
        terminal_height: usize,
    ) -> bool {
        let (layout, widths) = self.table_visual_geometry(kind);
        let viewport_width = self.table_viewport_width(kind, terminal_width, terminal_height);
        self.horizontal_scroll.entry(kind).or_default().move_active(
            &layout,
            &widths,
            viewport_width,
            reverse,
        )
    }

    pub fn move_table_column_edge_for_viewport(
        &mut self,
        kind: super::table_layout::TableKind,
        last: bool,
        terminal_width: u16,
        terminal_height: usize,
    ) -> bool {
        let (layout, widths) = self.table_visual_geometry(kind);
        let viewport_width = self.table_viewport_width(kind, terminal_width, terminal_height);
        self.horizontal_scroll
            .entry(kind)
            .or_default()
            .move_active_edge(&layout, &widths, viewport_width, last)
    }

    pub fn table_sort(
        &self,
        kind: super::table_layout::TableKind,
    ) -> Option<super::table_layout::TableSort> {
        self.table_interaction(kind).sort()
    }

    pub fn move_table_column(
        &mut self,
        kind: super::table_layout::TableKind,
        reverse: bool,
    ) -> bool {
        let (layout, widths) = self.table_visual_geometry(kind);
        self.horizontal_scroll.entry(kind).or_default().move_active(
            &layout,
            &widths,
            u16::MAX,
            reverse,
        )
    }

    pub fn toggle_table_sort(&mut self, kind: super::table_layout::TableKind) {
        self.change_table_sort(kind, false);
    }

    pub fn clear_table_sort(&mut self, kind: super::table_layout::TableKind) -> bool {
        self.change_table_sort(kind, true)
    }

    fn change_table_sort(&mut self, kind: super::table_layout::TableKind, clear: bool) -> bool {
        let device_disk = (kind == super::table_layout::TableKind::Devices)
            .then(|| self.selected_device().map(|row| row.disk))
            .flatten();
        let backup_path = (kind == super::table_layout::TableKind::Backups)
            .then(|| self.selected_backup().map(|row| row.path.clone()))
            .flatten();
        let provision_disk = (kind == super::table_layout::TableKind::ProvisionDevices)
            .then(|| self.provision_device_at(self.shell.selected).map(|row| row.disk))
            .flatten();
        let provision_kind = (kind == super::table_layout::TableKind::ProvisionMenu)
            .then(|| self.provision_kind_at_visible(self.shell.selected))
            .flatten();
        let inspect_key = (kind == super::table_layout::TableKind::InspectFields)
            .then(|| {
                self.inspect.advanced_detail_selected_row()
                    .map(|row| (row.field_index, row.child_index, row.range))
            })
            .flatten();

        let logical_column = self.table_logical_column(kind, self.table_active_column(kind));
        let interaction = self.horizontal_scroll.entry(kind).or_default();
        let changed = if clear {
            interaction.clear_sort()
        } else {
            interaction.toggle_sort_for(logical_column);
            true
        };

        if let Some(disk) = device_disk {
            if let Some(position) = self
                .visible_device_indices()
                .iter()
                .position(|index| self.devices.rows[*index].disk == disk)
            {
                self.shell.selected = position;
            }
        }
        if let Some(path) = backup_path {
            if let Some(position) = self
                .visible_backup_indices()
                .iter()
                .position(|index| self.backups.rows[*index].path == path)
            {
                self.shell.selected = position;
            }
        }
        if let Some(disk) = provision_disk {
            if let Some(position) = self
                .provision_selectable_device_indices()
                .iter()
                .position(|index| self.devices.rows[*index].disk == disk)
            {
                self.shell.selected = position;
            }
        }
        if let Some(kind) = provision_kind {
            if let Some(actual) = ProvisionKind::ALL
                .iter()
                .position(|candidate| *candidate == kind)
            {
                if let Some(position) = self
                    .provision_menu_order()
                    .iter()
                    .position(|index| *index == actual)
                {
                    self.shell.selected = position;
                }
            }
        }
        if let Some(key) = inspect_key {
            let rows = self.inspect.advanced_detail_rows();
            if let Some(position) = rows
                .iter()
                .position(|row| (row.field_index, row.child_index, row.range) == key)
            {
                if let Some(advanced) = self.inspect.advanced.as_mut() {
                    advanced
                        .pane_focus
                        .viewport_mut(crate::tui::pane::PaneId::InspectDetail)
                        .selected = Some(position);
                }
            }
        }
        changed
    }

    pub fn table_scroll_offset(&self, kind: super::table_layout::TableKind) -> usize {
        self.horizontal_scroll
            .get(&kind)
            .copied()
            .unwrap_or_default()
            .offset()
    }

    pub fn scroll_table(&mut self, kind: super::table_layout::TableKind, reverse: bool) -> bool {
        self.scroll_table_for_viewport(kind, reverse, 80, 24)
    }

    pub fn scroll_table_for_viewport(
        &mut self,
        kind: super::table_layout::TableKind,
        reverse: bool,
        terminal_width: u16,
        terminal_height: usize,
    ) -> bool {
        let (layout, widths) = self.table_visual_geometry(kind);
        let viewport_width = self.table_viewport_width(kind, terminal_width, terminal_height);
        self.horizontal_scroll
            .entry(kind)
            .or_default()
            .scroll_viewport(&layout, &widths, viewport_width, reverse)
    }

    pub fn pane_viewport(&self, pane: crate::tui::pane::PaneId) -> &crate::tui::pane::PaneViewport {
        if pane.is_inspect() {
            self.inspect.advanced
                .as_ref()
                .expect("Inspect pane requested without Inspect state")
                .pane_focus
                .viewport(pane)
        } else if pane.is_devices() {
            self.devices.pane_focus.viewport(pane)
        } else if pane.is_backups() {
            self.backups.pane_focus.viewport(pane)
        } else {
            self.provision.pane_focus.viewport(pane)
        }
    }

    pub fn pane_viewport_mut(
        &mut self,
        pane: crate::tui::pane::PaneId,
    ) -> &mut crate::tui::pane::PaneViewport {
        if pane.is_inspect() {
            self.inspect.advanced
                .as_mut()
                .expect("Inspect pane requested without Inspect state")
                .pane_focus
                .viewport_mut(pane)
        } else if pane.is_devices() {
            self.devices.pane_focus.viewport_mut(pane)
        } else if pane.is_backups() {
            self.backups.pane_focus.viewport_mut(pane)
        } else {
            self.provision.pane_focus.viewport_mut(pane)
        }
    }

    pub const fn devices_focused_pane(&self) -> crate::tui::pane::PaneId {
        self.devices.pane_focus.focused()
    }

    pub const fn backups_focused_pane(&self) -> crate::tui::pane::PaneId {
        self.backups.pane_focus.focused()
    }

    pub fn focus_devices_pane(&mut self, pane: crate::tui::pane::PaneId) {
        if pane.is_devices() {
            self.devices.pane_focus.focus(pane);
        }
    }

    pub fn activate_device_for_viewport(&mut self, _width: u16) -> Result<Option<u32>, String> {
        if self.selected_device().is_none() {
            return Err("请先选择设备。".into());
        }
        self.focus_devices_pane(crate::tui::pane::PaneId::DevicesSummary);
        Ok(None)
    }

    pub fn device_summary_selected_section(&self) -> DeviceSummarySection {
        DeviceSummarySection::ALL[self
            .device_summary_selected
            .min(DeviceSummarySection::ALL.len() - 1)]
    }

    pub fn device_summary_section_expanded(&self, section: DeviceSummarySection) -> bool {
        self.devices.summary_expanded & section.bit() != 0
    }

    pub fn device_summary_move_section(&mut self, delta: isize) {
        let max = DeviceSummarySection::ALL.len().saturating_sub(1);
        self.devices.summary_selected = if delta < 0 {
            self.devices.summary_selected
                .saturating_sub(delta.unsigned_abs())
        } else {
            self.devices.summary_selected
                .saturating_add(delta as usize)
                .min(max)
        };
    }

    pub fn device_summary_toggle_selected_section(&mut self) {
        let section = self.devices.summary_selected_section();
        self.devices.summary_expanded ^= section.bit();
    }

    pub fn disk_layout_tail_expansion(&self) -> super::disk_layout::TailExpansion {
        self.disk_layout_tail
    }

    pub fn toggle_disk_layout_tail(&mut self) {
        self.disk_layout_tail.toggle();
        self.disk_layout_selected = 0;
    }

    pub fn disk_layout_selected(&self) -> usize {
        self.disk_layout_selected
    }

    pub fn disk_layout_move_selection(&mut self, delta: isize, count: usize) {
        self.disk_layout_selected = if delta < 0 {
            self.disk_layout_selected
                .saturating_sub(delta.unsigned_abs())
        } else {
            self.disk_layout_selected.saturating_add(delta as usize)
        }
        .min(count.saturating_sub(1));
    }

    pub fn disk_layout_detail(
        &self,
        model: &super::disk_layout::DiskLayoutModel,
    ) -> Option<String> {
        let presentation = super::disk_layout::DiskLayoutPresentation::new(
            model,
            super::disk_layout::DiskLayoutProfile::DetailedExact,
            self.disk_layout_tail,
        );
        let visible = presentation.visible_model();
        let segment = visible.segments.get(self.disk_layout_selected)?;
        Some(format!(
            "{} · {} · {} sectors · {} bytes",
            segment.label,
            segment.closed_range(),
            segment.sector_count,
            segment
                .sector_count
                .saturating_mul(crate::common::SECTOR as u64)
        ))
    }

    pub fn shift_workspace_pane(&mut self, reverse: bool) {
        match self.shell.workspace {
            Workspace::Devices => self
                .devices_pane_focus
                .cycle(&crate::tui::pane::PaneId::DEVICES_ORDER, reverse),
            Workspace::Backups => self
                .backups_pane_focus
                .cycle(&crate::tui::pane::PaneId::BACKUPS_ORDER, reverse),
            Workspace::Inspect | Workspace::Provision => {}
        }
    }

    pub fn focus_backups_pane(&mut self, pane: crate::tui::pane::PaneId) {
        if pane.is_backups() {
            self.backups.pane_focus.focus(pane);
        }
    }

    pub fn spatial_workspace_focus(&mut self, dx: i8, dy: i8) {
        use crate::tui::pane::PaneId;
        let (focus, next) = match self.shell.workspace {
            Workspace::Devices => {
                let focus = self.devices.pane_focus.focused();
                let next = match (focus, dx.signum(), dy.signum()) {
                    (PaneId::DevicesList, _, 1) => Some(PaneId::DevicesSummary),
                    (PaneId::DevicesSummary | PaneId::DevicesStats, _, -1) => {
                        Some(PaneId::DevicesList)
                    }
                    (PaneId::DevicesSummary, 1, _) => Some(PaneId::DevicesStats),
                    (PaneId::DevicesStats, -1, _) => Some(PaneId::DevicesSummary),
                    _ => None,
                };
                (focus, next)
            }
            Workspace::Backups => {
                let focus = self.backups.pane_focus.focused();
                let next = match (focus, dx.signum(), dy.signum()) {
                    (PaneId::BackupsList, _, 1) => Some(PaneId::BackupSummary),
                    (PaneId::BackupSummary | PaneId::BackupCoverage, _, -1) => {
                        Some(PaneId::BackupsList)
                    }
                    (PaneId::BackupSummary, 1, _) => Some(PaneId::BackupCoverage),
                    (PaneId::BackupCoverage, -1, _) => Some(PaneId::BackupSummary),
                    _ => None,
                };
                (focus, next)
            }
            _ => return,
        };
        if let Some(next) = next {
            if focus.is_devices() {
                self.devices.pane_focus.focus(next);
            } else {
                self.backups.pane_focus.focus(next);
            }
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
            NavigationLocation::Devices => Some(self.devices.pane_focus.clone()),
            NavigationLocation::Backups => Some(self.backups.pane_focus.clone()),
        };
        self.shell.navigation.push(NavigationFrame {
            location,
            selection: self.shell.selected,
            item_count: self.shell.item_count,
            panel: None,
            tree_selection: 0,
            pane_focus,
            table_scroll,
        });
    }

    pub fn pop_navigation_frame(&mut self) -> Option<NavigationFrame> {
        self.shell.navigation.pop()
    }

    fn restore_workspace_frame(&mut self) {
        if let Some(frame) = self.pop_navigation_frame() {
            let workspace = match frame.location {
                NavigationLocation::Devices => Workspace::Devices,
                NavigationLocation::Backups => Workspace::Backups,
                NavigationLocation::Provision => Workspace::Provision,
                NavigationLocation::Inspect => Workspace::Inspect,
                NavigationLocation::SectorInspector => return,
            };
            self.switch_workspace(workspace);
            self.shell.selected = frame.selection.min(self.shell.item_count.saturating_sub(1));
            if let Some(pane_focus) = frame.pane_focus {
                match workspace {
                    Workspace::Devices => self.devices.pane_focus = pane_focus,
                    Workspace::Backups => self.backups.pane_focus = pane_focus,
                    Workspace::Provision => self.provision.pane_focus = pane_focus,
                    Workspace::Inspect => {}
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
        self.shell.item_count
    }

    pub const fn input_mode(&self) -> InputMode {
        self.shell.input_mode
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
            self.set_notice(
                "关键操作仍在执行，当前不能返回；操作完成后再按 Esc 返回。".to_string(),
            );
            Some(StateEffect::None)
        } else {
            self.set_notice("关键操作仍在执行，完成前不能执行该命令或启动其他任务。".to_string());
            Some(StateEffect::None)
        }
    }

    pub fn navigate(&mut self, command: NavCommand, viewport_height: usize) -> StateEffect {
        if let Some(effect) = self.guard_critical_command(command) {
            return effect;
        }

        if command == NavCommand::Escape {
            if let Some(advanced) = self
                .advanced_inspect
                .as_ref()
                .filter(|_| self.shell.workspace == Workspace::Inspect)
            {
                if advanced.stage == AdvancedInspectStage::Running {
                    self.set_notice("全盘检查正在后台读取结构，请等待完成。");
                } else if advanced.prompt.is_some() {
                    self.inspect.advanced_cancel_prompt();
                } else if !self.inspect.advanced_close_sector() {
                    self.close_advanced_inspect();
                }
                return StateEffect::None;
            }
            if self.shell.workspace == Workspace::Provision {
                match self.provision.stage {
                    ProvisionStage::SelectDisk | ProvisionStage::Menu => {
                        self.restore_workspace_frame();
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
                }
                return StateEffect::None;
            }
            if self.shell.workspace == Workspace::Devices
                && self.devices.pane_focus.focused() != crate::tui::pane::PaneId::DevicesList
            {
                self.devices.pane_focus
                    .focus(crate::tui::pane::PaneId::DevicesList);
                return StateEffect::None;
            }
            if self.shell.workspace == Workspace::Backups
                && self.backups.pane_focus.focused() != crate::tui::pane::PaneId::BackupsList
            {
                self.backups.pane_focus
                    .focus(crate::tui::pane::PaneId::BackupsList);
                return StateEffect::None;
            }
            if self.wizard.is_some() {
                self.wizard = None;
                self.shell.input_mode = InputMode::Normal;
                return StateEffect::None;
            }
            if self.backups.delete.is_some() {
                self.backups.delete = None;
                self.shell.input_mode = InputMode::Normal;
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
                self.switch_workspace(
                    self.shell.workspace
                        .shifted(command == NavCommand::PreviousWorkspace),
                );
            }
            NavCommand::WorkspaceDevices => self.switch_workspace(Workspace::Devices),
            NavCommand::WorkspaceInspect => self.switch_workspace(Workspace::Inspect),
            NavCommand::WorkspaceBackups => self.switch_workspace(Workspace::Backups),
            NavCommand::WorkspaceProvision => self.switch_workspace(Workspace::Provision),
            NavCommand::Up => {
                if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesSummary
                {
                    self.device_summary_move_section(-1);
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
                    }
                }
            }
            NavCommand::Down => {
                if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesSummary
                {
                    self.device_summary_move_section(1);
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
                            crate::tui::pane::PaneId::DevicesStats => 11,
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
                        self.shell.selected = (self.shell.selected + 1).min(self.shell.item_count - 1);
                    }
                }
            }
            NavCommand::Top => self.shell.selected = 0,
            NavCommand::Bottom => {
                self.shell.selected = self.shell.item_count.saturating_sub(1);
            }
            NavCommand::HalfPageDown => {
                if self.shell.item_count > 0 {
                    let delta = (viewport_height / 2).max(1);
                    self.shell.selected = self.shell.selected.saturating_add(delta).min(self.shell.item_count - 1);
                }
            }
            NavCommand::HalfPageUp => {
                let delta = (viewport_height / 2).max(1);
                self.shell.selected = self.shell.selected.saturating_sub(delta);
            }
            NavCommand::Search => {
                self.shell.input_buffer = self.shell.search_query.clone();
                self.shell.input_mode = InputMode::Search;
            }
            NavCommand::CommandPalette => {
                self.shell.input_buffer.clear();
                self.shell.input_mode = InputMode::Command;
            }
            NavCommand::Help => self.shell.input_mode = InputMode::Help,
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
