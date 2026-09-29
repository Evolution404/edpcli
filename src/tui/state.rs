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
#[path = "provision/state.rs"]
mod provision_state;
#[path = "shell/state.rs"]
mod shell_state;
#[path = "table_state.rs"]
mod table_state;

pub use backups_state::*;
pub use devices_state::*;
pub use inspect_state::*;
pub use navigation::*;
pub use provision_state::*;
pub use shell_state::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteKind {
    Restore,
    BackupCreate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardStage {
    Review,
    Confirm,
    Running,
    PostRestore,
    VolumeLabelInput,
    PasswordInput,
    EncryptedFormatConfirm,
    FormatConfirm,
    Formatting,
    ReinitializePassword,
    ReinitializePasswordConfirm,
    ReinitializeConfirm,
    Reinitializing,
    Result,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostRestoreLabelTarget {
    PlainFormat,
    EncryptedFormat,
    Reinitialize,
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
pub struct PostRestoreFormatIntent {
    pub disk: u32,
    pub outcome: crate::application::post_restore::MetadataRestoreOutcome,
    pub request: crate::application::post_restore::PartitionFormatRequest,
    pub volume_label: String,
}

#[derive(Debug, Clone)]
pub struct EncryptedPostRestoreFormatIntent {
    pub disk: u32,
    pub outcome: crate::application::post_restore::MetadataRestoreOutcome,
    pub request: crate::application::post_restore::PartitionFormatRequest,
    pub password: Option<crate::provision::SecretBytes>,
    pub volume_label: String,
}

#[derive(Debug, Clone)]
pub struct PostRestoreReinitializeIntent {
    pub disk: u32,
    pub outcome: crate::application::post_restore::MetadataRestoreOutcome,
    pub request: crate::application::post_restore::EncryptedPartitionReinitializeRequest,
    pub filesystem: crate::filesystem::FilesystemKind,
    pub volume_label: String,
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
    pub detail_expanded: bool,
    pub restore_outcome: Option<crate::application::post_restore::MetadataRestoreOutcome>,
    pub post_restore_selected: usize,
    pub pending_format: Option<crate::application::post_restore::PartitionFormatRequest>,
    pub volume_label_input: String,
    pub volume_label_target: Option<PostRestoreLabelTarget>,
    pub secret_input: crate::provision::SecretBytes,
    pub secret_first: crate::provision::SecretBytes,
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

    pub fn set_notice(&mut self, message: impl Into<String>) {
        self.shell.notice = Some(message.into());
        self.shell.notice_at = Some(std::time::Instant::now());
    }

    pub fn clear_notice(&mut self) {
        self.shell.notice = None;
        self.shell.notice_at = None;
    }

    pub fn wizard(&self) -> Option<&WizardState> {
        self.shell.wizard.as_ref()
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
        let stage = if kind == WriteKind::Restore {
            WizardStage::Review
        } else {
            WizardStage::Confirm
        };
        self.shell.input_mode = if stage == WizardStage::Confirm {
            InputMode::Confirm
        } else {
            InputMode::Normal
        };
        self.shell.wizard = Some(WizardState {
            stage,
            kind,
            disk,
            backup,
            expected_identity,
            confirmation: String::new(),
            message: None,
            detail_expanded: false,
            restore_outcome: None,
            post_restore_selected: 0,
            pending_format: None,
            volume_label_input: String::new(),
            volume_label_target: None,
            secret_input: crate::provision::SecretBytes::default(),
            secret_first: crate::provision::SecretBytes::default(),
            progress: None,
            progress_log: std::collections::VecDeque::new(),
        });
        true
    }

    pub fn advance_restore_review(&mut self) {
        if let Some(wizard) = self.shell.wizard.as_mut() {
            if wizard.kind == WriteKind::Restore && wizard.stage == WizardStage::Review {
                wizard.stage = WizardStage::Confirm;
                wizard.confirmation.clear();
                wizard.message = None;
                self.shell.input_mode = InputMode::Confirm;
            }
        }
    }

    pub fn toggle_wizard_detail(&mut self) {
        if let Some(wizard) = self.shell.wizard.as_mut() {
            wizard.detail_expanded = !wizard.detail_expanded;
        }
    }

    pub fn move_post_restore_selection(&mut self, delta: isize) {
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        if wizard.stage != WizardStage::PostRestore {
            return;
        }
        let Some(outcome) = wizard.restore_outcome.as_ref() else {
            return;
        };
        let len = outcome.assessment.partitions.len();
        if len == 0 {
            wizard.post_restore_selected = 0;
            return;
        }
        let current = wizard.post_restore_selected.min(len - 1) as isize;
        wizard.post_restore_selected = (current + delta).clamp(0, len as isize - 1) as usize;
    }

    fn selected_post_restore_format(
        wizard: &WizardState,
    ) -> Option<crate::application::post_restore::PartitionFormatRequest> {
        use crate::filesystem::FilesystemKind;

        let outcome = wizard.restore_outcome.as_ref()?;
        let partition = outcome
            .assessment
            .partitions
            .get(wizard.post_restore_selected)?;
        let hint = partition.filesystem_hint.as_deref().unwrap_or_default();
        let filesystem = if hint.eq_ignore_ascii_case("fat16") {
            FilesystemKind::Fat16
        } else if hint.eq_ignore_ascii_case("exfat") {
            FilesystemKind::ExFat
        } else if hint.eq_ignore_ascii_case("ntfs") || hint.eq_ignore_ascii_case("fat32") {
            return None;
        } else if partition.role.as_deref() == Some("boot") {
            FilesystemKind::Fat16
        } else {
            FilesystemKind::first_party_default()
        };
        Some(crate::application::post_restore::PartitionFormatRequest {
            partition_index: partition.index,
            filesystem,
        })
    }

    fn clear_post_restore_volume_label(wizard: &mut WizardState) {
        wizard.volume_label_input.clear();
        wizard.volume_label_target = None;
    }

    fn selected_post_restore_volume_label(wizard: &WizardState, partition_index: u32) -> String {
        wizard
            .restore_outcome
            .as_ref()
            .and_then(|outcome| {
                outcome
                    .partitions
                    .iter()
                    .find(|partition| partition.index == partition_index)
            })
            .and_then(|partition| partition.volume_label_hint.clone())
            .unwrap_or_default()
    }

    fn begin_volume_label_input(
        wizard: &mut WizardState,
        target: PostRestoreLabelTarget,
        request: crate::application::post_restore::PartitionFormatRequest,
    ) {
        wizard.volume_label_input =
            Self::selected_post_restore_volume_label(wizard, request.partition_index);
        wizard.volume_label_target = Some(target);
        wizard.pending_format = Some(request);
        wizard.confirmation.clear();
        wizard.message = None;
        wizard.stage = WizardStage::VolumeLabelInput;
    }

    pub fn begin_selected_post_restore_action(&mut self) {
        use crate::application::post_restore::PostRestorePartitionState;

        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        if wizard.stage != WizardStage::PostRestore {
            return;
        }
        let Some(outcome) = wizard.restore_outcome.as_ref() else {
            return;
        };
        let Some(partition) = outcome
            .assessment
            .partitions
            .get(wizard.post_restore_selected)
        else {
            return;
        };
        let state = partition.state;
        let requires_original_key = partition.requires_original_key;

        match state {
            PostRestorePartitionState::NeedsFormat if !requires_original_key => {
                let Some(request) = Self::selected_post_restore_format(wizard) else {
                    wizard.message = Some("当前便携格式化器尚不支持该文件系统。".into());
                    return;
                };
                Self::begin_volume_label_input(
                    wizard,
                    PostRestoreLabelTarget::PlainFormat,
                    request,
                );
                self.shell.input_mode = InputMode::Insert;
            }
            PostRestorePartitionState::NeedsFormat => {
                let Some(request) = Self::selected_post_restore_format(wizard) else {
                    wizard.message = Some("当前便携格式化器尚不支持该文件系统。".into());
                    return;
                };
                wizard.secret_input = crate::provision::SecretBytes::default();
                Self::begin_volume_label_input(
                    wizard,
                    PostRestoreLabelTarget::EncryptedFormat,
                    request,
                );
                wizard.message = Some("原密钥域已验证；可确认或修改恢复后的卷标。".into());
                self.shell.input_mode = InputMode::Insert;
            }
            PostRestorePartitionState::PasswordRequired => {
                let Some(request) = Self::selected_post_restore_format(wizard) else {
                    wizard.message = Some("当前便携格式化器尚不支持该文件系统。".into());
                    return;
                };
                wizard.volume_label_input =
                    Self::selected_post_restore_volume_label(wizard, request.partition_index);
                wizard.volume_label_target = Some(PostRestoreLabelTarget::EncryptedFormat);
                wizard.pending_format = Some(request);
                wizard.secret_input = crate::provision::SecretBytes::default();
                wizard.secret_first = crate::provision::SecretBytes::default();
                wizard.message = None;
                wizard.stage = WizardStage::PasswordInput;
                self.shell.input_mode = InputMode::Insert;
            }
            PostRestorePartitionState::CryptoMetadataInvalid => {
                let Some(request) = Self::selected_post_restore_format(wizard) else {
                    wizard.message = Some("当前便携格式化器尚不支持该文件系统。".into());
                    return;
                };
                wizard.volume_label_input =
                    Self::selected_post_restore_volume_label(wizard, request.partition_index);
                wizard.volume_label_target = Some(PostRestoreLabelTarget::Reinitialize);
                wizard.pending_format = Some(request);
                wizard.secret_input = crate::provision::SecretBytes::default();
                wizard.secret_first = crate::provision::SecretBytes::default();
                wizard.message = Some("将清空并重建该加密分区：旧 FileKey 与旧密码会失效。".into());
                wizard.stage = WizardStage::ReinitializePassword;
                self.shell.input_mode = InputMode::Insert;
            }
            PostRestorePartitionState::Usable => {
                wizard.message = Some("该分区已经可用，不需要执行破坏性操作。".into());
            }
            PostRestorePartitionState::Unsupported => {
                wizard.message = Some("当前状态无法可靠处理，拒绝猜测执行。".into());
            }
        }
    }

    fn secret_char_count(secret: &crate::provision::SecretBytes) -> usize {
        secret
            .as_bytes()
            .iter()
            .filter(|byte| (**byte & 0b1100_0000) != 0b1000_0000)
            .count()
    }

    pub fn wizard_secret_len(&self) -> usize {
        self.shell
            .wizard
            .as_ref()
            .map_or(0, |wizard| Self::secret_char_count(&wizard.secret_input))
    }

    pub fn push_wizard_secret_char(&mut self, ch: char) {
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        if !matches!(
            wizard.stage,
            WizardStage::PasswordInput
                | WizardStage::ReinitializePassword
                | WizardStage::ReinitializePasswordConfirm
        ) || wizard.secret_input.as_bytes().len() >= 128
        {
            return;
        }
        let mut bytes = wizard.secret_input.as_bytes().to_vec();
        let mut encoded = [0u8; 4];
        bytes.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
        wizard.secret_input = crate::provision::SecretBytes::new(&bytes);
        bytes.fill(0);
        encoded.fill(0);
        wizard.message = None;
    }

    pub fn backspace_wizard_secret(&mut self) {
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        if !matches!(
            wizard.stage,
            WizardStage::PasswordInput
                | WizardStage::ReinitializePassword
                | WizardStage::ReinitializePasswordConfirm
        ) {
            return;
        }
        let mut bytes = wizard.secret_input.as_bytes().to_vec();
        if !bytes.is_empty() {
            let mut cut = bytes.len() - 1;
            while cut > 0 && (bytes[cut] & 0b1100_0000) == 0b1000_0000 {
                cut -= 1;
            }
            bytes.truncate(cut);
        }
        wizard.secret_input = crate::provision::SecretBytes::new(&bytes);
        bytes.fill(0);
        wizard.message = None;
    }

    pub fn submit_wizard_secret(&mut self) {
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        if wizard.secret_input.is_empty() {
            wizard.message = Some("密码不能为空。".into());
            return;
        }
        match wizard.stage {
            WizardStage::PasswordInput => {
                wizard.confirmation.clear();
                wizard.message = None;
                wizard.stage = WizardStage::VolumeLabelInput;
                self.shell.input_mode = InputMode::Insert;
            }
            WizardStage::ReinitializePassword => {
                wizard.secret_first = wizard.secret_input.clone();
                wizard.secret_input = crate::provision::SecretBytes::default();
                wizard.message = None;
                wizard.stage = WizardStage::ReinitializePasswordConfirm;
                self.shell.input_mode = InputMode::Insert;
            }
            WizardStage::ReinitializePasswordConfirm => {
                if wizard.secret_first != wizard.secret_input {
                    wizard.secret_first = crate::provision::SecretBytes::default();
                    wizard.secret_input = crate::provision::SecretBytes::default();
                    wizard.message = Some("两次输入的新密码不一致，请重新输入。".into());
                    wizard.stage = WizardStage::ReinitializePassword;
                    self.shell.input_mode = InputMode::Insert;
                    return;
                }
                wizard.confirmation.clear();
                wizard.message = None;
                wizard.stage = WizardStage::VolumeLabelInput;
                self.shell.input_mode = InputMode::Insert;
            }
            _ => {}
        }
    }

    pub fn push_wizard_volume_label_char(&mut self, ch: char) {
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        if wizard.stage != WizardStage::VolumeLabelInput
            || wizard.volume_label_input.len() >= 128
            || ch.is_control()
        {
            return;
        }
        wizard.volume_label_input.push(ch);
        wizard.message = None;
    }

    pub fn backspace_wizard_volume_label(&mut self) {
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        if wizard.stage == WizardStage::VolumeLabelInput {
            wizard.volume_label_input.pop();
            wizard.message = None;
        }
    }

    pub fn submit_wizard_volume_label(&mut self) {
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        if wizard.stage != WizardStage::VolumeLabelInput {
            return;
        }
        let Some(request) = wizard.pending_format.as_ref() else {
            wizard.message = Some("缺少恢复后格式化请求。".into());
            return;
        };
        if let Err(message) =
            crate::filesystem::validate_volume_label(request.filesystem, &wizard.volume_label_input)
        {
            wizard.message = Some(message);
            return;
        }
        wizard.confirmation.clear();
        wizard.message = None;
        wizard.stage = match wizard.volume_label_target {
            Some(PostRestoreLabelTarget::PlainFormat) => WizardStage::FormatConfirm,
            Some(PostRestoreLabelTarget::EncryptedFormat) => WizardStage::EncryptedFormatConfirm,
            Some(PostRestoreLabelTarget::Reinitialize) => WizardStage::ReinitializeConfirm,
            None => {
                wizard.message = Some("缺少卷标输入目标。".into());
                return;
            }
        };
        self.shell.input_mode = InputMode::Confirm;
    }

    pub fn cancel_post_restore_volume_label(&mut self) {
        if let Some(wizard) = self.shell.wizard.as_mut() {
            if wizard.stage == WizardStage::VolumeLabelInput {
                wizard.stage = WizardStage::PostRestore;
                wizard.confirmation.clear();
                wizard.pending_format = None;
                wizard.volume_label_input.clear();
                wizard.volume_label_target = None;
                wizard.secret_input = crate::provision::SecretBytes::default();
                wizard.secret_first = crate::provision::SecretBytes::default();
                wizard.message = None;
                self.shell.input_mode = InputMode::Normal;
            }
        }
    }

    pub fn cancel_post_restore_secret_flow(&mut self) {
        if let Some(wizard) = self.shell.wizard.as_mut() {
            if matches!(
                wizard.stage,
                WizardStage::PasswordInput
                    | WizardStage::EncryptedFormatConfirm
                    | WizardStage::ReinitializePassword
                    | WizardStage::ReinitializePasswordConfirm
                    | WizardStage::ReinitializeConfirm
            ) {
                wizard.stage = WizardStage::PostRestore;
                wizard.confirmation.clear();
                wizard.pending_format = None;
                wizard.volume_label_input.clear();
                wizard.volume_label_target = None;
                wizard.secret_input = crate::provision::SecretBytes::default();
                wizard.secret_first = crate::provision::SecretBytes::default();
                wizard.message = None;
                self.shell.input_mode = InputMode::Normal;
            }
        }
    }

    pub fn submit_encrypted_format_confirmation(
        &mut self,
    ) -> Option<EncryptedPostRestoreFormatIntent> {
        let wizard = self.shell.wizard.as_mut()?;
        if wizard.stage != WizardStage::EncryptedFormatConfirm {
            return None;
        }
        if wizard.confirmation != "YES" {
            wizard.message = Some("加密格式化必须独立输入 YES。".into());
            return None;
        }
        let outcome = wizard.restore_outcome.clone()?;
        let request = wizard.pending_format.clone()?;
        let password = if wizard.secret_input.is_empty() {
            None
        } else {
            Some(wizard.secret_input.clone())
        };
        let volume_label = wizard.volume_label_input.clone();
        wizard.confirmation.clear();
        wizard.secret_input = crate::provision::SecretBytes::default();
        wizard.secret_first = crate::provision::SecretBytes::default();
        wizard.stage = WizardStage::Formatting;
        wizard.message = Some("正在使用原 FileKey 创建新的空加密文件系统。".into());
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = true;
        Some(EncryptedPostRestoreFormatIntent {
            disk: wizard.disk,
            outcome,
            request,
            password,
            volume_label,
        })
    }

    pub fn submit_reinitialize_confirmation(&mut self) -> Option<PostRestoreReinitializeIntent> {
        let wizard = self.shell.wizard.as_mut()?;
        if wizard.stage != WizardStage::ReinitializeConfirm {
            return None;
        }
        if wizard.confirmation != "YES" {
            wizard.message = Some("重建密钥域必须独立输入 YES。".into());
            return None;
        }
        let outcome = wizard.restore_outcome.clone()?;
        let format = wizard.pending_format.clone()?;
        let request = crate::application::post_restore::EncryptedPartitionReinitializeRequest::new(
            format.partition_index,
            wizard.secret_first.as_bytes(),
            wizard.secret_input.as_bytes(),
        )
        .ok()?;
        let volume_label = wizard.volume_label_input.clone();
        wizard.confirmation.clear();
        wizard.secret_input = crate::provision::SecretBytes::default();
        wizard.secret_first = crate::provision::SecretBytes::default();
        wizard.stage = WizardStage::Reinitializing;
        wizard.message = Some("正在生成新 FileKey、更新密钥记录并创建新的空加密文件系统。".into());
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = true;
        Some(PostRestoreReinitializeIntent {
            disk: wizard.disk,
            outcome,
            request,
            filesystem: format.filesystem,
            volume_label,
        })
    }

    pub fn cancel_post_restore_format(&mut self) {
        if let Some(wizard) = self.shell.wizard.as_mut() {
            if wizard.stage == WizardStage::FormatConfirm {
                wizard.stage = WizardStage::PostRestore;
                wizard.confirmation.clear();
                wizard.pending_format = None;
                Self::clear_post_restore_volume_label(wizard);
                wizard.message = None;
                self.shell.input_mode = InputMode::Normal;
            }
        }
    }

    pub fn submit_post_restore_format_confirmation(&mut self) -> Option<PostRestoreFormatIntent> {
        let wizard = self.shell.wizard.as_mut()?;
        if wizard.stage != WizardStage::FormatConfirm {
            return None;
        }
        if wizard.confirmation != "YES" {
            wizard.message = Some("格式化必须再次精确输入 YES。".into());
            return None;
        }
        let outcome = wizard.restore_outcome.clone()?;
        let request = wizard.pending_format.clone()?;
        let volume_label = wizard.volume_label_input.clone();
        wizard.stage = WizardStage::Formatting;
        wizard.confirmation.clear();
        wizard.message = Some("正在创建新的空文件系统；元数据恢复结果保持成功。".into());
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = true;
        Some(PostRestoreFormatIntent {
            disk: wizard.disk,
            outcome,
            request,
            volume_label,
        })
    }

    pub fn push_wizard_confirmation(&mut self, ch: char) {
        if let Some(wizard) = self.shell.wizard.as_mut() {
            if matches!(
                wizard.stage,
                WizardStage::Confirm
                    | WizardStage::FormatConfirm
                    | WizardStage::EncryptedFormatConfirm
                    | WizardStage::ReinitializeConfirm
            ) && wizard.confirmation.len() < 16
            {
                wizard.confirmation.push(ch);
                wizard.message = None;
            }
        }
    }

    pub fn backspace_wizard_confirmation(&mut self) {
        if let Some(wizard) = self.shell.wizard.as_mut() {
            if matches!(
                wizard.stage,
                WizardStage::Confirm
                    | WizardStage::FormatConfirm
                    | WizardStage::EncryptedFormatConfirm
                    | WizardStage::ReinitializeConfirm
            ) {
                wizard.confirmation.pop();
                wizard.message = None;
            }
        }
    }

    pub fn clear_wizard_confirmation(&mut self) {
        if let Some(wizard) = self.shell.wizard.as_mut() {
            wizard.confirmation.clear();
            wizard.message = None;
        }
    }

    pub fn submit_wizard_confirmation(&mut self) -> Option<WriteIntent> {
        let wizard = self.shell.wizard.as_mut()?;
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
        if let Some(wizard) = self.shell.wizard.as_mut() {
            if wizard.stage == WizardStage::Running {
                wizard.progress = Some(event.clone());
                if wizard.progress_log.len() == 200 {
                    wizard.progress_log.pop_front();
                }
                wizard.progress_log.push_back(event);
            }
        }
    }

    pub fn finish_restore(
        &mut self,
        result: Result<crate::application::post_restore::MetadataRestoreOutcome, String>,
    ) {
        self.shell.critical_operation = false;
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        match result {
            Ok(outcome) => {
                wizard.stage = WizardStage::PostRestore;
                wizard.progress = None;
                wizard.restore_outcome = Some(outcome);
                wizard.post_restore_selected = 0;
                wizard.pending_format = None;
                Self::clear_post_restore_volume_label(wizard);
                wizard.message = Some("元数据恢复成功；文件系统状态已完成只读检查。".into());
                self.shell.input_mode = InputMode::Normal;
            }
            Err(message) => {
                wizard.stage = WizardStage::Result;
                wizard.progress = None;
                wizard.message = Some(message);
                self.shell.input_mode = InputMode::Normal;
            }
        }
    }

    pub fn abort_post_restore_format(&mut self, message: impl Into<String>) {
        self.shell.critical_operation = false;
        if let Some(wizard) = self.shell.wizard.as_mut() {
            wizard.stage = WizardStage::PostRestore;
            wizard.pending_format = None;
            Self::clear_post_restore_volume_label(wizard);
            wizard.message = Some(message.into());
            self.shell.input_mode = InputMode::Normal;
        }
    }

    pub fn finish_post_restore_format(
        &mut self,
        result: crate::application::post_restore::PostRestoreFormatResult,
    ) {
        use crate::application::post_restore::PostRestorePartitionState;

        self.shell.critical_operation = false;
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        wizard.stage = WizardStage::PostRestore;
        wizard.pending_format = None;
        Self::clear_post_restore_volume_label(wizard);
        match result.result {
            Ok(()) => {
                if let Some(outcome) = wizard.restore_outcome.as_mut() {
                    if let Some(partition) = outcome
                        .assessment
                        .partitions
                        .iter_mut()
                        .find(|partition| partition.index == result.partition_index)
                    {
                        partition.state = PostRestorePartitionState::Usable;
                        partition.detail = "格式化完成并通过读回重新评估".into();
                    }
                }
                wizard.message = Some(format!(
                    "分区 {} 格式化完成并重新评估为可用。",
                    result.partition_index
                ));
            }
            Err(message) => {
                wizard.message = Some(format!(
                    "分区 {} 格式化失败：{}；元数据恢复仍保持成功。",
                    result.partition_index, message
                ));
            }
        }
        self.shell.input_mode = InputMode::Normal;
    }

    pub fn finish_post_restore_encrypted_format(
        &mut self,
        result: crate::application::post_restore::EncryptedPostRestoreFormatResult,
    ) {
        use crate::application::post_restore::{
            EncryptedPostRestoreError, PostRestorePartitionState,
        };
        use crate::provision::ExistingFileKeyError;

        self.shell.critical_operation = false;
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        wizard.pending_format = None;
        wizard.secret_input = crate::provision::SecretBytes::default();
        wizard.secret_first = crate::provision::SecretBytes::default();

        match result.result {
            Ok(()) => {
                if let Some(outcome) = wizard.restore_outcome.as_mut() {
                    if let Some(partition) = outcome
                        .assessment
                        .partitions
                        .iter_mut()
                        .find(|partition| partition.index == result.partition_index)
                    {
                        partition.state = PostRestorePartitionState::Usable;
                        partition.detail = "原 FileKey 格式化完成并通过读回重新评估".into();
                    }
                }
                wizard.stage = WizardStage::PostRestore;
                Self::clear_post_restore_volume_label(wizard);
                wizard.message = Some(format!(
                    "分区 {} 已使用原密钥域格式化并重新评估为可用。",
                    result.partition_index
                ));
                self.shell.input_mode = InputMode::Normal;
            }
            Err(EncryptedPostRestoreError::FileKey(
                ExistingFileKeyError::PasswordRequired | ExistingFileKeyError::PasswordMismatch,
            )) => {
                if let Some(outcome) = wizard.restore_outcome.as_mut() {
                    if let Some(partition) = outcome
                        .assessment
                        .partitions
                        .iter_mut()
                        .find(|partition| partition.index == result.partition_index)
                    {
                        partition.state = PostRestorePartitionState::PasswordRequired;
                    }
                }
                wizard.pending_format =
                    Some(crate::application::post_restore::PartitionFormatRequest {
                        partition_index: result.partition_index,
                        filesystem: result.filesystem,
                    });
                wizard.stage = WizardStage::PasswordInput;
                wizard.message = Some("原密码验证失败，请重新输入原密码。".into());
                self.shell.input_mode = InputMode::Insert;
            }
            Err(EncryptedPostRestoreError::FileKey(
                ExistingFileKeyError::UnsupportedEncryptMode
                | ExistingFileKeyError::FileKeyCrcMismatch
                | ExistingFileKeyError::MalformedKeyRecord,
            )) => {
                if let Some(outcome) = wizard.restore_outcome.as_mut() {
                    if let Some(partition) = outcome
                        .assessment
                        .partitions
                        .iter_mut()
                        .find(|partition| partition.index == result.partition_index)
                    {
                        partition.state = PostRestorePartitionState::CryptoMetadataInvalid;
                    }
                }
                wizard.stage = WizardStage::PostRestore;
                wizard.message = Some(
                    "密钥记录无法可靠验证；已转为“加密元数据异常”，可选择重建加密分区。".into(),
                );
                self.shell.input_mode = InputMode::Normal;
            }
            Err(EncryptedPostRestoreError::Operation(message)) => {
                wizard.stage = WizardStage::PostRestore;
                Self::clear_post_restore_volume_label(wizard);
                wizard.message = Some(format!(
                    "分区 {} 加密格式化失败：{}；元数据恢复仍保持成功。",
                    result.partition_index, message
                ));
                self.shell.input_mode = InputMode::Normal;
            }
        }
    }

    pub fn abort_post_restore_encrypted_action(&mut self, message: impl Into<String>) {
        self.shell.critical_operation = false;
        if let Some(wizard) = self.shell.wizard.as_mut() {
            wizard.stage = WizardStage::PostRestore;
            wizard.pending_format = None;
            Self::clear_post_restore_volume_label(wizard);
            wizard.secret_input = crate::provision::SecretBytes::default();
            wizard.secret_first = crate::provision::SecretBytes::default();
            wizard.message = Some(message.into());
            self.shell.input_mode = InputMode::Normal;
        }
    }

    pub fn finish_post_restore_reinitialize(
        &mut self,
        result: crate::application::post_restore::EncryptedPartitionReinitializeResult,
    ) {
        use crate::application::post_restore::PostRestorePartitionState;

        self.shell.critical_operation = false;
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        wizard.pending_format = None;
        Self::clear_post_restore_volume_label(wizard);
        wizard.secret_input = crate::provision::SecretBytes::default();
        wizard.secret_first = crate::provision::SecretBytes::default();
        wizard.stage = WizardStage::PostRestore;
        self.shell.input_mode = InputMode::Normal;

        match result.result {
            Ok(()) => {
                if let Some(outcome) = wizard.restore_outcome.as_mut() {
                    if let Some(partition) = outcome
                        .assessment
                        .partitions
                        .iter_mut()
                        .find(|partition| partition.index == result.partition_index)
                    {
                        partition.state = PostRestorePartitionState::Usable;
                        partition.detail = "新密钥域与新空文件系统已通过读回验证".into();
                    }
                }
                wizard.message = Some(format!(
                    "分区 {} 已重建密钥域并重新评估为可用。",
                    result.partition_index
                ));
            }
            Err(message) => {
                wizard.message = Some(format!(
                    "分区 {} 重建失败：{}；元数据恢复仍保持成功。",
                    result.partition_index, message
                ));
            }
        }
    }

    pub fn finish_write(&mut self, result: Result<(), String>) {
        self.shell.critical_operation = false;
        if let Some(wizard) = self.shell.wizard.as_mut() {
            wizard.stage = WizardStage::Result;
            wizard.progress = None;
            wizard.message = Some(match result {
                Ok(()) if wizard.kind == WriteKind::BackupCreate => {
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
            .shell
            .pinned_disk
            .is_some_and(|disk| !devices.iter().any(|row| row.disk == disk))
        {
            self.shell.pinned_disk = None;
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
            if self.shell.pinned_disk.is_none() {
                self.provision.stage = ProvisionStage::SelectDisk;
                self.set_item_count(self.provision_selectable_devices().count());
            } else if self.selected_device().is_none() {
                self.shell.pinned_disk = None;
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
                        if self.workspace_filter_active() {
                            self.shell
                                .search_matches
                                .iter()
                                .position(|value| *value == index)
                        } else {
                            Some(index)
                        }
                    })
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
        self.visible_device_indices().get(position).copied()
    }

    pub fn backup_source_index_at_visible(&self, position: usize) -> Option<usize> {
        self.visible_backup_indices().get(position).copied()
    }

    pub fn visible_device_indices(&self) -> Vec<usize> {
        let indices = if self.shell.workspace == Workspace::Devices
            && !self.active_search_query().is_empty()
        {
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
        let indices = if self.shell.workspace == Workspace::Backups
            && !self.active_search_query().is_empty()
        {
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
                .shell
                .pinned_disk
                .and_then(|disk| self.devices.rows.iter().find(|row| row.disk == disk)),
        }
    }

    fn provision_selectable_device_indices(&self) -> Vec<usize> {
        let mut indices = self
            .devices
            .rows
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
        self.shell.pinned_disk = Some(disk);
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
        self.shell.pinned_disk = Some(disk);
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
            if let Some(path) = selected_path {
                let source_index = self.backups.rows.iter().position(|row| row.path == path);
                self.shell.selected = source_index
                    .and_then(|index| {
                        if self.workspace_filter_active() {
                            self.shell
                                .search_matches
                                .iter()
                                .position(|value| *value == index)
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
            self.shell.pinned_disk = self.selected_device().map(|row| row.disk);
        }
        if workspace == Workspace::Provision {
            if let Some(disk) = self.provision.target_disk {
                self.shell.pinned_disk = Some(disk);
            } else {
                self.shell.pinned_disk = None;
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
                    .rows
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

    pub fn pane_viewport(&self, pane: crate::tui::pane::PaneId) -> &crate::tui::pane::PaneViewport {
        if pane.is_inspect() {
            self.inspect
                .advanced
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
            self.inspect
                .advanced
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
        self.focus_devices_pane(crate::tui::pane::PaneId::DevicesTree);
        Ok(None)
    }

    pub fn device_info_selected_key(&self) -> DeviceInfoNodeKey {
        let selected = self.devices.info_selected;
        if self
            .device_info_tree_rows()
            .iter()
            .any(|row| row.key == selected)
        {
            selected
        } else {
            DeviceInfoNodeKey::Capacity
        }
    }

    fn reconcile_device_info_selection(&mut self) {
        self.devices.info_selected = self.device_info_selected_key();
        self.devices
            .pane_focus
            .viewport_mut(crate::tui::pane::PaneId::DevicesDetail)
            .scroll_y
            .top();
    }

    pub fn device_info_tree_rows(&self) -> Vec<DeviceInfoTreeNode> {
        fn size_text(bytes: u64) -> String {
            if bytes >= 1_000_000_000 {
                format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
            } else if bytes >= 1_000_000 {
                format!("{:.2} MB", bytes as f64 / 1_000_000.0)
            } else if bytes >= 1_000 {
                format!("{:.2} kB", bytes as f64 / 1_000.0)
            } else {
                format!("{bytes} B")
            }
        }

        fn reliability_label(row: &crate::disk_scan::Row) -> &'static str {
            use crate::application::media_identity::SerialQuality;
            match row
                .identity_pin
                .as_ref()
                .map(|pin| pin.snapshot.hardware.serial_quality)
            {
                Some(SerialQuality::Usable) => "强",
                Some(SerialQuality::Suspicious) => "中",
                Some(SerialQuality::Missing) if row.device_id.is_some() && row.onlyid.is_some() => {
                    "中"
                }
                Some(SerialQuality::Missing) => "弱",
                None if row.serial.is_some() || row.device_id.is_some() || row.onlyid.is_some() => {
                    "待确认"
                }
                None => "未知",
            }
        }

        fn backup_summary(row: &crate::disk_scan::Row) -> String {
            let status = if row.probe_error.is_some() || row.denied {
                "异常"
            } else {
                "正常"
            };
            match row.n_possible_baks {
                0 => format!("{status} · {}", row.n_baks),
                possible => format!("{status} · {}+{possible}", row.n_baks),
            }
        }

        let expanded = |key| self.devices.info_expanded.contains(&key);
        let mut rows = vec![DeviceInfoTreeNode {
            key: DeviceInfoNodeKey::Capacity,
            depth: 0,
            label: "容量布局".into(),
            value: self.selected_device().map(|row| size_text(row.size)),
            expandable: true,
            expanded: expanded(DeviceInfoNodeKey::Capacity),
        }];

        if expanded(DeviceInfoNodeKey::Capacity) {
            if let Some(row) = self.selected_device() {
                if let Ok(model) = row.canonical_layout() {
                    let collapsed = model.collapsed_tail_model();
                    for segment in &collapsed.segments {
                        let is_tail = segment.kind == crate::disk_layout::DiskRegionKind::Tail;
                        let key = if is_tail {
                            DeviceInfoNodeKey::TailGroup
                        } else {
                            DeviceInfoNodeKey::LayoutSegment {
                                start_lba: segment.start_lba,
                                kind: segment.kind,
                            }
                        };
                        rows.push(DeviceInfoTreeNode {
                            key,
                            depth: 1,
                            label: segment.label.clone(),
                            value: Some(size_text(
                                segment
                                    .sector_count
                                    .saturating_mul(crate::common::SECTOR as u64),
                            )),
                            expandable: is_tail && model.tail_group().is_some(),
                            expanded: is_tail && expanded(DeviceInfoNodeKey::TailGroup),
                        });
                        if is_tail && expanded(DeviceInfoNodeKey::TailGroup) {
                            if let Some(tail) = model.tail_group() {
                                rows.extend(tail.children.iter().map(|child| {
                                    DeviceInfoTreeNode {
                                        key: DeviceInfoNodeKey::LayoutSegment {
                                            start_lba: child.start_lba,
                                            kind: child.kind,
                                        },
                                        depth: 2,
                                        label: child.label.clone(),
                                        value: Some(size_text(
                                            child
                                                .sector_count
                                                .saturating_mul(crate::common::SECTOR as u64),
                                        )),
                                        expandable: false,
                                        expanded: false,
                                    }
                                }));
                            }
                        }
                    }
                }
            }
        }

        rows.extend([
            DeviceInfoTreeNode {
                key: DeviceInfoNodeKey::Identity,
                depth: 0,
                label: "身份与协议".into(),
                value: self
                    .selected_device()
                    .map(|row| reliability_label(row).into()),
                expandable: false,
                expanded: false,
            },
            DeviceInfoTreeNode {
                key: DeviceInfoNodeKey::Status,
                depth: 0,
                label: "状态与备份".into(),
                value: self.selected_device().map(backup_summary),
                expandable: false,
                expanded: false,
            },
        ]);
        rows
    }

    pub fn device_info_move_tree(&mut self, delta: isize) {
        let rows = self.device_info_tree_rows();
        if rows.is_empty() {
            return;
        }
        let selected = self.device_info_selected_key();
        let current = rows.iter().position(|row| row.key == selected).unwrap_or(0);
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize).min(rows.len() - 1)
        };
        self.devices.info_selected = rows[next].key;
        self.devices
            .pane_focus
            .viewport_mut(crate::tui::pane::PaneId::DevicesTree)
            .selected = Some(next);
    }

    pub fn device_info_jump_tree(&mut self, to_end: bool) {
        let rows = self.device_info_tree_rows();
        if rows.is_empty() {
            return;
        }
        let index = if to_end { rows.len() - 1 } else { 0 };
        self.devices.info_selected = rows[index].key;
        let viewport = self
            .devices
            .pane_focus
            .viewport_mut(crate::tui::pane::PaneId::DevicesTree);
        viewport.selected = Some(index);
        viewport.scroll_y.top();
    }

    pub fn device_info_toggle_selected(&mut self) {
        let key = self.device_info_selected_key();
        self.devices.info_selected = key;
        if !matches!(
            key,
            DeviceInfoNodeKey::Capacity | DeviceInfoNodeKey::TailGroup
        ) {
            return;
        }
        if !self.devices.info_expanded.remove(&key) {
            self.devices.info_expanded.insert(key);
        }
        let rows = self.device_info_tree_rows();
        if !rows.iter().any(|row| row.key == self.devices.info_selected) {
            self.devices.info_selected = DeviceInfoNodeKey::Capacity;
        }
    }

    pub fn device_info_focus_detail(&mut self) {
        self.focus_devices_pane(crate::tui::pane::PaneId::DevicesDetail);
        self.pane_viewport_mut(crate::tui::pane::PaneId::DevicesDetail)
            .scroll_y
            .top();
    }

    fn device_info_detail_line_count(&self) -> usize {
        match self.device_info_selected_key() {
            DeviceInfoNodeKey::Identity => 20,
            DeviceInfoNodeKey::Capacity => self
                .selected_device()
                .and_then(|row| row.canonical_layout().ok())
                .map(|model| model.collapsed_tail_model().segments.len() + 11)
                .unwrap_or(3),
            DeviceInfoNodeKey::TailGroup => self
                .selected_device()
                .and_then(|row| row.canonical_layout().ok())
                .and_then(|model| model.tail_group().map(|tail| tail.children.len() + 13))
                .unwrap_or(3),
            DeviceInfoNodeKey::LayoutSegment { .. } => 18,
            DeviceInfoNodeKey::Status => self
                .selected_device()
                .map(|row| 12 + row.n_baks + row.n_possible_baks)
                .unwrap_or(12),
            DeviceInfoNodeKey::Backups => 12,
            DeviceInfoNodeKey::Protocol => 20,
        }
    }

    pub fn disk_layout_tail_expansion(&self) -> super::disk_layout::TailExpansion {
        self.shell.disk_layout_tail
    }

    pub fn toggle_disk_layout_tail(&mut self) {
        self.shell.disk_layout_tail.toggle();
        self.shell.disk_layout_selected = 0;
    }

    pub fn disk_layout_selected(&self) -> usize {
        self.shell.disk_layout_selected
    }

    pub fn disk_layout_move_selection(&mut self, delta: isize, count: usize) {
        self.shell.disk_layout_selected = if delta < 0 {
            self.shell
                .disk_layout_selected
                .saturating_sub(delta.unsigned_abs())
        } else {
            self.shell
                .disk_layout_selected
                .saturating_add(delta as usize)
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
            self.shell.disk_layout_tail,
        );
        let visible = presentation.visible_model();
        let segment = visible.segments.get(self.shell.disk_layout_selected)?;
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
                .devices
                .pane_focus
                .cycle(&crate::tui::pane::PaneId::DEVICES_ORDER, reverse),
            Workspace::Backups => self
                .backups
                .pane_focus
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
                    (PaneId::DevicesList, _, 1) => Some(PaneId::DevicesTree),
                    (PaneId::DevicesTree | PaneId::DevicesDetail, _, -1) => {
                        Some(PaneId::DevicesList)
                    }
                    (PaneId::DevicesTree, 1, _) => Some(PaneId::DevicesDetail),
                    (PaneId::DevicesDetail, -1, _) => Some(PaneId::DevicesTree),
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
                .inspect
                .advanced
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
                self.shell
                    .horizontal_scroll
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
                .inspect
                .advanced
                .as_ref()
                .filter(|_| self.shell.workspace == Workspace::Inspect)
            {
                if advanced.stage == AdvancedInspectStage::Running {
                    self.set_notice("全盘检查正在后台读取结构，请等待完成。");
                } else if advanced.prompt.is_some() {
                    self.advanced_inspect_cancel_prompt();
                } else if !self.advanced_inspect_close_sector() {
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
            if self.shell.wizard.is_some() {
                self.shell.wizard = None;
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
                self.pane_viewport_mut(crate::tui::pane::PaneId::DevicesDetail)
                    .scroll_y
                    .top();
            }
            NavCommand::Bottom
                if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesDetail =>
            {
                let content_len = self.device_info_detail_line_count();
                self.pane_viewport_mut(crate::tui::pane::PaneId::DevicesDetail)
                    .scroll_y
                    .bottom(content_len, viewport_height);
            }
            NavCommand::HalfPageDown
                if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesDetail =>
            {
                let content_len = self.device_info_detail_line_count();
                self.pane_viewport_mut(crate::tui::pane::PaneId::DevicesDetail)
                    .scroll_y
                    .half_page_down(content_len, viewport_height);
            }
            NavCommand::HalfPageUp
                if self.shell.workspace == Workspace::Devices
                    && self.devices_focused_pane() == crate::tui::pane::PaneId::DevicesDetail =>
            {
                self.pane_viewport_mut(crate::tui::pane::PaneId::DevicesDetail)
                    .scroll_y
                    .half_page_up(viewport_height);
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
            NavCommand::HalfPageDown => {
                if self.shell.item_count > 0 {
                    let delta = (viewport_height / 2).max(1);
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
            NavCommand::HalfPageUp => {
                let delta = (viewport_height / 2).max(1);
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
            NavCommand::Help => self.shell.input_mode = InputMode::Help,
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
