//! Pure TUI state machine.
//!
//! The state layer never performs I/O. That makes navigation and cancellation semantics testable
//! without a real terminal and keeps critical-operation policy independent from crossterm.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectMode {
    Fields,
    DecodedHex,
    RawHex,
}

#[derive(Debug, Clone)]
pub struct InspectState {
    selected: usize,
    item_count: usize,
    mode: InspectMode,
    scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteKind {
    Apply,
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

#[derive(Debug, Clone)]
pub struct BackupDeleteState {
    pub stage: WizardStage,
    pub path: std::path::PathBuf,
    pub expected_sha256: String,
    pub confirmation: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupPruneStage {
    Input,
    Planning,
    Review,
    Confirm,
    Running,
    Result,
}

pub struct BackupPrunePrepared {
    pub plan: crate::application::backup::DeletePlan,
    pub keep: usize,
    pub originals: usize,
    pub retained_snapshots: usize,
}

pub struct BackupPruneState {
    pub stage: BackupPruneStage,
    pub keep_input: String,
    pub prepared: Option<BackupPrunePrepared>,
    pub confirmation: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workspace {
    Devices,
    Backups,
    Provision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionKind {
    Mode0,
    Mode1,
    Mode2,
    Mode3,
    Convert,
}

impl ProvisionKind {
    pub const ALL: [Self; 5] = [
        Self::Mode0,
        Self::Mode1,
        Self::Mode2,
        Self::Mode3,
        Self::Convert,
    ];

    pub const fn mode(self) -> Option<u8> {
        match self {
            Self::Mode0 => Some(0),
            Self::Mode1 => Some(1),
            Self::Mode2 => Some(2),
            Self::Mode3 => Some(3),
            Self::Convert => None,
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Mode0 => "模式 0 · 缺省三分区",
            Self::Mode1 => "模式 1 · 启动/交换二合一",
            Self::Mode2 => "模式 2 · 整盘加密",
            Self::Mode3 => "模式 3 · 内外网双分区",
            Self::Convert => "现有官方盘 · 严格免密改造",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Mode0 => "type1 启动区 + type2 交换区 + type4 保密区",
            Self::Mode1 => "type2 二合一区 + type4 保密区",
            Self::Mode2 => "兼容 type1 + type4 整盘加密布局",
            Self::Mode3 => "type1 + type2 内外网双分区",
            Self::Convert => "保留原 type4 几何/密钥，仅重建前部明文 exFAT",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionStage {
    Menu,
    Form,
    Planning,
    Review,
    Confirm,
    Running,
    Result,
}

#[derive(Debug, Clone)]
pub enum ProvisionPrepared {
    New(Box<crate::application::provision::PreparedNewProvision>),
    Convert(Box<crate::application::provision::PreparedPasswordlessConversion>),
}

#[derive(Debug, Clone)]
pub struct ProvisionForm {
    pub boot_mib: String,
    pub share_mib: String,
    pub encrypt_mib: String,
    pub label_id: String,
    pub user: String,
    pub dept: String,
    pub label: String,
    pub password: String,
    pub volume_label: String,
}

impl Default for ProvisionForm {
    fn default() -> Self {
        Self {
            boot_mib: "512".into(),
            share_mib: "1024".into(),
            encrypt_mib: "2048".into(),
            label_id: String::new(),
            user: String::new(),
            dept: String::new(),
            label: "SAFE6".into(),
            password: String::new(),
            volume_label: "SAFE6".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProvisionState {
    pub stage: ProvisionStage,
    pub kind: ProvisionKind,
    pub menu_selected: usize,
    pub field_selected: usize,
    pub form: ProvisionForm,
    pub prepared: Option<ProvisionPrepared>,
    pub confirmation: String,
    pub message: Option<String>,
}

impl Default for ProvisionState {
    fn default() -> Self {
        Self {
            stage: ProvisionStage::Menu,
            kind: ProvisionKind::Mode0,
            menu_selected: 0,
            field_selected: 0,
            form: ProvisionForm::default(),
            prepared: None,
            confirmation: String::new(),
            message: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
    Command,
    Help,
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
    Refresh,
    BeginApply,
    BeginRestore,
    BeginBackupCreate,
    BeginBackupCreateDeep,
    BeginBackupDelete,
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
    device_scan_pending: bool,
    backup_scan_pending: bool,
    selected: usize,
    item_count: usize,
    input_mode: InputMode,
    critical_operation: bool,
    exit_pending: bool,
    wizard: Option<WizardState>,
    backup_delete: Option<BackupDeleteState>,
    backup_prune: Option<BackupPruneState>,
    provision: ProvisionState,
    pinned_disk: Option<u32>,
    inspect: Option<InspectState>,
    inspect_data: Option<crate::application::inspect::InspectWorkspace>,
    inspect_pending: bool,
    notice: Option<String>,
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
            device_scan_pending: false,
            backup_scan_pending: false,
            selected: 0,
            item_count: 0,
            input_mode: InputMode::Normal,
            critical_operation: false,
            exit_pending: false,
            wizard: None,
            backup_delete: None,
            backup_prune: None,
            provision: ProvisionState::default(),
            pinned_disk: None,
            inspect: None,
            inspect_data: None,
            inspect_pending: false,
            notice: None,
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
            if self.input_mode == InputMode::Search && self.inspect.is_none() {
                self.rebuild_workspace_filter();
            }
        }
    }

    pub fn backspace_input(&mut self) {
        if matches!(self.input_mode, InputMode::Search | InputMode::Command) {
            self.input_buffer.pop();
            if self.input_mode == InputMode::Search && self.inspect.is_none() {
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
        if was_search && self.inspect.is_none() {
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
        let text = format!(
            "disk{} {} {} {}:{} {} {} {}",
            row.disk,
            row.device_id.as_deref().unwrap_or_default(),
            row.onlyid.as_deref().unwrap_or_default(),
            row.vid,
            row.pid,
            row.user.as_deref().unwrap_or_default(),
            row.dept.as_deref().unwrap_or_default(),
            row.proto
        );
        text.to_ascii_lowercase().contains(query)
    }

    fn backup_matches_query(row: &crate::application::BackupWorkspaceItem, query: &str) -> bool {
        let text = format!(
            "{} {} {} {} {} {}",
            row.file_name,
            row.display_time,
            row.onlyid.as_deref().unwrap_or_default(),
            row.user.as_deref().unwrap_or_default(),
            row.dept.as_deref().unwrap_or_default(),
            if row.is_nopwd {
                "nopwd 免密"
            } else {
                "encrypted 加密"
            }
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
        let Some(&target) = self.search_matches.get(match_index) else {
            return;
        };
        if let Some(inspect) = self.inspect.as_mut() {
            inspect.selected = target.min(inspect.item_count.saturating_sub(1));
        } else if !self.active_search_query().is_empty() {
            self.selected = match_index.min(self.item_count.saturating_sub(1));
        } else {
            self.selected = target.min(self.item_count.saturating_sub(1));
        }
    }

    pub fn submit_search(&mut self) -> usize {
        self.search_query = self.input_buffer.trim().to_ascii_lowercase();
        self.input_buffer.clear();
        self.input_mode = InputMode::Normal;
        self.search_matches.clear();
        self.search_cursor = 0;
        if self.search_query.is_empty() {
            if self.inspect.is_none() {
                self.rebuild_workspace_filter();
            }
            return 0;
        }

        if let Some(workspace) = &self.inspect_data {
            for (index, view) in workspace.views.iter().enumerate() {
                let mut text = format!("lba{} {}", view.lba, view.method);
                for field in &view.fields {
                    text.push(' ');
                    text.push_str(&field.label);
                    text.push(' ');
                    text.push_str(&field.value);
                    for child in &field.children {
                        text.push(' ');
                        text.push_str(&child.label);
                        text.push(' ');
                        text.push_str(&child.value);
                    }
                }
                for note in &view.notes {
                    text.push(' ');
                    text.push_str(note);
                }
                let ascii: String = view
                    .raw
                    .iter()
                    .map(|byte| {
                        if (0x20..=0x7e).contains(byte) {
                            *byte as char
                        } else {
                            ' '
                        }
                    })
                    .collect();
                text.push(' ');
                text.push_str(&ascii);
                text.push(' ');
                for byte in &view.raw {
                    text.push_str(&format!("{byte:02x}"));
                    text.push(' ');
                }
                if text.to_ascii_lowercase().contains(&self.search_query) {
                    self.search_matches.push(index);
                }
            }
        } else {
            self.rebuild_workspace_filter();
            return self.search_matches.len();
        }
        if !self.search_matches.is_empty() {
            self.activate_search_match(0);
        }
        self.search_matches.len()
    }

    fn cycle_search(&mut self, reverse: bool) {
        if self.search_matches.is_empty() {
            return;
        }
        if self.inspect.is_none() && self.workspace_filter_active() {
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
            return;
        }
        if reverse {
            self.search_cursor = if self.search_cursor == 0 {
                self.search_matches.len() - 1
            } else {
                self.search_cursor - 1
            };
        } else {
            self.search_cursor = (self.search_cursor + 1) % self.search_matches.len();
        }
        self.activate_search_match(self.search_cursor);
    }

    pub fn search_status(&self) -> Option<String> {
        (!self.search_query.is_empty()).then(|| {
            if self.inspect.is_some() {
                format!(
                    "/{}  {}/{}",
                    self.search_query,
                    if self.search_matches.is_empty() {
                        0
                    } else {
                        self.search_cursor + 1
                    },
                    self.search_matches.len()
                )
            } else {
                format!(
                    "/{}  {} 条结果",
                    self.search_query,
                    self.search_matches.len()
                )
            }
        })
    }

    pub fn inspect_data(&self) -> Option<&crate::application::inspect::InspectWorkspace> {
        self.inspect_data.as_ref()
    }

    pub const fn inspect_pending(&self) -> bool {
        self.inspect_pending
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn set_notice(&mut self, message: impl Into<String>) {
        self.notice = Some(message.into());
    }

    pub fn clear_notice(&mut self) {
        self.notice = None;
    }

    pub fn set_inspect_pending(&mut self, pending: bool) {
        self.inspect_pending = pending;
        if pending {
            self.notice = Some("正在后台读取 LBA0-12…".into());
        }
    }

    pub fn open_inspect(&mut self, item_count: usize) {
        self.inspect = Some(InspectState {
            selected: 0,
            item_count,
            mode: InspectMode::Fields,
            scroll: 0,
        });
    }

    pub fn replace_inspect(&mut self, workspace: crate::application::inspect::InspectWorkspace) {
        self.clear_search_matches();
        self.search_query.clear();
        let count = workspace.views.len();
        self.inspect_data = Some(workspace);
        self.inspect_pending = false;
        self.notice = None;
        self.open_inspect(count);
    }

    pub fn inspect_selected_lba(&self) -> Option<u32> {
        self.inspect
            .as_ref()
            .and_then(|inspect| (inspect.item_count > 0).then_some(inspect.selected as u32))
    }

    pub fn inspect_mode(&self) -> Option<InspectMode> {
        self.inspect.as_ref().map(|inspect| inspect.mode)
    }

    pub fn inspect_scroll(&self) -> Option<usize> {
        self.inspect.as_ref().map(|inspect| inspect.scroll)
    }

    pub fn close_inspect(&mut self) {
        self.inspect = None;
        self.inspect_data = None;
        self.inspect_pending = false;
        self.clear_search_matches();
        self.search_query.clear();
        self.input_buffer.clear();
        if self.input_mode == InputMode::Search {
            self.input_mode = InputMode::Normal;
        }
        let count = match self.workspace {
            Workspace::Devices => self.devices.len(),
            Workspace::Backups => self.backups.len(),
            Workspace::Provision => ProvisionKind::ALL.len(),
        };
        self.set_item_count(count);
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
            self.notice = Some("关键操作仍在执行，完成前不能启动其他任务。".to_string());
            return false;
        }
        self.input_mode = InputMode::Normal;
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

    pub fn backup_delete(&self) -> Option<&BackupDeleteState> {
        self.backup_delete.as_ref()
    }

    pub fn backup_prune(&self) -> Option<&BackupPruneState> {
        self.backup_prune.as_ref()
    }

    pub fn backup_prune_mut(&mut self) -> Option<&mut BackupPruneState> {
        self.backup_prune.as_mut()
    }

    pub fn begin_backup_prune(&mut self) -> bool {
        if self.critical_operation || self.backup_prune.is_some() {
            return false;
        }
        self.backup_prune = Some(BackupPruneState {
            stage: BackupPruneStage::Input,
            keep_input: "3".into(),
            prepared: None,
            confirmation: String::new(),
            message: None,
        });
        true
    }

    pub fn backup_prune_push_digit(&mut self, ch: char) {
        if let Some(prune) = self.backup_prune.as_mut() {
            if prune.stage == BackupPruneStage::Input
                && ch.is_ascii_digit()
                && prune.keep_input.len() < 6
            {
                prune.keep_input.push(ch);
                prune.message = None;
            }
        }
    }

    pub fn backup_prune_backspace(&mut self) {
        if let Some(prune) = self.backup_prune.as_mut() {
            match prune.stage {
                BackupPruneStage::Input => {
                    prune.keep_input.pop();
                    prune.message = None;
                }
                BackupPruneStage::Confirm => {
                    prune.confirmation.pop();
                    prune.message = None;
                }
                _ => {}
            }
        }
    }

    pub fn backup_prune_start_plan(&mut self) -> Result<usize, String> {
        let prune = self
            .backup_prune
            .as_mut()
            .ok_or_else(|| "清理向导未打开".to_string())?;
        let keep = prune
            .keep_input
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| "保留份数必须为大于 0 的整数".to_string())?;
        prune.stage = BackupPruneStage::Planning;
        prune.message = Some("正在后台扫描备份并生成固定清理计划…".into());
        Ok(keep)
    }

    pub fn backup_prune_finish_plan(&mut self, result: Result<BackupPrunePrepared, String>) {
        let Some(prune) = self.backup_prune.as_mut() else {
            return;
        };
        match result {
            Ok(prepared) if prepared.plan.targets.is_empty() => {
                prune.prepared = Some(prepared);
                prune.stage = BackupPruneStage::Result;
                prune.message = Some("无需清理：当前备份已经满足保留策略。".into());
            }
            Ok(prepared) => {
                prune.prepared = Some(prepared);
                prune.stage = BackupPruneStage::Review;
                prune.message = None;
            }
            Err(message) => {
                prune.stage = BackupPruneStage::Input;
                prune.message = Some(message);
            }
        }
    }

    pub fn backup_prune_begin_confirm(&mut self) {
        if let Some(prune) = self.backup_prune.as_mut() {
            if prune.stage == BackupPruneStage::Review {
                prune.stage = BackupPruneStage::Confirm;
                prune.confirmation.clear();
                prune.message = None;
            }
        }
    }

    pub fn backup_prune_push_confirmation(&mut self, ch: char) {
        if let Some(prune) = self.backup_prune.as_mut() {
            if prune.stage == BackupPruneStage::Confirm && prune.confirmation.len() < 16 {
                prune.confirmation.push(ch);
                prune.message = None;
            }
        }
    }

    pub fn backup_prune_take_for_execute(&mut self) -> Option<BackupPrunePrepared> {
        let prune = self.backup_prune.as_mut()?;
        if prune.stage != BackupPruneStage::Confirm {
            return None;
        }
        if prune.confirmation != "YES" {
            prune.message = Some("必须精确输入 YES 才会删除备份".into());
            return None;
        }
        let prepared = prune.prepared.take()?;
        prune.stage = BackupPruneStage::Running;
        prune.message = Some("正在逐条复核摘要并清理固定候选…".into());
        self.critical_operation = true;
        Some(prepared)
    }

    pub fn backup_prune_finish_execute(&mut self, result: Result<usize, String>) {
        self.critical_operation = false;
        if let Some(prune) = self.backup_prune.as_mut() {
            prune.stage = BackupPruneStage::Result;
            prune.message = Some(match result {
                Ok(count) => format!("清理完成：已安全删除 {count} 份旧备份。"),
                Err(message) => message,
            });
        }
    }

    pub fn close_backup_prune(&mut self) {
        if !self.critical_operation {
            self.backup_prune = None;
        }
    }

    pub fn begin_backup_delete(
        &mut self,
        path: std::path::PathBuf,
        expected_sha256: String,
    ) -> bool {
        if self.critical_operation {
            self.notice = Some("关键操作仍在执行，完成前不能启动其他任务。".to_string());
            return false;
        }
        self.input_mode = InputMode::Normal;
        self.backup_delete = Some(BackupDeleteState {
            stage: WizardStage::Confirm,
            path,
            expected_sha256,
            confirmation: String::new(),
            message: None,
        });
        true
    }

    pub fn push_backup_delete_confirmation(&mut self, ch: char) {
        if let Some(delete) = self.backup_delete.as_mut() {
            if delete.stage == WizardStage::Confirm && delete.confirmation.len() < 16 {
                delete.confirmation.push(ch);
                delete.message = None;
            }
        }
    }

    pub fn backspace_backup_delete_confirmation(&mut self) {
        if let Some(delete) = self.backup_delete.as_mut() {
            if delete.stage == WizardStage::Confirm {
                delete.confirmation.pop();
                delete.message = None;
            }
        }
    }

    pub fn submit_backup_delete_confirmation(&mut self) -> Option<(std::path::PathBuf, String)> {
        let delete = self.backup_delete.as_mut()?;
        if delete.stage != WizardStage::Confirm {
            return None;
        }
        if delete.confirmation != "YES" {
            delete.message = Some("必须精确输入 YES 才会删除备份".to_string());
            return None;
        }
        delete.stage = WizardStage::Running;
        delete.message = Some("正在复核文件内容并删除备份…".to_string());
        self.critical_operation = true;
        Some((delete.path.clone(), delete.expected_sha256.clone()))
    }

    pub fn finish_backup_delete(&mut self, result: Result<(), String>) {
        self.critical_operation = false;
        if let Some(delete) = self.backup_delete.as_mut() {
            delete.stage = WizardStage::Result;
            delete.message = Some(match result {
                Ok(()) => "备份已删除；列表已刷新".to_string(),
                Err(message) => message,
            });
        }
    }

    pub const fn workspace(&self) -> Workspace {
        self.workspace
    }

    pub const fn provision(&self) -> &ProvisionState {
        &self.provision
    }

    pub fn provision_mut(&mut self) -> &mut ProvisionState {
        &mut self.provision
    }

    pub fn provision_reset(&mut self) {
        let selected = self
            .provision
            .menu_selected
            .min(ProvisionKind::ALL.len() - 1);
        self.provision = ProvisionState::default();
        self.provision.menu_selected = selected;
        self.provision.kind = ProvisionKind::ALL[selected];
        if self.workspace == Workspace::Provision {
            self.set_item_count(ProvisionKind::ALL.len());
            self.selected = selected;
        }
    }

    pub fn provision_begin_selected(&mut self) -> ProvisionKind {
        let index = self.selected.min(ProvisionKind::ALL.len() - 1);
        let kind = ProvisionKind::ALL[index];
        self.provision.menu_selected = index;
        self.provision.kind = kind;
        self.provision.field_selected = 0;
        self.provision.confirmation.clear();
        self.provision.message = None;
        self.provision.prepared = None;
        if kind == ProvisionKind::Convert {
            self.provision.stage = ProvisionStage::Planning;
        } else {
            let defaults = self.selected_device().map(|row| {
                (
                    row.onlyid.clone().unwrap_or_default(),
                    row.user.clone().unwrap_or_default(),
                    row.dept.clone().unwrap_or_default(),
                )
            });
            if let Some((label_id, user, dept)) = defaults {
                self.provision.form.label_id = label_id;
                self.provision.form.user = user;
                self.provision.form.dept = dept;
            }
            self.provision.stage = ProvisionStage::Form;
        }
        kind
    }

    pub fn provision_field_count(&self) -> usize {
        match self.provision.kind {
            ProvisionKind::Mode0 => 9,
            ProvisionKind::Mode1 => 8,
            ProvisionKind::Mode2 => 7,
            ProvisionKind::Mode3 => 8,
            ProvisionKind::Convert => 0,
        }
    }

    pub fn provision_move_field(&mut self, delta: isize) {
        let count = self.provision_field_count();
        if count == 0 {
            return;
        }
        self.provision.field_selected = if delta < 0 {
            self.provision
                .field_selected
                .saturating_sub(delta.unsigned_abs())
        } else {
            (self.provision.field_selected + delta as usize).min(count - 1)
        };
    }

    fn provision_field_slot(&self, display_index: usize) -> Option<usize> {
        let mode = self.provision.kind.mode()?;
        let mut slots = Vec::with_capacity(9);
        if matches!(mode, 0 | 3) {
            slots.push(0);
        }
        if matches!(mode, 0 | 1 | 3) {
            slots.push(1);
        }
        if matches!(mode, 0 | 1 | 2) {
            slots.push(2);
        }
        slots.extend([3, 4, 5, 6, 7, 8]);
        slots.get(display_index).copied()
    }

    pub fn provision_visible_fields(&self) -> Vec<(&'static str, &str, bool)> {
        let mut out = Vec::new();
        let mode = match self.provision.kind.mode() {
            Some(value) => value,
            None => return out,
        };
        if matches!(mode, 0 | 3) {
            out.push(("启动区 MiB", self.provision.form.boot_mib.as_str(), false));
        }
        if matches!(mode, 0 | 1 | 3) {
            out.push(("交换区 MiB", self.provision.form.share_mib.as_str(), false));
        }
        if matches!(mode, 0 | 1 | 2) {
            out.push((
                "保密区 MiB",
                self.provision.form.encrypt_mib.as_str(),
                false,
            ));
        }
        out.extend([
            ("标签标识", self.provision.form.label_id.as_str(), false),
            ("用户", self.provision.form.user.as_str(), false),
            ("部门", self.provision.form.dept.as_str(), false),
            ("标签", self.provision.form.label.as_str(), false),
            ("密码", self.provision.form.password.as_str(), true),
            ("卷标", self.provision.form.volume_label.as_str(), false),
        ]);
        out
    }

    fn provision_selected_field_mut(&mut self) -> Option<&mut String> {
        match self.provision_field_slot(self.provision.field_selected)? {
            0 => Some(&mut self.provision.form.boot_mib),
            1 => Some(&mut self.provision.form.share_mib),
            2 => Some(&mut self.provision.form.encrypt_mib),
            3 => Some(&mut self.provision.form.label_id),
            4 => Some(&mut self.provision.form.user),
            5 => Some(&mut self.provision.form.dept),
            6 => Some(&mut self.provision.form.label),
            7 => Some(&mut self.provision.form.password),
            8 => Some(&mut self.provision.form.volume_label),
            _ => None,
        }
    }

    pub fn provision_push_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        if let Some(field) = self.provision_selected_field_mut() {
            if field.chars().count() < 128 {
                field.push(ch);
                self.provision.message = None;
            }
        }
    }

    pub fn provision_backspace(&mut self) {
        if let Some(field) = self.provision_selected_field_mut() {
            field.pop();
            self.provision.message = None;
        }
    }

    pub fn provision_request(
        &mut self,
    ) -> Result<crate::application::provision::NewProvisionRequest, String> {
        let mode = self
            .provision
            .kind
            .mode()
            .ok_or_else(|| "免密改造不使用新盘表单".to_string())?;
        let parse = |value: &str, label: &str| -> Result<u64, String> {
            value
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| format!("{label} 必须为正整数 MiB"))
        };
        let boot_mib = matches!(mode, 0 | 3)
            .then(|| parse(&self.provision.form.boot_mib, "启动区"))
            .transpose()?;
        let share_mib = matches!(mode, 0 | 1 | 3)
            .then(|| parse(&self.provision.form.share_mib, "交换区"))
            .transpose()?;
        let encrypt_mib = matches!(mode, 0 | 1 | 2)
            .then(|| parse(&self.provision.form.encrypt_mib, "保密区"))
            .transpose()?;
        if self.provision.form.label_id.trim().is_empty()
            || self.provision.form.user.trim().is_empty()
            || self.provision.form.dept.trim().is_empty()
            || self.provision.form.label.trim().is_empty()
            || self.provision.form.password.is_empty()
        {
            return Err("标签标识、用户、部门、标签和密码均不能为空".into());
        }
        Ok(crate::application::provision::NewProvisionRequest {
            mode,
            boot_mib,
            share_mib,
            encrypt_mib,
            label_id: self.provision.form.label_id.trim().to_string(),
            user: self.provision.form.user.trim().to_string(),
            dept: self.provision.form.dept.trim().to_string(),
            label: self.provision.form.label.trim().to_string(),
            password: self.provision.form.password.clone(),
            volume_label: self.provision.form.volume_label.trim().to_string(),
        })
    }

    pub fn provision_set_planning(&mut self) {
        self.provision.stage = ProvisionStage::Planning;
        self.provision.message = Some("正在只读检查目标并生成精确制盘计划…".into());
    }

    pub fn provision_finish_plan(&mut self, result: Result<ProvisionPrepared, String>) {
        match result {
            Ok(prepared) => {
                self.provision.prepared = Some(prepared);
                self.provision.stage = ProvisionStage::Review;
                self.provision.message = None;
            }
            Err(message) => {
                self.provision.stage = if self.provision.kind == ProvisionKind::Convert {
                    ProvisionStage::Menu
                } else {
                    ProvisionStage::Form
                };
                self.provision.message = Some(message);
            }
        }
    }

    pub fn provision_begin_confirm(&mut self) {
        if self.provision.prepared.is_some() {
            self.provision.stage = ProvisionStage::Confirm;
            self.provision.confirmation.clear();
            self.provision.message = None;
        }
    }

    pub fn provision_push_confirmation(&mut self, ch: char) {
        if self.provision.stage == ProvisionStage::Confirm && self.provision.confirmation.len() < 16
        {
            self.provision.confirmation.push(ch);
            self.provision.message = None;
        }
    }

    pub fn provision_backspace_confirmation(&mut self) {
        if self.provision.stage == ProvisionStage::Confirm {
            self.provision.confirmation.pop();
            self.provision.message = None;
        }
    }

    pub fn provision_take_for_write(&mut self) -> Option<ProvisionPrepared> {
        if self.provision.stage != ProvisionStage::Confirm {
            return None;
        }
        if self.provision.confirmation != "YES" {
            self.provision.message = Some("必须精确输入 YES 才会执行破坏性写盘".into());
            return None;
        }
        let prepared = self.provision.prepared.take()?;
        self.provision.stage = ProvisionStage::Running;
        self.provision.message = Some("事务写盘进行中；退出请求会延迟到安全检查点".into());
        self.critical_operation = true;
        Some(prepared)
    }

    pub fn provision_finish_write(&mut self, result: Result<(), String>) {
        self.critical_operation = false;
        self.provision.stage = ProvisionStage::Result;
        self.provision.message = Some(match result {
            Ok(()) => "操作完成，写入/同步/读回安全链全部通过。请拔出重插后复核。".into(),
            Err(message) => message,
        });
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
        self.devices = devices;
        self.device_scan_pending = false;
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

    pub fn visible_device_indices(&self) -> Vec<usize> {
        if self.workspace == Workspace::Devices
            && self.inspect.is_none()
            && !self.active_search_query().is_empty()
        {
            self.search_matches.clone()
        } else {
            (0..self.devices.len()).collect()
        }
    }

    pub fn visible_device_count(&self) -> usize {
        if self.workspace == Workspace::Devices
            && self.inspect.is_none()
            && !self.active_search_query().is_empty()
        {
            self.search_matches.len()
        } else {
            self.devices.len()
        }
    }

    pub fn device_at_visible(&self, position: usize) -> Option<&crate::disk_scan::Row> {
        let index = if self.workspace == Workspace::Devices
            && self.inspect.is_none()
            && !self.active_search_query().is_empty()
        {
            *self.search_matches.get(position)?
        } else {
            position
        };
        self.devices.get(index)
    }

    pub fn visible_backup_indices(&self) -> Vec<usize> {
        if self.workspace == Workspace::Backups
            && self.inspect.is_none()
            && !self.active_search_query().is_empty()
        {
            self.search_matches.clone()
        } else {
            (0..self.backups.len()).collect()
        }
    }

    pub fn visible_backup_count(&self) -> usize {
        if self.workspace == Workspace::Backups
            && self.inspect.is_none()
            && !self.active_search_query().is_empty()
        {
            self.search_matches.len()
        } else {
            self.backups.len()
        }
    }

    pub fn backup_at_visible(
        &self,
        position: usize,
    ) -> Option<&crate::application::BackupWorkspaceItem> {
        let index = if self.workspace == Workspace::Backups
            && self.inspect.is_none()
            && !self.active_search_query().is_empty()
        {
            *self.search_matches.get(position)?
        } else {
            position
        };
        self.backups.get(index)
    }

    pub fn workspace_filter_active(&self) -> bool {
        self.inspect.is_none() && !self.active_search_query().is_empty()
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
        if self.workspace == Workspace::Devices && workspace != Workspace::Devices {
            self.pinned_disk = self.selected_device().map(|row| row.disk);
        }
        self.clear_search_matches();
        self.search_query.clear();
        self.input_buffer.clear();
        if self.input_mode == InputMode::Search {
            self.input_mode = InputMode::Normal;
        }
        self.workspace = workspace;
        self.selected = 0;
        let count = match workspace {
            Workspace::Devices => self.devices.len(),
            Workspace::Backups => self.backups.len(),
            Workspace::Provision => ProvisionKind::ALL.len(),
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

    /// Apply the one global command policy used while a destructive or otherwise
    /// critical worker owns the operation slot. Every command entry point (keys,
    /// command palette and direct dispatch) must pass through this guard.
    pub fn guard_critical_command(&mut self, command: NavCommand) -> Option<StateEffect> {
        if !self.critical_operation {
            return None;
        }
        if matches!(command, NavCommand::Quit | NavCommand::Escape) {
            self.exit_pending = true;
            Some(StateEffect::ExitDeferred)
        } else {
            self.notice = Some("关键操作仍在执行，完成前不能切换页面或启动其他任务。".to_string());
            Some(StateEffect::None)
        }
    }

    pub fn navigate(&mut self, command: NavCommand, viewport_height: usize) -> StateEffect {
        if let Some(effect) = self.guard_critical_command(command) {
            return effect;
        }

        if command == NavCommand::Escape {
            if self.workspace == Workspace::Provision
                && self.provision.stage != ProvisionStage::Menu
            {
                match self.provision.stage {
                    ProvisionStage::Running => {
                        self.exit_pending = true;
                    }
                    ProvisionStage::Confirm => {
                        self.provision.stage = ProvisionStage::Review;
                        self.provision.confirmation.clear();
                    }
                    ProvisionStage::Review => {
                        self.provision.stage = if self.provision.kind == ProvisionKind::Convert {
                            ProvisionStage::Menu
                        } else {
                            ProvisionStage::Form
                        };
                    }
                    ProvisionStage::Form | ProvisionStage::Planning | ProvisionStage::Result => {
                        self.provision_reset();
                    }
                    ProvisionStage::Menu => {}
                }
                return StateEffect::None;
            }
            if self.inspect.is_some() {
                self.close_inspect();
                return StateEffect::None;
            }
            if self.wizard.is_some() {
                self.wizard = None;
                return StateEffect::None;
            }
            if self.backup_delete.is_some() {
                self.backup_delete = None;
                return StateEffect::None;
            }
            if self.input_mode != InputMode::Normal {
                self.cancel_input();
                return StateEffect::None;
            }
            return StateEffect::ExitRequested;
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

        if let Some(inspect) = self.inspect.as_mut() {
            match command {
                NavCommand::Up => {
                    inspect.selected = inspect.selected.saturating_sub(1);
                    inspect.scroll = 0;
                }
                NavCommand::Down => {
                    if inspect.item_count > 0 {
                        inspect.selected = (inspect.selected + 1).min(inspect.item_count - 1);
                        inspect.scroll = 0;
                    }
                }
                NavCommand::Top => {
                    inspect.selected = 0;
                    inspect.scroll = 0;
                }
                NavCommand::Bottom => {
                    inspect.selected = inspect.item_count.saturating_sub(1);
                    inspect.scroll = 0;
                }
                NavCommand::HalfPageDown => {
                    inspect.scroll = inspect.scroll.saturating_add((viewport_height / 2).max(1));
                }
                NavCommand::HalfPageUp => {
                    inspect.scroll = inspect.scroll.saturating_sub((viewport_height / 2).max(1));
                }
                NavCommand::Left => {
                    inspect.mode = match inspect.mode {
                        InspectMode::Fields => InspectMode::Fields,
                        InspectMode::DecodedHex => InspectMode::Fields,
                        InspectMode::RawHex => InspectMode::DecodedHex,
                    };
                    inspect.scroll = 0;
                }
                NavCommand::Right => {
                    inspect.mode = match inspect.mode {
                        InspectMode::Fields => InspectMode::DecodedHex,
                        InspectMode::DecodedHex => InspectMode::RawHex,
                        InspectMode::RawHex => InspectMode::RawHex,
                    };
                    inspect.scroll = 0;
                }
                NavCommand::NextWorkspace
                | NavCommand::PreviousWorkspace
                | NavCommand::WorkspaceDevices
                | NavCommand::WorkspaceBackups
                | NavCommand::WorkspaceProvision => {}
                NavCommand::Search => {
                    self.input_buffer = self.search_query.clone();
                    self.input_mode = InputMode::Search;
                }
                NavCommand::CommandPalette => {
                    self.input_buffer.clear();
                    self.input_mode = InputMode::Command;
                }
                NavCommand::Help => self.input_mode = InputMode::Help,
                NavCommand::Quit => return StateEffect::ExitRequested,
                NavCommand::Escape
                | NavCommand::Refresh
                | NavCommand::BeginApply
                | NavCommand::BeginRestore
                | NavCommand::BeginBackupCreate
                | NavCommand::BeginBackupCreateDeep
                | NavCommand::BeginBackupDelete
                | NavCommand::BeginBackupPrune
                | NavCommand::VerifyBackup
                | NavCommand::OpenInspect
                | NavCommand::NextMatch
                | NavCommand::PreviousMatch => {}
            }
            return StateEffect::None;
        }

        match command {
            NavCommand::NextWorkspace | NavCommand::PreviousWorkspace => {
                self.switch_workspace(match self.workspace {
                    Workspace::Devices if command == NavCommand::NextWorkspace => {
                        Workspace::Backups
                    }
                    Workspace::Backups if command == NavCommand::NextWorkspace => {
                        Workspace::Provision
                    }
                    Workspace::Provision if command == NavCommand::NextWorkspace => {
                        Workspace::Devices
                    }
                    Workspace::Devices => Workspace::Provision,
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
            NavCommand::Left => {
                let target = match self.workspace {
                    Workspace::Devices => Workspace::Provision,
                    Workspace::Backups => Workspace::Devices,
                    Workspace::Provision => Workspace::Backups,
                };
                self.switch_workspace(target);
            }
            NavCommand::Right => {
                let target = match self.workspace {
                    Workspace::Devices => Workspace::Backups,
                    Workspace::Backups => Workspace::Provision,
                    Workspace::Provision => Workspace::Devices,
                };
                self.switch_workspace(target);
            }
            NavCommand::Refresh
            | NavCommand::BeginApply
            | NavCommand::BeginRestore
            | NavCommand::BeginBackupCreate
            | NavCommand::BeginBackupCreateDeep
            | NavCommand::BeginBackupDelete
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
