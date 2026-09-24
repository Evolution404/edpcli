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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvancedInspectSource {
    Disk(u32),
    Backup(std::path::PathBuf),
}

impl AdvancedInspectSource {
    pub fn label(&self) -> String {
        match self {
            Self::Disk(disk) => format!("物理盘 disk{disk}"),
            Self::Backup(path) => format!("备份 {}", path.display()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedInspectStage {
    Form,
    Running,
    Result,
}

#[derive(Debug, Clone)]
pub struct AdvancedInspectForm {
    pub mode: crate::application::inspect::AdvancedInspectMode,
    pub lba_spec: String,
    pub count: String,
    pub device_id: String,
    pub export_dir: String,
    pub field_selected: usize,
}

impl Default for AdvancedInspectForm {
    fn default() -> Self {
        Self {
            mode: crate::application::inspect::AdvancedInspectMode::Meta,
            lba_spec: "0-12".into(),
            count: String::new(),
            device_id: String::new(),
            export_dir: String::new(),
            field_selected: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdvancedInspectState {
    pub source: AdvancedInspectSource,
    pub stage: AdvancedInspectStage,
    pub form: AdvancedInspectForm,
    pub result: Option<crate::application::inspect::AdvancedInspectWorkspace>,
    pub selected: usize,
    pub scroll: usize,
    pub message: Option<String>,
}

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

#[derive(Debug, Clone)]
pub struct BackupDeleteState {
    pub stage: WizardStage,
    pub path: std::path::PathBuf,
    pub expected_sha256: String,
    pub confirmation: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupBatchDeleteStage {
    Planning,
    Review,
    Confirm,
    Running,
    Result,
}

pub struct BackupBatchDeleteState {
    pub stage: BackupBatchDeleteStage,
    pub prepared: Option<crate::application::backup::DeletePlan>,
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
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionBarKind {
    Free,
    Boot,
    Share,
    Encrypt,
    Compatibility,
}

impl ProvisionKind {
    pub const ALL: [Self; 5] = [
        Self::Mode0,
        Self::Mode1,
        Self::Mode2,
        Self::Mode3,
        Self::Offline,
    ];

    pub const fn mode(self) -> Option<u8> {
        match self {
            Self::Mode0 => Some(0),
            Self::Mode1 => Some(1),
            Self::Mode2 => Some(2),
            Self::Mode3 => Some(3),
            Self::Offline => None,
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Mode0 => "模式 0 · 缺省三分区",
            Self::Mode1 => "模式 1 · 启动/交换二合一",
            Self::Mode2 => "模式 2 · 整盘加密",
            Self::Mode3 => "模式 3 · 内外网双分区",
            Self::Offline => "离线工具 · LBA 快照转换",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Mode0 => "启动区 + 交换区 + 保密区",
            Self::Mode1 => "启动/交换二合一区 + 保密区",
            Self::Mode2 => "兼容保留区 + 保密区（整盘加密）",
            Self::Mode3 => "启动区 + 交换区（内外网双分区）",
            Self::Offline => "从快照生成离线转换结果，不写物理盘",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionStage {
    SelectDisk,
    BackupPrompt,
    BackupSaving,
    Menu,
    Form,
    Planning,
    Review,
    ExportPath,
    Exporting,
    Confirm,
    Running,
    Result,
    OfflineForm,
    OfflineRunning,
    OfflineResult,
}

#[derive(Debug, Clone)]
pub enum ProvisionPrepared {
    New(Box<crate::application::provision::PreparedNewProvision>),
}

#[derive(Debug, Clone)]
pub struct ProvisionForm {
    pub boot_input_mode: crate::provision::CapacityInputMode,
    pub share_input_mode: crate::provision::CapacityInputMode,
    pub encrypt_input_mode: crate::provision::CapacityInputMode,
    pub boot_quick_unit: crate::provision::QuickCapacityUnit,
    pub share_quick_unit: crate::provision::QuickCapacityUnit,
    pub encrypt_quick_unit: crate::provision::QuickCapacityUnit,
    boot_capacity_edited: bool,
    share_capacity_edited: bool,
    encrypt_capacity_edited: bool,
    pub boot_capacity_source: crate::provision::CapacitySource,
    pub share_capacity_source: crate::provision::CapacitySource,
    pub encrypt_capacity_source: crate::provision::CapacitySource,
    pub boot_start_lba: String,
    pub share_start_lba: String,
    pub encrypt_start_lba: String,
    pub boot_mib: String,
    pub boot_sectors: String,
    pub share_mib: String,
    pub share_sectors: String,
    pub encrypt_mib: String,
    pub encrypt_sectors: String,
    pub label_id: String,
    pub user: String,
    pub dept: String,
    pub label: String,
    pub password: String,
    pub volume_label: String,
    pub format_boot: bool,
    pub format_share: bool,
    pub format_encrypt: bool,
    pub share_label: String,
    pub encrypt_label: String,
    pub boot_fs: crate::provision::OfficialFilesystemFormat,
    pub share_fs: crate::provision::OfficialFilesystemFormat,
    pub encrypt_fs: crate::provision::OfficialFilesystemFormat,
    pub force_change_password: bool,
    pub cancel_password_complexity_check: bool,
    pub max_share_password_errors: String,
    pub max_encrypt_password_errors: String,
}

fn toggle_supported_fs(
    value: crate::provision::OfficialFilesystemFormat,
) -> crate::provision::OfficialFilesystemFormat {
    match value {
        crate::provision::OfficialFilesystemFormat::Fat16 => {
            crate::provision::OfficialFilesystemFormat::ExFat
        }
        _ => crate::provision::OfficialFilesystemFormat::Fat16,
    }
}

#[derive(Debug, Clone, Default)]
pub struct OfflineConvertForm {
    pub source_dir: String,
    pub device_id: String,
    pub size_gb: String,
    pub output_dir: String,
}

#[derive(Debug, Clone)]
pub struct OfflineConvertView {
    pub reports: Vec<crate::sectors::ConvertReport>,
    pub share: u64,
    pub enc_start: u64,
    pub enc_size: u64,
    pub crc: u32,
    pub k0: u32,
    pub output_dir: Option<std::path::PathBuf>,
}

impl Default for ProvisionForm {
    fn default() -> Self {
        Self {
            boot_input_mode: crate::provision::CapacityInputMode::Exact,
            share_input_mode: crate::provision::CapacityInputMode::Quick,
            encrypt_input_mode: crate::provision::CapacityInputMode::Quick,
            boot_quick_unit: crate::provision::QuickCapacityUnit::MiB,
            share_quick_unit: crate::provision::QuickCapacityUnit::MiB,
            encrypt_quick_unit: crate::provision::QuickCapacityUnit::MiB,
            boot_capacity_edited: false,
            share_capacity_edited: false,
            encrypt_capacity_edited: false,
            boot_capacity_source: crate::provision::CapacitySource::SystemDefault,
            share_capacity_source: crate::provision::CapacitySource::SystemDefault,
            encrypt_capacity_source: crate::provision::CapacitySource::SystemDefault,
            boot_start_lba: "63".into(),
            share_start_lba: String::new(),
            encrypt_start_lba: String::new(),
            boot_mib: "512".into(),
            boot_sectors: crate::provision::DEFAULT_MODE0_BOOT_SECTORS.to_string(),
            share_mib: "1024".into(),
            share_sectors: String::new(),
            encrypt_mib: "1024".into(),
            encrypt_sectors: String::new(),
            label_id: crate::provision::OnlyId::random_candidate()
                .map(|value| value.text().to_string())
                .unwrap_or_else(|_| "1".into()),
            user: String::new(),
            dept: String::new(),
            label: crate::provision::DEFAULT_SAFE6_LABEL.into(),
            password: "0000aaaa".into(),
            volume_label: "启动区".into(),
            format_boot: false,
            format_share: false,
            format_encrypt: false,
            share_label: "交换区".into(),
            encrypt_label: "保密区".into(),
            boot_fs: crate::provision::OfficialFilesystemFormat::Fat16,
            share_fs: crate::provision::OfficialFilesystemFormat::ExFat,
            encrypt_fs: crate::provision::OfficialFilesystemFormat::ExFat,
            force_change_password: false,
            cancel_password_complexity_check: false,
            max_share_password_errors: u8::MAX.to_string(),
            max_encrypt_password_errors: u8::MAX.to_string(),
        }
    }
}

impl ProvisionForm {
    fn format_sector_unit_3(sectors: u64, sectors_per_unit: u64) -> String {
        let scaled =
            ((sectors as u128) * 1_000 + (sectors_per_unit as u128 / 2)) / sectors_per_unit as u128;
        format!("{}.{:03}", scaled / 1_000, scaled % 1_000)
    }

    fn parse_decimal_unit_to_sectors_rounded(
        value: &str,
        sectors_per_unit: u64,
        unit_name: &str,
        label: &str,
    ) -> Result<u64, String> {
        let value = value.trim();
        if value.is_empty() {
            return Err(format!("{label} {unit_name} 不能为空"));
        }
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        if whole.is_empty()
            || !whole.chars().all(|ch| ch.is_ascii_digit())
            || !fraction.chars().all(|ch| ch.is_ascii_digit())
            || fraction.len() > 21
        {
            return Err(format!("{label} {unit_name} 必须是正数，最多 21 位小数"));
        }
        let denominator = 10u128
            .checked_pow(fraction.len() as u32)
            .ok_or_else(|| format!("{label} {unit_name} 精度过高"))?;
        let whole = whole
            .parse::<u128>()
            .map_err(|_| format!("{label} {unit_name} 数值无效"))?;
        let fraction = if fraction.is_empty() {
            0
        } else {
            fraction
                .parse::<u128>()
                .map_err(|_| format!("{label} {unit_name} 数值无效"))?
        };
        let numerator = whole
            .checked_mul(denominator)
            .and_then(|value| value.checked_add(fraction))
            .ok_or_else(|| format!("{label} {unit_name} 容量溢出"))?;
        if numerator == 0 {
            return Err(format!("{label} {unit_name} 必须大于 0"));
        }
        let scaled = numerator
            .checked_mul(sectors_per_unit as u128)
            .ok_or_else(|| format!("{label} {unit_name} 容量溢出"))?;
        let quotient = scaled / denominator;
        let remainder = scaled % denominator;
        let rounded = quotient + u128::from(remainder.saturating_mul(2) >= denominator);
        u64::try_from(rounded).map_err(|_| format!("{label} {unit_name} 容量溢出"))
    }

    fn parse_mib_to_sectors(value: &str, label: &str) -> Result<u64, String> {
        Self::parse_decimal_unit_to_sectors_rounded(value, 2_048, "MiB", label)
    }

    fn parse_gib_to_sectors(value: &str, label: &str) -> Result<u64, String> {
        Self::parse_decimal_unit_to_sectors_rounded(value, 2_097_152, "GiB", label)
    }

    fn resolve_quick_sectors(
        quick: &str,
        exact: &str,
        unit: crate::provision::QuickCapacityUnit,
        edited: bool,
        label: &str,
    ) -> Result<u64, String> {
        if let Some(sectors) = exact.parse::<u64>().ok().filter(|_| !edited) {
            let generated = match unit {
                crate::provision::QuickCapacityUnit::MiB => {
                    Self::format_sector_unit_3(sectors, 2_048)
                }
                crate::provision::QuickCapacityUnit::GiB => {
                    Self::format_sector_unit_3(sectors, 2_097_152)
                }
            };
            if quick == generated {
                return Ok(sectors);
            }
        }
        match unit {
            crate::provision::QuickCapacityUnit::MiB => Self::parse_mib_to_sectors(quick, label),
            crate::provision::QuickCapacityUnit::GiB => Self::parse_gib_to_sectors(quick, label),
        }
    }

    fn apply_prefill(&mut self, prefill: &crate::provision::ProvisionPrefill) {
        use crate::provision::CapacityInputMode;
        self.boot_capacity_edited = false;
        self.share_capacity_edited = false;
        self.encrypt_capacity_edited = false;
        let set = |input: Option<crate::provision::CapacityInput>,
                   mode: &mut CapacityInputMode,
                   mib: &mut String,
                   sectors: &mut String,
                   source: &mut crate::provision::CapacitySource| {
            if let Some(input) = input {
                *mode = input.mode();
                *sectors = input.sectors().to_string();
                *source = input.source();
                *mib = Self::format_sector_unit_3(input.sectors(), 2_048);
            }
        };
        set(
            prefill.boot,
            &mut self.boot_input_mode,
            &mut self.boot_mib,
            &mut self.boot_sectors,
            &mut self.boot_capacity_source,
        );
        set(
            prefill.share,
            &mut self.share_input_mode,
            &mut self.share_mib,
            &mut self.share_sectors,
            &mut self.share_capacity_source,
        );
        set(
            prefill.encrypt,
            &mut self.encrypt_input_mode,
            &mut self.encrypt_mib,
            &mut self.encrypt_sectors,
            &mut self.encrypt_capacity_source,
        );
        self.boot_start_lba = prefill
            .boot_start_lba
            .map_or_else(String::new, |value| value.to_string());
        self.share_start_lba = prefill
            .share_start_lba
            .map_or_else(String::new, |value| value.to_string());
        self.encrypt_start_lba = prefill
            .encrypt_start_lba
            .map_or_else(String::new, |value| value.to_string());
    }

    fn toggle_capacity_input(&mut self, slot: usize) -> Result<(), String> {
        use crate::provision::{CapacityInputMode, QuickCapacityUnit};
        let (mode, unit, quick, exact, edited) = match slot {
            0 | 21 => (
                &mut self.boot_input_mode,
                &mut self.boot_quick_unit,
                &mut self.boot_mib,
                &mut self.boot_sectors,
                &mut self.boot_capacity_edited,
            ),
            1 | 22 => (
                &mut self.share_input_mode,
                &mut self.share_quick_unit,
                &mut self.share_mib,
                &mut self.share_sectors,
                &mut self.share_capacity_edited,
            ),
            2 | 23 => (
                &mut self.encrypt_input_mode,
                &mut self.encrypt_quick_unit,
                &mut self.encrypt_mib,
                &mut self.encrypt_sectors,
                &mut self.encrypt_capacity_edited,
            ),
            _ => return Err("不是容量输入方式字段".into()),
        };
        match (*mode, *unit) {
            (CapacityInputMode::Exact, _) => {
                let sectors = exact
                    .parse::<u64>()
                    .map_err(|_| "请先输入有效的 sector 数".to_string())?;
                *quick = Self::format_sector_unit_3(sectors, 2_048);
                *mode = CapacityInputMode::Quick;
                *unit = QuickCapacityUnit::MiB;
            }
            (CapacityInputMode::Quick, QuickCapacityUnit::MiB) => {
                let sectors =
                    Self::resolve_quick_sectors(quick, exact, *unit, *edited, "当前容量")?;
                *exact = sectors.to_string();
                *quick = Self::format_sector_unit_3(sectors, 2_097_152);
                *unit = QuickCapacityUnit::GiB;
            }
            (CapacityInputMode::Quick, QuickCapacityUnit::GiB) => {
                let sectors =
                    Self::resolve_quick_sectors(quick, exact, *unit, *edited, "当前容量")?;
                *exact = sectors.to_string();
                *mode = CapacityInputMode::Exact;
            }
        }
        *edited = false;
        Ok(())
    }

    fn mark_quick_capacity_edit(&mut self, slot: Option<usize>) {
        use crate::provision::CapacityInputMode;
        match slot {
            Some(0) if self.boot_input_mode == CapacityInputMode::Quick => {
                self.boot_capacity_edited = true;
            }
            Some(1) if self.share_input_mode == CapacityInputMode::Quick => {
                self.share_capacity_edited = true;
            }
            Some(2) if self.encrypt_input_mode == CapacityInputMode::Quick => {
                self.encrypt_capacity_edited = true;
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProvisionState {
    pub stage: ProvisionStage,
    pub kind: ProvisionKind,
    pub menu_selected: usize,
    pub field_selected: usize,
    pub field_cursor: usize,
    pub form: ProvisionForm,
    pub prepared: Option<ProvisionPrepared>,
    pub confirmation: String,
    pub export_path: String,
    pub offline_form: OfflineConvertForm,
    pub offline_field_selected: usize,
    pub offline_result: Option<OfflineConvertView>,
    pub message: Option<String>,
    target_disk: Option<u32>,
    form_initialized_for: Option<(u32, u64, Option<String>, ProvisionKind)>,
}

impl Default for ProvisionState {
    fn default() -> Self {
        Self {
            stage: ProvisionStage::Menu,
            kind: ProvisionKind::Mode0,
            menu_selected: 0,
            field_selected: 0,
            field_cursor: 0,
            form: ProvisionForm::default(),
            prepared: None,
            confirmation: String::new(),
            export_path: String::new(),
            offline_form: OfflineConvertForm::default(),
            offline_field_selected: 0,
            offline_result: None,
            message: None,
            target_disk: None,
            form_initialized_for: None,
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
    BeginRestore,
    BeginBackupCreate,
    BeginBackupCreateDeep,
    BeginBackupDelete,
    ToggleBackupSelection,
    BeginBackupBatchDelete,
    BeginBackupPrune,
    VerifyBackup,
    OpenInspect,
    OpenAdvancedInspect,
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
    backup_batch_delete: Option<BackupBatchDeleteState>,
    backup_selection: std::collections::BTreeSet<std::path::PathBuf>,
    backup_prune: Option<BackupPruneState>,
    provision: ProvisionState,
    pinned_disk: Option<u32>,
    inspect: Option<InspectState>,
    inspect_data: Option<crate::application::inspect::InspectWorkspace>,
    advanced_inspect: Option<AdvancedInspectState>,
    inspect_pending: bool,
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
            backup_prune: None,
            provision: ProvisionState::default(),
            pinned_disk: None,
            inspect: None,
            inspect_data: None,
            advanced_inspect: None,
            inspect_pending: false,
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
            "disk{} {} {} {}:{} {} {} {} {} {}",
            row.disk,
            row.device_id.as_deref().unwrap_or_default(),
            row.onlyid.as_deref().unwrap_or_default(),
            row.vid,
            row.pid,
            row.user.as_deref().unwrap_or_default(),
            row.dept.as_deref().unwrap_or_default(),
            row.proto,
            row.provision_kind.short_name(),
            row.provision_kind.full_name(),
        );
        text.to_ascii_lowercase().contains(query)
    }

    fn backup_matches_query(row: &crate::application::BackupWorkspaceItem, query: &str) -> bool {
        let text = format!(
            "{} {} {} {} {} {} {}",
            row.file_name,
            row.display_time,
            row.onlyid.as_deref().unwrap_or_default(),
            row.user.as_deref().unwrap_or_default(),
            row.dept.as_deref().unwrap_or_default(),
            row.provision_kind.short_name(),
            row.provision_kind.full_name(),
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

    pub fn advanced_inspect(&self) -> Option<&AdvancedInspectState> {
        self.advanced_inspect.as_ref()
    }

    pub fn advanced_inspect_mut(&mut self) -> Option<&mut AdvancedInspectState> {
        self.advanced_inspect.as_mut()
    }

    pub fn begin_advanced_inspect(&mut self, source: AdvancedInspectSource) -> bool {
        if self.critical_operation {
            self.set_notice("关键操作仍在执行，完成前不能启动高级检查。");
            return false;
        }
        let mut form = AdvancedInspectForm::default();
        if let AdvancedInspectSource::Disk(disk) = &source {
            if let Some(row) = self.devices.iter().find(|row| row.disk == *disk) {
                form.device_id = row.device_id.clone().unwrap_or_default();
            }
        }
        self.advanced_inspect = Some(AdvancedInspectState {
            source,
            stage: AdvancedInspectStage::Form,
            form,
            result: None,
            selected: 0,
            scroll: 0,
            message: None,
        });
        self.input_mode = InputMode::Normal;
        true
    }

    pub fn advanced_inspect_shift_mode(&mut self, reverse: bool) {
        if let Some(state) = self.advanced_inspect.as_mut() {
            if state.stage == AdvancedInspectStage::Form {
                state.form.mode = if reverse {
                    state.form.mode.previous()
                } else {
                    state.form.mode.next()
                };
                state.message = None;
            }
        }
    }

    pub fn advanced_inspect_move_field(&mut self, delta: isize) {
        let Some(state) = self.advanced_inspect.as_mut() else {
            return;
        };
        if state.stage != AdvancedInspectStage::Form {
            return;
        }
        const COUNT: usize = 4;
        state.form.field_selected = if delta < 0 {
            state
                .form
                .field_selected
                .saturating_sub(delta.unsigned_abs())
        } else {
            (state.form.field_selected + delta as usize).min(COUNT - 1)
        };
    }

    fn advanced_selected_field_mut(&mut self) -> Option<&mut String> {
        let state = self.advanced_inspect.as_mut()?;
        if state.stage != AdvancedInspectStage::Form {
            return None;
        }
        match state.form.field_selected {
            0 => Some(&mut state.form.lba_spec),
            1 => Some(&mut state.form.count),
            2 => Some(&mut state.form.device_id),
            3 => Some(&mut state.form.export_dir),
            _ => None,
        }
    }

    pub fn advanced_inspect_push_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        if let Some(field) = self.advanced_selected_field_mut() {
            if field.chars().count() < 512 {
                field.push(ch);
            }
        }
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.message = None;
        }
    }

    pub fn advanced_inspect_backspace(&mut self) {
        if let Some(field) = self.advanced_selected_field_mut() {
            field.pop();
        }
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.message = None;
        }
    }

    pub fn advanced_inspect_request(
        &mut self,
    ) -> Result<
        (
            AdvancedInspectSource,
            crate::application::inspect::AdvancedInspectRequest,
        ),
        String,
    > {
        let state = self
            .advanced_inspect
            .as_mut()
            .ok_or_else(|| "高级检查未打开".to_string())?;
        let lbas = crate::application::inspect::parse_advanced_lbas(
            &state.form.lba_spec,
            &state.form.count,
        )?;
        let request = crate::application::inspect::AdvancedInspectRequest {
            mode: state.form.mode,
            lbas,
            export_dir: (!state.form.export_dir.trim().is_empty())
                .then(|| std::path::PathBuf::from(state.form.export_dir.trim())),
            device_id_override: (!state.form.device_id.trim().is_empty())
                .then(|| state.form.device_id.trim().to_string()),
        };
        Ok((state.source.clone(), request))
    }

    pub fn advanced_inspect_start(&mut self) {
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.stage = AdvancedInspectStage::Running;
            state.result = None;
            state.selected = 0;
            state.scroll = 0;
            state.message = Some("正在后台读取并解析指定扇区…".into());
        }
    }

    pub fn advanced_inspect_finish(
        &mut self,
        result: Result<crate::application::inspect::AdvancedInspectWorkspace, String>,
    ) {
        let Some(state) = self.advanced_inspect.as_mut() else {
            return;
        };
        match result {
            Ok(workspace) => {
                state.stage = AdvancedInspectStage::Result;
                state.result = Some(workspace);
                state.selected = 0;
                state.scroll = 0;
                state.message = None;
            }
            Err(message) => {
                state.stage = AdvancedInspectStage::Form;
                state.result = None;
                state.message = Some(message);
            }
        }
    }

    pub fn advanced_inspect_move_result(&mut self, delta: isize) {
        let Some(state) = self.advanced_inspect.as_mut() else {
            return;
        };
        if state.stage != AdvancedInspectStage::Result {
            return;
        }
        let count = state
            .result
            .as_ref()
            .map(|value| value.items.len())
            .unwrap_or(0);
        if count == 0 {
            state.selected = 0;
            return;
        }
        state.selected = if delta < 0 {
            state.selected.saturating_sub(delta.unsigned_abs())
        } else {
            (state.selected + delta as usize).min(count - 1)
        };
        state.scroll = 0;
    }

    pub fn advanced_inspect_scroll(&mut self, delta: isize) {
        if let Some(state) = self.advanced_inspect.as_mut() {
            if state.stage == AdvancedInspectStage::Result {
                state.scroll = if delta < 0 {
                    state.scroll.saturating_sub(delta.unsigned_abs())
                } else {
                    state.scroll.saturating_add(delta as usize)
                };
            }
        }
    }

    pub fn advanced_inspect_back_to_form(&mut self) {
        if let Some(state) = self.advanced_inspect.as_mut() {
            if state.stage == AdvancedInspectStage::Result {
                state.stage = AdvancedInspectStage::Form;
                state.result = None;
                state.selected = 0;
                state.scroll = 0;
                state.message = None;
            }
        }
    }

    pub fn close_advanced_inspect(&mut self) {
        if self
            .advanced_inspect
            .as_ref()
            .is_some_and(|state| state.stage != AdvancedInspectStage::Running)
        {
            self.advanced_inspect = None;
        }
    }
    pub fn inspect_data(&self) -> Option<&crate::application::inspect::InspectWorkspace> {
        self.inspect_data.as_ref()
    }

    pub const fn inspect_pending(&self) -> bool {
        self.inspect_pending
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

    pub fn set_inspect_pending(&mut self, pending: bool) {
        self.inspect_pending = pending;
        if pending {
            self.set_notice("正在后台读取 LBA0-12…");
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
        self.clear_notice();
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
            self.set_notice("关键操作仍在执行，完成前不能启动其他任务。".to_string());
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

    pub fn backup_batch_delete(&self) -> Option<&BackupBatchDeleteState> {
        self.backup_batch_delete.as_ref()
    }

    pub fn backup_selection_count(&self) -> usize {
        self.backup_selection.len()
    }

    pub fn backup_is_selected(&self, path: &std::path::Path) -> bool {
        self.backup_selection.contains(path)
    }

    pub fn toggle_selected_backup(&mut self) {
        let Some((path, expected_sha256)) = self.selected_backup_delete_target() else {
            self.set_notice("当前备份缺少固定 SHA-256，不能加入批量删除选择。");
            return;
        };
        debug_assert!(!expected_sha256.is_empty());
        if !self.backup_selection.remove(&path) {
            self.backup_selection.insert(path);
        }
        self.set_notice(format!(
            "批量删除已勾选 {} 份备份；空格继续选择，X 生成删除计划。",
            self.backup_selection.len()
        ));
    }

    pub fn selected_backup_batch_targets(&self) -> Vec<(std::path::PathBuf, String)> {
        self.backups
            .iter()
            .filter(|row| self.backup_selection.contains(&row.path))
            .filter_map(|row| {
                row.content_sha256
                    .as_ref()
                    .map(|hash| (row.path.clone(), hash.clone()))
            })
            .collect()
    }

    pub fn begin_backup_batch_delete(&mut self) -> Option<Vec<(std::path::PathBuf, String)>> {
        if self.critical_operation || self.backup_batch_delete.is_some() {
            self.set_notice("已有关键操作或批量删除向导正在执行。");
            return None;
        }
        let targets = self.selected_backup_batch_targets();
        if targets.is_empty() {
            self.set_notice("先在备份页按空格勾选至少一份备份。");
            return None;
        }
        self.backup_batch_delete = Some(BackupBatchDeleteState {
            stage: BackupBatchDeleteStage::Planning,
            prepared: None,
            confirmation: String::new(),
            message: Some("正在新鲜扫描并逐项复核 SHA-256，生成固定删除计划…".into()),
        });
        Some(targets)
    }

    pub fn backup_batch_delete_finish_plan(
        &mut self,
        result: Result<crate::application::backup::DeletePlan, String>,
    ) {
        let Some(batch) = self.backup_batch_delete.as_mut() else {
            return;
        };
        match result {
            Ok(plan) if plan.targets.is_empty() => {
                batch.stage = BackupBatchDeleteStage::Result;
                batch.message = Some("批量删除计划为空，没有可删除目标。".into());
            }
            Ok(plan) => {
                batch.prepared = Some(plan);
                batch.stage = BackupBatchDeleteStage::Review;
                batch.message = None;
            }
            Err(message) => {
                batch.stage = BackupBatchDeleteStage::Result;
                batch.message = Some(message);
            }
        }
    }

    pub fn backup_batch_delete_begin_confirm(&mut self) {
        if let Some(batch) = self.backup_batch_delete.as_mut() {
            if batch.stage == BackupBatchDeleteStage::Review {
                batch.stage = BackupBatchDeleteStage::Confirm;
                batch.confirmation.clear();
                batch.message = None;
            }
        }
    }

    pub fn backup_batch_delete_push_confirmation(&mut self, ch: char) {
        if let Some(batch) = self.backup_batch_delete.as_mut() {
            if batch.stage == BackupBatchDeleteStage::Confirm && batch.confirmation.len() < 16 {
                batch.confirmation.push(ch);
                batch.message = None;
            }
        }
    }

    pub fn backup_batch_delete_backspace(&mut self) {
        if let Some(batch) = self.backup_batch_delete.as_mut() {
            if batch.stage == BackupBatchDeleteStage::Confirm {
                batch.confirmation.pop();
                batch.message = None;
            }
        }
    }

    pub fn backup_batch_delete_take_for_execute(
        &mut self,
    ) -> Option<crate::application::backup::DeletePlan> {
        let batch = self.backup_batch_delete.as_mut()?;
        if batch.stage != BackupBatchDeleteStage::Confirm {
            return None;
        }
        if batch.confirmation != "YES" {
            batch.message = Some("必须精确输入 YES 才会批量删除备份。".into());
            return None;
        }
        let plan = batch.prepared.take()?;
        batch.stage = BackupBatchDeleteStage::Running;
        batch.message = Some("正在按固定计划逐条复核并删除…".into());
        self.critical_operation = true;
        Some(plan)
    }

    pub fn backup_batch_delete_finish_execute(&mut self, result: Result<usize, String>) {
        self.critical_operation = false;
        let success = result.is_ok();
        if let Some(batch) = self.backup_batch_delete.as_mut() {
            batch.stage = BackupBatchDeleteStage::Result;
            batch.message = Some(match result {
                Ok(count) => format!("批量删除完成：已安全删除 {count} 份备份。"),
                Err(message) => message,
            });
        }
        if success {
            self.backup_selection.clear();
        }
    }

    pub fn close_backup_batch_delete(&mut self) {
        if !self.critical_operation {
            self.backup_batch_delete = None;
        }
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
            self.set_notice("关键操作仍在执行，完成前不能启动其他任务。".to_string());
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
        let target_disk = self.provision.target_disk;
        self.provision = ProvisionState::default();
        self.provision.menu_selected = selected;
        self.provision.kind = ProvisionKind::ALL[selected];
        self.provision.target_disk = target_disk;
        self.pinned_disk = target_disk;
        if self.workspace == Workspace::Provision {
            if target_disk.is_some() {
                self.provision.stage = ProvisionStage::Menu;
                self.set_item_count(ProvisionKind::ALL.len());
                self.selected = selected;
            } else {
                self.provision.stage = ProvisionStage::SelectDisk;
                self.set_item_count(self.provision_selectable_devices().count());
                self.selected = 0;
            }
        }
    }

    pub fn provision_backup_summary(&self) -> String {
        let Some(device) = self.selected_device() else {
            return "未固定目标 USB，无法判断历史保存记录".into();
        };
        if self.backup_scan_pending {
            return "正在扫描历史保存记录…".into();
        }
        let matches = device.onlyid.as_ref().map_or_else(Vec::new, |onlyid| {
            self.backups
                .iter()
                .filter(|backup| backup.onlyid.as_ref() == Some(onlyid))
                .collect::<Vec<_>>()
        });
        let count = matches.len().max(device.n_baks);
        if count == 0 {
            "此盘此前没有保存记录".into()
        } else {
            let latest = matches
                .iter()
                .map(|backup| backup.display_time.as_str())
                .max()
                .unwrap_or("时间未知");
            format!("此盘此前已保存 {count} 份 · 最近 {latest}")
        }
    }

    pub fn provision_skip_backup(&mut self) {
        if self.provision.stage != ProvisionStage::BackupPrompt {
            return;
        }
        self.provision.stage = ProvisionStage::Menu;
        self.provision.message = Some("已选择不保存当前盘，继续选择制盘模式。".into());
        self.set_item_count(ProvisionKind::ALL.len());
        self.selected = self
            .provision
            .menu_selected
            .min(ProvisionKind::ALL.len().saturating_sub(1));
    }

    pub fn provision_begin_backup_save(&mut self) {
        if self.provision.stage == ProvisionStage::BackupPrompt {
            self.provision.stage = ProvisionStage::BackupSaving;
            self.provision.message = Some("正在保存当前盘…".into());
            self.critical_operation = true;
        }
    }

    pub fn provision_finish_backup_save(&mut self, result: Result<(), String>) {
        self.critical_operation = false;
        match result {
            Ok(()) => {
                self.provision.stage = ProvisionStage::Menu;
                self.provision.message = Some("当前盘保存完成；继续选择制盘模式。".into());
                self.set_item_count(ProvisionKind::ALL.len());
                self.selected = self
                    .provision
                    .menu_selected
                    .min(ProvisionKind::ALL.len().saturating_sub(1));
            }
            Err(message) => {
                self.provision.stage = ProvisionStage::BackupPrompt;
                self.provision.message = Some(message);
                self.set_item_count(2);
                self.selected = 0;
            }
        }
    }

    pub fn provision_begin_selected(&mut self) -> ProvisionKind {
        let index = self.selected.min(ProvisionKind::ALL.len() - 1);
        let kind = ProvisionKind::ALL[index];
        if kind != ProvisionKind::Offline && self.selected_device().is_none() {
            self.provision.stage = ProvisionStage::SelectDisk;
            self.provision.message = Some("请先在制盘页明确选择 USB 目标盘。".into());
            self.set_item_count(self.provision_selectable_devices().count());
            return kind;
        }
        self.provision.menu_selected = index;
        self.provision.kind = kind;
        self.provision.field_selected = 0;
        self.provision.field_cursor = 0;
        self.provision.confirmation.clear();
        self.provision.message = None;
        self.provision.prepared = None;
        self.provision.offline_result = None;
        if kind == ProvisionKind::Offline {
            self.provision.stage = ProvisionStage::OfflineForm;
        } else {
            let current_target = self
                .selected_device()
                .map(|row| (row.disk, row.size, row.device_id.clone(), kind));
            if self.provision.form_initialized_for == current_target {
                self.provision.stage = ProvisionStage::Form;
                self.provision_sync_cursor_to_end();
                return kind;
            }
            self.provision.form = ProvisionForm::default();
            let defaults = self.selected_device().map(|row| {
                (
                    row.onlyid.clone(),
                    row.user.clone().unwrap_or_default(),
                    row.dept.clone().unwrap_or_default(),
                    row.label.clone(),
                    row.force_change_password,
                    row.cancel_password_complexity_check,
                    row.max_share_password_errors,
                    row.max_encrypt_password_errors,
                )
            });
            let scanned_onlyid = defaults
                .as_ref()
                .and_then(|(onlyid, _, _, _, _, _, _, _)| onlyid.clone())
                .filter(|value| !value.trim().is_empty());
            self.provision.form.label_id = scanned_onlyid.unwrap_or_else(|| {
                crate::provision::OnlyId::random_candidate()
                    .map(|value| value.text().to_string())
                    .unwrap_or_else(|_| "1".into())
            });
            if let Some((
                _,
                user,
                dept,
                label,
                force_change_password,
                cancel_password_complexity_check,
                max_share_password_errors,
                max_encrypt_password_errors,
            )) = defaults
            {
                self.provision.form.user = user;
                self.provision.form.dept = dept;
                if let Some(label) = label.filter(|value| !value.trim().is_empty()) {
                    self.provision.form.label = label;
                }
                if let Some(force_change_password) = force_change_password {
                    self.provision.form.force_change_password = force_change_password;
                }
                if let Some(cancel) = cancel_password_complexity_check {
                    self.provision.form.cancel_password_complexity_check = cancel;
                }
                if let Some(value) = max_share_password_errors {
                    self.provision.form.max_share_password_errors = value.to_string();
                }
                if let Some(value) = max_encrypt_password_errors {
                    self.provision.form.max_encrypt_password_errors = value.to_string();
                }
            }
            let target_mode = match kind {
                ProvisionKind::Mode0 => {
                    crate::provision::OfficialPartitionMode::DefaultThreePartition
                }
                ProvisionKind::Mode1 => crate::provision::OfficialPartitionMode::BootShareCombined,
                ProvisionKind::Mode2 => crate::provision::OfficialPartitionMode::WholeDiskEncrypted,
                ProvisionKind::Mode3 => {
                    crate::provision::OfficialPartitionMode::IntranetExtranetDualPartition
                }
                ProvisionKind::Offline => unreachable!(),
            };
            let prefill = self.selected_device().and_then(|row| {
                let source = row.existing_profile_for_prefill();
                let total = row.size / crate::common::SECTOR as u64;
                let lce = crate::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity(total, crate::common::SECTOR as u32)?;
                crate::provision::prefill_for_target_mode(source.as_ref(), target_mode, lce.start_lba, crate::common::SECTOR as u64).ok()
            });
            if let Some(prefill) = prefill {
                self.provision.form.apply_prefill(&prefill);
            }
            self.provision.form_initialized_for = current_target;
            self.provision.stage = ProvisionStage::Form;
            self.provision_sync_cursor_to_end();
        }
        kind
    }

    pub fn provision_field_count(&self) -> usize {
        (0..)
            .take_while(|&index| self.provision_field_slot(index).is_some())
            .count()
    }

    pub fn offline_fields(&self) -> [(&'static str, &str); 4] {
        [
            ("快照目录", self.provision.offline_form.source_dir.as_str()),
            ("device_id", self.provision.offline_form.device_id.as_str()),
            ("目标大小 GiB", self.provision.offline_form.size_gb.as_str()),
            ("输出目录", self.provision.offline_form.output_dir.as_str()),
        ]
    }

    pub fn offline_move_field(&mut self, delta: isize) {
        self.provision.offline_field_selected = if delta < 0 {
            self.provision
                .offline_field_selected
                .saturating_sub(delta.unsigned_abs())
        } else {
            (self.provision.offline_field_selected + delta as usize).min(3)
        };
    }

    fn offline_selected_field_mut(&mut self) -> &mut String {
        match self.provision.offline_field_selected {
            0 => &mut self.provision.offline_form.source_dir,
            1 => &mut self.provision.offline_form.device_id,
            2 => &mut self.provision.offline_form.size_gb,
            _ => &mut self.provision.offline_form.output_dir,
        }
    }

    pub fn offline_push_char(&mut self, ch: char) {
        if !ch.is_control() {
            let field = self.offline_selected_field_mut();
            if field.chars().count() < 512 {
                field.push(ch);
                self.provision.message = None;
            }
        }
    }

    pub fn offline_backspace(&mut self) {
        self.offline_selected_field_mut().pop();
        self.provision.message = None;
    }

    pub fn offline_request(
        &mut self,
    ) -> Result<crate::application::offline_convert::OfflineConvertRequest, String> {
        let source_dir = self.provision.offline_form.source_dir.trim();
        let device_id = self.provision.offline_form.device_id.trim();
        if source_dir.is_empty() || device_id.is_empty() {
            return Err("快照目录和 device_id 不能为空".into());
        }
        let size_gb = if self.provision.offline_form.size_gb.trim().is_empty() {
            None
        } else {
            Some(
                self.provision
                    .offline_form
                    .size_gb
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite() && *value > 0.0)
                    .ok_or_else(|| "目标大小必须为大于 0 的 GiB 数值".to_string())?,
            )
        };
        let output_dir = (!self.provision.offline_form.output_dir.trim().is_empty())
            .then(|| std::path::PathBuf::from(self.provision.offline_form.output_dir.trim()));
        Ok(crate::application::offline_convert::OfflineConvertRequest {
            source_dir: std::path::PathBuf::from(source_dir),
            device_id: device_id.to_string(),
            size_gb,
            output_dir,
        })
    }

    pub fn offline_start(&mut self) {
        self.provision.stage = ProvisionStage::OfflineRunning;
        self.provision.message = Some("正在后台读取 LBA 快照并执行离线转换…".into());
        self.provision.offline_result = None;
    }

    pub fn offline_finish(&mut self, result: Result<OfflineConvertView, String>) {
        self.provision.stage = ProvisionStage::OfflineResult;
        match result {
            Ok(view) => {
                self.provision.offline_result = Some(view);
                self.provision.message = None;
            }
            Err(message) => {
                self.provision.offline_result = None;
                self.provision.message = Some(message);
            }
        }
    }

    pub fn offline_back_to_form(&mut self) {
        self.provision.stage = ProvisionStage::OfflineForm;
        self.provision.message = None;
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
        self.provision_sync_cursor_to_end();
    }

    fn provision_field_slot(&self, display_index: usize) -> Option<usize> {
        let mode = self.provision.kind.mode()?;
        let mut slots = Vec::with_capacity(30);
        slots.extend([3, 4, 5, 6, 7]);
        if matches!(mode, 0 | 3) {
            slots.extend([0, 24]);
        }
        if matches!(mode, 0 | 1 | 3) {
            slots.extend([1, 25]);
        }
        if matches!(mode, 0..=2) {
            slots.extend([2, 26]);
        }
        for target in self.provision_format_template() {
            let (toggle, filesystem, label) = match target.role {
                crate::provision::PartitionRole::Boot => (11, Some(18), Some(14)),
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined => (12, Some(19), Some(15)),
                crate::provision::PartitionRole::Encrypt => (13, Some(20), Some(16)),
                crate::provision::PartitionRole::CompatibilityReserve => (17, None, None),
            };
            slots.push(toggle);
            if let Some(filesystem) = filesystem {
                slots.push(filesystem);
            }
            if let Some(label) = label {
                slots.push(label);
            }
        }
        slots.extend([9, 27, 28, 29]);
        slots.get(display_index).copied()
    }

    fn provision_format_template(&self) -> Vec<crate::provision::PartitionFormatTarget> {
        let Some(mode) = self.provision.kind.mode() else {
            return Vec::new();
        };
        let mode = match mode {
            0 => crate::provision::OfficialPartitionMode::DefaultThreePartition,
            1 => crate::provision::OfficialPartitionMode::BootShareCombined,
            2 => crate::provision::OfficialPartitionMode::WholeDiskEncrypted,
            _ => crate::provision::OfficialPartitionMode::IntranetExtranetDualPartition,
        };
        crate::provision::official_format_targets_with_filesystems(
            mode,
            crate::provision::OfficialPartitionSizes::new(32, 64, 128),
            512,
            crate::provision::OfficialPartitionFilesystems {
                boot: self.provision.form.boot_fs,
                share: self.provision.form.share_fs,
                encrypt: self.provision.form.encrypt_fs,
            },
        )
        .unwrap_or_default()
    }

    pub fn provision_visible_fields(&self) -> Vec<(String, &str, bool)> {
        let mut out = Vec::new();
        let mode = match self.provision.kind.mode() {
            Some(value) => value,
            None => return out,
        };
        out.extend([
            (
                "标签标识".into(),
                self.provision.form.label_id.as_str(),
                false,
            ),
            ("用户名".into(), self.provision.form.user.as_str(), false),
            ("部门".into(), self.provision.form.dept.as_str(), false),
            (
                "SAFE6 标签".into(),
                self.provision.form.label.as_str(),
                false,
            ),
            (
                "初始密码".into(),
                self.provision.form.password.as_str(),
                true,
            ),
        ]);
        if matches!(mode, 0 | 3) {
            let exact =
                self.provision.form.boot_input_mode == crate::provision::CapacityInputMode::Exact;
            out.push((
                (if exact {
                    "启动区容量 (sector)"
                } else {
                    match self.provision.form.boot_quick_unit {
                        crate::provision::QuickCapacityUnit::MiB => "启动区容量 (MiB)",
                        crate::provision::QuickCapacityUnit::GiB => "启动区容量 (GiB)",
                    }
                })
                .into(),
                if exact {
                    self.provision.form.boot_sectors.as_str()
                } else {
                    self.provision.form.boot_mib.as_str()
                },
                false,
            ));
            out.push((
                "启动区起点 LBA".into(),
                self.provision.form.boot_start_lba.as_str(),
                false,
            ));
        }
        if matches!(mode, 0 | 1 | 3) {
            let exact =
                self.provision.form.share_input_mode == crate::provision::CapacityInputMode::Exact;
            out.push((
                (if exact {
                    "交换区容量 (sector)"
                } else {
                    match self.provision.form.share_quick_unit {
                        crate::provision::QuickCapacityUnit::MiB => "交换区容量 (MiB)",
                        crate::provision::QuickCapacityUnit::GiB => "交换区容量 (GiB)",
                    }
                })
                .into(),
                if exact {
                    self.provision.form.share_sectors.as_str()
                } else {
                    self.provision.form.share_mib.as_str()
                },
                false,
            ));
            out.push((
                "交换区起点 LBA".into(),
                self.provision.form.share_start_lba.as_str(),
                false,
            ));
        }
        if matches!(mode, 0..=2) {
            let exact = self.provision.form.encrypt_input_mode
                == crate::provision::CapacityInputMode::Exact;
            out.push((
                (if exact {
                    "保密区容量 (sector)"
                } else {
                    match self.provision.form.encrypt_quick_unit {
                        crate::provision::QuickCapacityUnit::MiB => "保密区容量 (MiB)",
                        crate::provision::QuickCapacityUnit::GiB => "保密区容量 (GiB)",
                    }
                })
                .into(),
                if exact {
                    self.provision.form.encrypt_sectors.as_str()
                } else {
                    self.provision.form.encrypt_mib.as_str()
                },
                false,
            ));
            out.push((
                "保密区起点 LBA".into(),
                self.provision.form.encrypt_start_lba.as_str(),
                false,
            ));
        }
        for target in self.provision_format_template() {
            let role = target.role;
            if !target.format_capable {
                out.push((role.label().into(), "固定，不格式化", false));
                continue;
            }
            let (selected, label) = match role {
                crate::provision::PartitionRole::Boot => (
                    self.provision.form.format_boot,
                    self.provision.form.volume_label.as_str(),
                ),
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined => (
                    self.provision.form.format_share,
                    self.provision.form.share_label.as_str(),
                ),
                crate::provision::PartitionRole::Encrypt => (
                    self.provision.form.format_encrypt,
                    self.provision.form.encrypt_label.as_str(),
                ),
                crate::provision::PartitionRole::CompatibilityReserve => unreachable!(),
            };
            out.push((
                format!("{}格式化", role.label()),
                if selected { "☑ 是" } else { "☐ 否" },
                false,
            ));
            out.push((
                format!("{}文件系统", role.label()),
                target.filesystem.unwrap().windows_format_name(),
                false,
            ));
            out.push((format!("{}卷标", role.label()), label, false));
        }
        out.push((
            "初始化密码强制修改".into(),
            if self.provision.form.force_change_password {
                "☑ 是"
            } else {
                "☐ 否"
            },
            false,
        ));
        out.push((
            "取消密码复杂性验证".into(),
            if self.provision.form.cancel_password_complexity_check {
                "☑ 是"
            } else {
                "☐ 否"
            },
            false,
        ));
        out.push((
            "交换区密码最大错误次数".into(),
            self.provision.form.max_share_password_errors.as_str(),
            false,
        ));
        out.push((
            "保密区密码最大错误次数".into(),
            self.provision.form.max_encrypt_password_errors.as_str(),
            false,
        ));
        out
    }

    fn provision_selected_field_mut(&mut self) -> Option<&mut String> {
        match self.provision_field_slot(self.provision.field_selected)? {
            0 => Some(
                if self.provision.form.boot_input_mode == crate::provision::CapacityInputMode::Exact
                {
                    &mut self.provision.form.boot_sectors
                } else {
                    &mut self.provision.form.boot_mib
                },
            ),
            1 => Some(
                if self.provision.form.share_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    &mut self.provision.form.share_sectors
                } else {
                    &mut self.provision.form.share_mib
                },
            ),
            2 => Some(
                if self.provision.form.encrypt_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    &mut self.provision.form.encrypt_sectors
                } else {
                    &mut self.provision.form.encrypt_mib
                },
            ),
            3 => Some(&mut self.provision.form.label_id),
            4 => Some(&mut self.provision.form.user),
            5 => Some(&mut self.provision.form.dept),
            6 => Some(&mut self.provision.form.label),
            7 => Some(&mut self.provision.form.password),
            14 => Some(&mut self.provision.form.volume_label),
            15 => Some(&mut self.provision.form.share_label),
            16 => Some(&mut self.provision.form.encrypt_label),
            24 => Some(&mut self.provision.form.boot_start_lba),
            25 => Some(&mut self.provision.form.share_start_lba),
            26 => Some(&mut self.provision.form.encrypt_start_lba),
            28 => Some(&mut self.provision.form.max_share_password_errors),
            29 => Some(&mut self.provision.form.max_encrypt_password_errors),
            _ => None,
        }
    }

    fn provision_selected_field(&self) -> Option<&str> {
        match self.provision_field_slot(self.provision.field_selected)? {
            0 => Some(
                if self.provision.form.boot_input_mode == crate::provision::CapacityInputMode::Exact
                {
                    self.provision.form.boot_sectors.as_str()
                } else {
                    self.provision.form.boot_mib.as_str()
                },
            ),
            1 => Some(
                if self.provision.form.share_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    self.provision.form.share_sectors.as_str()
                } else {
                    self.provision.form.share_mib.as_str()
                },
            ),
            2 => Some(
                if self.provision.form.encrypt_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    self.provision.form.encrypt_sectors.as_str()
                } else {
                    self.provision.form.encrypt_mib.as_str()
                },
            ),
            3 => Some(self.provision.form.label_id.as_str()),
            4 => Some(self.provision.form.user.as_str()),
            5 => Some(self.provision.form.dept.as_str()),
            6 => Some(self.provision.form.label.as_str()),
            7 => Some(self.provision.form.password.as_str()),
            14 => Some(self.provision.form.volume_label.as_str()),
            15 => Some(self.provision.form.share_label.as_str()),
            16 => Some(self.provision.form.encrypt_label.as_str()),
            24 => Some(self.provision.form.boot_start_lba.as_str()),
            25 => Some(self.provision.form.share_start_lba.as_str()),
            26 => Some(self.provision.form.encrypt_start_lba.as_str()),
            28 => Some(self.provision.form.max_share_password_errors.as_str()),
            29 => Some(self.provision.form.max_encrypt_password_errors.as_str()),
            _ => None,
        }
    }

    fn provision_sync_cursor_to_end(&mut self) {
        self.provision.field_cursor = self
            .provision_selected_field()
            .map(|value| value.chars().count())
            .unwrap_or(0);
    }

    pub fn provision_selected_field_is_editable(&self) -> bool {
        self.provision_selected_field().is_some()
    }

    pub fn provision_field_cursor(&self) -> usize {
        let len = self
            .provision_selected_field()
            .map(|value| value.chars().count())
            .unwrap_or(0);
        self.provision.field_cursor.min(len)
    }

    pub fn provision_move_cursor(&mut self, delta: isize) {
        let Some(value) = self.provision_selected_field() else {
            return;
        };
        let len = value.chars().count();
        self.provision.field_cursor = if delta < 0 {
            self.provision
                .field_cursor
                .saturating_sub(delta.unsigned_abs())
        } else {
            (self.provision.field_cursor + delta as usize).min(len)
        };
    }

    pub fn provision_cursor_home(&mut self) {
        if self.provision_selected_field().is_some() {
            self.provision.field_cursor = 0;
        }
    }

    pub fn provision_cursor_end(&mut self) {
        self.provision_sync_cursor_to_end();
    }

    fn provision_target_mode(&self) -> Option<crate::provision::OfficialPartitionMode> {
        match self.provision.kind {
            ProvisionKind::Mode0 => {
                Some(crate::provision::OfficialPartitionMode::DefaultThreePartition)
            }
            ProvisionKind::Mode1 => {
                Some(crate::provision::OfficialPartitionMode::BootShareCombined)
            }
            ProvisionKind::Mode2 => {
                Some(crate::provision::OfficialPartitionMode::WholeDiskEncrypted)
            }
            ProvisionKind::Mode3 => {
                Some(crate::provision::OfficialPartitionMode::IntranetExtranetDualPartition)
            }
            ProvisionKind::Offline => None,
        }
    }

    fn provision_resolved_prefill(
        &self,
    ) -> Result<
        (
            crate::provision::ProvisionPrefill,
            Option<crate::provision::ExistingProvisionProfile>,
        ),
        String,
    > {
        use crate::provision::{
            apply_target_geometry_overrides, CapacityInput, CapacityInputMode, CapacitySource,
            QuickCapacityUnit, TargetGeometryOverrides,
        };

        let row = self
            .selected_device()
            .ok_or_else(|| "请先选择 USB 目标盘".to_string())?;
        let target_mode = self
            .provision_target_mode()
            .ok_or_else(|| "离线快照工具不使用物理制盘表单".to_string())?;
        let total_sectors = row.size / crate::common::SECTOR as u64;
        let lce = crate::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity(
            total_sectors,
            crate::common::SECTOR as u32,
        )
        .ok_or_else(|| "当前目标不符合已验证的 512B/255x63 USB LCE 几何".to_string())?;
        let source = row.existing_profile_for_prefill();
        let base = crate::provision::prefill_for_target_mode(
            source.as_ref(),
            target_mode,
            lce.start_lba,
            crate::common::SECTOR as u64,
        )?;
        let form = &self.provision.form;
        let capacity = |mode: CapacityInputMode,
                        unit: QuickCapacityUnit,
                        quick: &str,
                        exact: &str,
                        edited: bool,
                        original: Option<CapacityInput>,
                        label: &str|
         -> Result<CapacityInput, String> {
            let provisional = match mode {
                CapacityInputMode::Exact => CapacityInput::from_exact(
                    exact
                        .parse::<u64>()
                        .ok()
                        .filter(|value| *value > 0)
                        .ok_or_else(|| format!("{label}必须为正整数 sector"))?,
                    CapacitySource::UserEdited,
                )?,
                CapacityInputMode::Quick => CapacityInput::from_quick_sectors(
                    ProvisionForm::resolve_quick_sectors(quick, exact, unit, edited, label)?,
                    CapacitySource::UserEdited,
                )?,
            };
            let source = original
                .filter(|value| value.sectors() == provisional.sectors())
                .map(CapacityInput::source)
                .unwrap_or(CapacitySource::UserEdited);
            match mode {
                CapacityInputMode::Exact => {
                    CapacityInput::from_exact(provisional.sectors(), source)
                }
                CapacityInputMode::Quick => match unit {
                    QuickCapacityUnit::MiB => {
                        CapacityInput::from_quick_sectors(provisional.sectors(), source)
                    }
                    QuickCapacityUnit::GiB => {
                        CapacityInput::from_quick_sectors(provisional.sectors(), source)
                    }
                },
            }
        };
        let parse_start = |active: bool,
                           value: &str,
                           original: Option<u64>,
                           label: &str|
         -> Result<Option<u64>, String> {
            if !active {
                return Ok(None);
            }
            let parsed = value
                .parse::<u64>()
                .map_err(|_| format!("{label}起点必须为整数 LBA"))?;
            Ok((Some(parsed) != original).then_some(parsed))
        };

        let mode = self.provision.kind.mode().unwrap_or(0);
        let overrides = TargetGeometryOverrides {
            boot: matches!(mode, 0 | 3)
                .then(|| {
                    capacity(
                        form.boot_input_mode,
                        form.boot_quick_unit,
                        &form.boot_mib,
                        &form.boot_sectors,
                        form.boot_capacity_edited,
                        base.boot,
                        "启动区",
                    )
                })
                .transpose()?,
            share: matches!(mode, 0 | 1 | 3)
                .then(|| {
                    capacity(
                        form.share_input_mode,
                        form.share_quick_unit,
                        &form.share_mib,
                        &form.share_sectors,
                        form.share_capacity_edited,
                        base.share,
                        "交换区",
                    )
                })
                .transpose()?,
            encrypt: matches!(mode, 0..=2)
                .then(|| {
                    capacity(
                        form.encrypt_input_mode,
                        form.encrypt_quick_unit,
                        &form.encrypt_mib,
                        &form.encrypt_sectors,
                        form.encrypt_capacity_edited,
                        base.encrypt,
                        "保密区",
                    )
                })
                .transpose()?,
            boot_start_lba: parse_start(
                matches!(mode, 0 | 3),
                &form.boot_start_lba,
                base.boot_start_lba,
                "启动区",
            )?,
            share_start_lba: parse_start(
                matches!(mode, 0 | 1 | 3),
                &form.share_start_lba,
                base.share_start_lba,
                "交换区",
            )?,
            encrypt_start_lba: parse_start(
                matches!(mode, 0..=2),
                &form.encrypt_start_lba,
                base.encrypt_start_lba,
                "保密区",
            )?,
        };
        let resolved = apply_target_geometry_overrides(base, source.as_ref(), overrides)?;
        Ok((resolved, source))
    }

    pub fn provision_geometry_preview_lines(&self) -> Vec<String> {
        use crate::provision::PartitionRole;
        let (resolved, source) = match self.provision_resolved_prefill() {
            Ok(value) => value,
            Err(message) => return vec![format!("布局无效: {message}")],
        };
        let parts = match resolved.target_partitions(crate::common::SECTOR as u64) {
            Ok(parts) => parts,
            Err(message) => return vec![format!("布局无效: {message}")],
        };
        let mut lines = Vec::new();
        let mut cursor = crate::provision::OFFICIAL_PARTITION_START_SECTOR;
        for part in &parts {
            if part.start_lba > cursor {
                lines.push(format!("空隙  {} sector", part.start_lba - cursor));
            }
            let end = part.start_lba + part.sector_count - 1;
            let candidate = source
                .as_ref()
                .and_then(|profile| profile.partition(part.role))
                .is_some_and(|old| {
                    part.role != PartitionRole::CompatibilityReserve
                        && old.partition_type == part.partition_type
                        && old.start_lba == part.start_lba
                        && old.sector_count == part.sector_count
                        && old.physically_encrypted == part.physically_encrypted
                });
            let action = if candidate {
                "可保留（待校验）"
            } else {
                "重建"
            };
            lines.push(format!(
                "{}  {} sector (~{} MiB)  ·  LBA {}–{}  ·  {}",
                part.role.label(),
                part.sector_count,
                part.sector_count / 2048,
                part.start_lba,
                end,
                action
            ));
            cursor = part.start_lba + part.sector_count;
        }
        let unallocated =
            crate::provision::validate_target_geometry(&parts, resolved.usable_end_lba)
                .unwrap_or_default();
        lines.push(format!("未分配  {unallocated} sector"));
        lines
    }

    pub fn provision_field_section(&self, display_index: usize) -> Option<&'static str> {
        let slot = self.provision_field_slot(display_index)?;
        match slot {
            0..=2 | 24..=26 => Some("分区布局"),
            3..=7 => Some("身份信息"),
            11..=20 => Some("格式化（可选）"),
            9 | 27..=29 => Some("密码策略"),
            _ => None,
        }
    }

    pub fn provision_compact_field_rows(&self) -> Vec<(&'static str, Vec<usize>)> {
        let fields = self.provision_visible_fields();
        let mut rows = Vec::new();
        let mut index = 0usize;
        while index < fields.len() {
            let section = self.provision_field_section(index).unwrap_or("其他");
            let slot = self.provision_field_slot(index).unwrap_or(usize::MAX);
            let width = match section {
                "身份信息" => 2,
                "分区布局" if matches!(slot, 0..=2) => 2,
                "分区布局" => 1,
                "密码策略" => 2,
                "格式化（可选）" if slot == 17 => 1,
                "格式化（可选）" if matches!(slot, 11..=13) => 2,
                "格式化（可选）" => 1,
                _ => 1,
            };
            let mut end = index + 1;
            while end < fields.len()
                && end < index + width
                && self.provision_field_section(end) == Some(section)
            {
                end += 1;
            }
            rows.push((section, (index..end).collect()));
            index = end;
        }
        rows
    }

    fn provision_selected_partition_role(&self) -> Option<crate::provision::PartitionRole> {
        use crate::provision::PartitionRole;
        let mode = self.provision.kind.mode()?;
        let slot = self.provision_field_slot(self.provision.field_selected)?;
        match slot {
            0 | 24 | 11 | 14 | 18 => Some(PartitionRole::Boot),
            1 | 25 | 12 | 15 | 19 => Some(if mode == 1 {
                PartitionRole::BootShareCombined
            } else {
                PartitionRole::Share
            }),
            2 | 26 | 13 | 16 | 20 => Some(PartitionRole::Encrypt),
            17 => Some(PartitionRole::CompatibilityReserve),
            _ => None,
        }
    }

    fn format_sector_size(sectors: u64) -> String {
        let mib = sectors as f64 / 2048.0;
        if mib >= 1024.0 {
            format!("{:.2} GiB", mib / 1024.0)
        } else if mib >= 1.0 {
            format!("{mib:.1} MiB")
        } else {
            format!("{sectors} sector")
        }
    }

    pub fn provision_layout_editor_lines(&self) -> Vec<String> {
        use crate::provision::PartitionRole;

        let Some(row) = self.selected_device() else {
            return vec!["未选择目标盘".into()];
        };
        let total_sectors = row.size / crate::common::SECTOR as u64;
        let mut lines = vec![format!(
            "disk{}  整盘 {}  ·  {} sector",
            row.disk,
            Self::format_sector_size(total_sectors),
            total_sectors
        )];
        let (resolved, source) = match self.provision_resolved_prefill() {
            Ok(value) => value,
            Err(message) => {
                lines.push(format!("✗ 布局无效: {message}"));
                return lines;
            }
        };
        let mut parts = match resolved.target_partitions(crate::common::SECTOR as u64) {
            Ok(parts) => parts,
            Err(message) => {
                lines.push(format!("✗ 布局无效: {message}"));
                return lines;
            }
        };
        parts.sort_by_key(|part| part.start_lba);
        let usable_start = crate::provision::OFFICIAL_PARTITION_START_SECTOR;
        let usable_end_exclusive = resolved.usable_end_lba;
        let usable_sectors = usable_end_exclusive.saturating_sub(usable_start);
        let allocated = parts.iter().map(|part| part.sector_count).sum::<u64>();
        let unallocated = crate::provision::validate_target_geometry(&parts, usable_end_exclusive)
            .unwrap_or_default();
        lines.push(format!(
            "可分区 LBA {}–{}  ·  {}",
            usable_start,
            usable_end_exclusive.saturating_sub(1),
            Self::format_sector_size(usable_sectors)
        ));
        lines.push(format!(
            "已分配 {}  ·  未分配 {}",
            Self::format_sector_size(allocated),
            Self::format_sector_size(unallocated)
        ));

        lines.push(String::new());

        let mut cursor = usable_start;
        for part in &parts {
            if part.start_lba > cursor {
                let gap = part.start_lba - cursor;
                lines.push(format!(
                    "空闲  LBA {}–{}  ·  {}",
                    cursor,
                    part.start_lba - 1,
                    Self::format_sector_size(gap)
                ));
            }
            let end = part.start_lba + part.sector_count - 1;
            let candidate = source
                .as_ref()
                .and_then(|profile| profile.partition(part.role))
                .is_some_and(|old| {
                    part.role != PartitionRole::CompatibilityReserve
                        && old.partition_type == part.partition_type
                        && old.start_lba == part.start_lba
                        && old.sector_count == part.sector_count
                        && old.physically_encrypted == part.physically_encrypted
                });
            lines.push(format!(
                "{}  LBA {}–{}  ·  {}  ·  {}",
                part.role.label(),
                part.start_lba,
                end,
                Self::format_sector_size(part.sector_count),
                if candidate { "可保留" } else { "重建" }
            ));
            cursor = part.start_lba.saturating_add(part.sector_count);
        }
        if unallocated > 0 && cursor < usable_end_exclusive {
            lines.push(format!(
                "空闲  LBA {}–{}  ·  {}",
                cursor,
                usable_end_exclusive - 1,
                Self::format_sector_size(usable_end_exclusive - cursor)
            ));
        }

        lines.push(String::new());
        if let Some(role) = self.provision_selected_partition_role() {
            if let Some((index, current)) =
                parts.iter().enumerate().find(|(_, part)| part.role == role)
            {
                let base = self.provision_target_mode().and_then(|mode| {
                    crate::provision::prefill_for_target_mode(
                        source.as_ref(),
                        mode,
                        resolved.usable_end_lba,
                        crate::common::SECTOR as u64,
                    )
                    .ok()
                });
                let explicit_start = |candidate: PartitionRole| -> bool {
                    let Some(base) = base.as_ref() else {
                        return false;
                    };
                    let (text, original) = match candidate {
                        PartitionRole::Boot | PartitionRole::CompatibilityReserve => (
                            self.provision.form.boot_start_lba.as_str(),
                            base.boot_start_lba,
                        ),
                        PartitionRole::Share | PartitionRole::BootShareCombined => (
                            self.provision.form.share_start_lba.as_str(),
                            base.share_start_lba,
                        ),
                        PartitionRole::Encrypt => (
                            self.provision.form.encrypt_start_lba.as_str(),
                            base.encrypt_start_lba,
                        ),
                    };
                    text.parse::<u64>()
                        .ok()
                        .is_some_and(|start| Some(start) != original)
                };
                let anchored = |candidate: PartitionRole| {
                    source
                        .as_ref()
                        .and_then(|profile| profile.partition(candidate))
                        .is_some()
                        || explicit_start(candidate)
                };

                let mut boundary = usable_end_exclusive;
                let mut downstream_unanchored = 0u64;
                let mut limiter = None;
                for next in parts.iter().skip(index + 1) {
                    if anchored(next.role) {
                        boundary = next.start_lba;
                        limiter = Some((next.role, next.start_lba));
                        break;
                    }
                    downstream_unanchored = downstream_unanchored.saturating_add(next.sector_count);
                }
                let max_sectors = boundary
                    .saturating_sub(current.start_lba)
                    .saturating_sub(downstream_unanchored);
                let grow = max_sectors.saturating_sub(current.sector_count);
                lines.push(format!("当前: {}", current.role.label()));
                lines.push(format!(
                    "大小 {} ({} sector)",
                    Self::format_sector_size(current.sector_count),
                    current.sector_count
                ));
                lines.push(format!(
                    "最大可设 {} ({} sector)",
                    Self::format_sector_size(max_sectors),
                    max_sectors
                ));
                lines.push(format!("还能增加 {}", Self::format_sector_size(grow)));
                if let Some((next_role, next_start)) = limiter {
                    lines.push(format!(
                        "限制: 后续{}固定起点 LBA {}",
                        next_role.label(),
                        next_start
                    ));
                } else {
                    lines.push(format!(
                        "限制: 可分区末端 LBA {}；后续未锚定分区可自动后移",
                        usable_end_exclusive.saturating_sub(1)
                    ));
                }
            }
        } else {
            lines.push("选中分区容量/单位/起点，可查看最大可设范围".into());
        }
        lines.push("✓ 当前布局无重叠、未越界".into());
        lines
    }

    pub fn provision_layout_bar(&self, width: usize) -> Vec<ProvisionBarKind> {
        use crate::provision::PartitionRole;

        let width = width.clamp(8, 96);
        let Ok((resolved, _)) = self.provision_resolved_prefill() else {
            return vec![ProvisionBarKind::Free; width];
        };
        let Ok(mut parts) = resolved.target_partitions(crate::common::SECTOR as u64) else {
            return vec![ProvisionBarKind::Free; width];
        };
        parts.sort_by_key(|part| part.start_lba);
        let usable_start = crate::provision::OFFICIAL_PARTITION_START_SECTOR;
        let usable_sectors = resolved.usable_end_lba.saturating_sub(usable_start);
        if usable_sectors == 0 {
            return vec![ProvisionBarKind::Free; width];
        }

        let mut segments = Vec::<(ProvisionBarKind, u64)>::new();
        let mut cursor = usable_start;
        for part in &parts {
            if part.start_lba > cursor {
                segments.push((
                    ProvisionBarKind::Free,
                    part.start_lba.saturating_sub(cursor),
                ));
            }
            let kind = match part.role {
                PartitionRole::Boot => ProvisionBarKind::Boot,
                PartitionRole::Share | PartitionRole::BootShareCombined => ProvisionBarKind::Share,
                PartitionRole::Encrypt => ProvisionBarKind::Encrypt,
                PartitionRole::CompatibilityReserve => ProvisionBarKind::Compatibility,
            };
            segments.push((kind, part.sector_count));
            cursor = part.start_lba.saturating_add(part.sector_count);
        }
        if cursor < resolved.usable_end_lba {
            segments.push((
                ProvisionBarKind::Free,
                resolved.usable_end_lba.saturating_sub(cursor),
            ));
        }
        if segments.is_empty() {
            return vec![ProvisionBarKind::Free; width];
        }

        let baseline = usize::from(segments.len() <= width);
        let baseline_total = baseline * segments.len();
        let remaining = width.saturating_sub(baseline_total);
        let total_weight = segments
            .iter()
            .map(|(_, sectors)| *sectors as u128)
            .sum::<u128>()
            .max(1);
        let mut allocations = Vec::with_capacity(segments.len());
        let mut assigned = 0usize;
        let mut remainders = Vec::with_capacity(segments.len());
        for (index, (_, sectors)) in segments.iter().enumerate() {
            let scaled = *sectors as u128 * remaining as u128;
            let extra = (scaled / total_weight) as usize;
            allocations.push(baseline + extra);
            assigned += baseline + extra;
            remainders.push((scaled % total_weight, index));
        }
        remainders.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        for (_, index) in remainders.into_iter().take(width.saturating_sub(assigned)) {
            allocations[index] += 1;
        }

        let mut cells = Vec::with_capacity(width);
        for ((kind, _), count) in segments.into_iter().zip(allocations) {
            cells.extend(std::iter::repeat_n(kind, count));
        }
        cells.truncate(width);
        while cells.len() < width {
            cells.push(ProvisionBarKind::Free);
        }
        cells
    }

    pub fn provision_field_hint(&self, display_index: usize) -> Option<String> {
        let slot = self.provision_field_slot(display_index)?;
        match slot {
            0..=2 => Some("Space 切换 MiB / GiB / sector".into()),
            7 => Some("交换区和保密区的初始密码".into()),
            9 | 11..=13 | 18..=20 | 27 => Some("Space 切换".into()),
            24..=26 => Some("通常无需修改；固定分区边界时再调整".into()),
            28 | 29 => Some("范围 0–255".into()),
            _ => None,
        }
    }

    pub fn provision_toggle_selected_option(&mut self) -> bool {
        match self.provision_field_slot(self.provision.field_selected) {
            Some(slot @ 0..=2) => {
                match self.provision.form.toggle_capacity_input(slot) {
                    Ok(()) => {
                        self.provision.message = None;
                        self.provision_sync_cursor_to_end();
                    }
                    Err(message) => self.provision.message = Some(message),
                }
                true
            }
            Some(9) => {
                self.provision.form.force_change_password =
                    !self.provision.form.force_change_password;
                self.provision.message = None;
                true
            }
            Some(27) => {
                self.provision.form.cancel_password_complexity_check =
                    !self.provision.form.cancel_password_complexity_check;
                self.provision.message = None;
                true
            }
            Some(11) => {
                self.provision.form.format_boot = !self.provision.form.format_boot;
                true
            }
            Some(12) => {
                self.provision.form.format_share = !self.provision.form.format_share;
                true
            }
            Some(13) => {
                self.provision.form.format_encrypt = !self.provision.form.format_encrypt;
                true
            }
            Some(18) => {
                self.provision.form.boot_fs = toggle_supported_fs(self.provision.form.boot_fs);
                true
            }
            Some(19) => {
                self.provision.form.share_fs = toggle_supported_fs(self.provision.form.share_fs);
                true
            }
            Some(20) => {
                self.provision.form.encrypt_fs =
                    toggle_supported_fs(self.provision.form.encrypt_fs);
                true
            }
            _ => false,
        }
    }

    pub fn provision_toggle_force_change_password(&mut self) -> bool {
        if self.provision_field_slot(self.provision.field_selected) != Some(9) {
            return false;
        }
        self.provision.form.force_change_password = !self.provision.form.force_change_password;
        self.provision.message = None;
        true
    }

    pub fn provision_push_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        let cursor = self.provision_field_cursor();
        let slot = self.provision_field_slot(self.provision.field_selected);
        if let Some(field) = self.provision_selected_field_mut() {
            if field.chars().count() < 128 {
                let mut chars = field.chars().collect::<Vec<_>>();
                chars.insert(cursor.min(chars.len()), ch);
                *field = chars.into_iter().collect();
                self.provision.field_cursor = cursor + 1;
                self.provision.form.mark_quick_capacity_edit(slot);
                self.provision.message = None;
            }
        }
    }

    pub fn provision_backspace(&mut self) {
        let cursor = self.provision_field_cursor();
        let slot = self.provision_field_slot(self.provision.field_selected);
        if cursor == 0 {
            return;
        }
        if let Some(field) = self.provision_selected_field_mut() {
            let mut chars = field.chars().collect::<Vec<_>>();
            if cursor <= chars.len() {
                chars.remove(cursor - 1);
                *field = chars.into_iter().collect();
                self.provision.field_cursor = cursor - 1;
                self.provision.form.mark_quick_capacity_edit(slot);
                self.provision.message = None;
            }
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
        let parse_sectors = |value: &str, label: &str| -> Result<u64, String> {
            value
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| format!("{label} 必须为正整数扇区"))
        };
        let form = &self.provision.form;
        let exact = crate::provision::CapacityInputMode::Exact;
        let capacity_sectors = |active: bool,
                                mode: crate::provision::CapacityInputMode,
                                unit: crate::provision::QuickCapacityUnit,
                                quick: &str,
                                exact_value: &str,
                                edited: bool,
                                label: &str|
         -> Result<Option<u64>, String> {
            if !active {
                return Ok(None);
            }
            let sectors = if mode == exact {
                parse_sectors(exact_value, label)?
            } else {
                ProvisionForm::resolve_quick_sectors(quick, exact_value, unit, edited, label)?
            };
            Ok(Some(sectors))
        };
        let boot_sectors = capacity_sectors(
            matches!(mode, 0 | 3),
            form.boot_input_mode,
            form.boot_quick_unit,
            &form.boot_mib,
            &form.boot_sectors,
            form.boot_capacity_edited,
            "启动区",
        )?;
        let share_sectors = capacity_sectors(
            matches!(mode, 0 | 1 | 3),
            form.share_input_mode,
            form.share_quick_unit,
            &form.share_mib,
            &form.share_sectors,
            form.share_capacity_edited,
            "交换区",
        )?;
        let encrypt_sectors = capacity_sectors(
            matches!(mode, 0..=2),
            form.encrypt_input_mode,
            form.encrypt_quick_unit,
            &form.encrypt_mib,
            &form.encrypt_sectors,
            form.encrypt_capacity_edited,
            "保密区",
        )?;
        let boot_mib = None;
        let share_mib = None;
        let encrypt_mib = None;
        let (resolved, _) = self.provision_resolved_prefill()?;
        if self.provision.form.label_id.trim().is_empty()
            || self.provision.form.user.trim().is_empty()
            || self.provision.form.dept.trim().is_empty()
            || self.provision.form.label.trim().is_empty()
            || self.provision.form.password.is_empty()
        {
            return Err("标签标识、用户、部门、标签和密码均不能为空".into());
        }
        let max_share_password_errors = self
            .provision
            .form
            .max_share_password_errors
            .trim()
            .parse::<u8>()
            .map_err(|_| "交换区密码最大错误次数必须为 0..255".to_string())?;
        let max_encrypt_password_errors = self
            .provision
            .form
            .max_encrypt_password_errors
            .trim()
            .parse::<u8>()
            .map_err(|_| "保密区密码最大错误次数必须为 0..255".to_string())?;
        Ok(crate::application::provision::NewProvisionRequest {
            mode,
            boot_start_lba: matches!(mode, 0 | 3)
                .then_some(resolved.boot_start_lba)
                .flatten(),
            share_start_lba: matches!(mode, 0 | 1 | 3)
                .then_some(resolved.share_start_lba)
                .flatten(),
            encrypt_start_lba: matches!(mode, 0..=2)
                .then_some(resolved.encrypt_start_lba)
                .flatten(),
            boot_mib,
            boot_sectors,
            share_mib,
            share_sectors,
            encrypt_mib,
            encrypt_sectors,
            label_id: self.provision.form.label_id.trim().to_string(),
            user: self.provision.form.user.trim().to_string(),
            dept: self.provision.form.dept.trim().to_string(),
            label: self.provision.form.label.trim().to_string(),
            password: self.provision.form.password.clone(),
            volume_label: self.provision.form.volume_label.trim().to_string(),
            format: crate::application::provision::FormatOptions {
                boot: self.provision.form.format_boot,
                share: self.provision.form.format_share,
                encrypt: self.provision.form.format_encrypt,
                boot_label: self.provision.form.volume_label.trim().to_string(),
                share_label: self.provision.form.share_label.trim().to_string(),
                encrypt_label: self.provision.form.encrypt_label.trim().to_string(),
                boot_fs: self.provision.form.boot_fs,
                share_fs: self.provision.form.share_fs,
                encrypt_fs: self.provision.form.encrypt_fs,
            },
            force_change_password: Some(self.provision.form.force_change_password),
            cancel_password_complexity_check: Some(
                self.provision.form.cancel_password_complexity_check,
            ),
            max_share_password_errors: Some(max_share_password_errors),
            max_encrypt_password_errors: Some(max_encrypt_password_errors),
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
                self.provision.stage = ProvisionStage::Form;
                self.provision.message = Some(message);
            }
        }
    }

    pub fn provision_begin_export(&mut self) {
        let is_new = matches!(self.provision.prepared, Some(ProvisionPrepared::New(_)));
        if self.provision.stage != ProvisionStage::Review || !is_new {
            return;
        }
        let mode = self.provision.kind.mode().unwrap_or(0);
        self.provision.export_path = format!("./edp-mode{mode}.img");
        self.provision.stage = ProvisionStage::ExportPath;
        self.provision.message = None;
    }

    pub fn provision_export_push_char(&mut self, ch: char) {
        if self.provision.stage == ProvisionStage::ExportPath
            && !ch.is_control()
            && self.provision.export_path.chars().count() < 512
        {
            self.provision.export_path.push(ch);
            self.provision.message = None;
        }
    }

    pub fn provision_export_backspace(&mut self) {
        if self.provision.stage == ProvisionStage::ExportPath {
            self.provision.export_path.pop();
            self.provision.message = None;
        }
    }

    pub fn provision_take_export(
        &mut self,
    ) -> Option<(
        crate::application::provision::PreparedNewProvision,
        std::path::PathBuf,
    )> {
        if self.provision.stage != ProvisionStage::ExportPath {
            return None;
        }
        let path = self.provision.export_path.trim();
        if path.is_empty() {
            self.provision.message = Some("镜像导出路径不能为空".into());
            return None;
        }
        let prepared = match self.provision.prepared.as_ref()? {
            ProvisionPrepared::New(prepared) => prepared.as_ref().clone(),
        };
        let path = std::path::PathBuf::from(path);
        self.provision.stage = ProvisionStage::Exporting;
        self.provision.message = Some(format!("正在后台导出 {}…", path.display()));
        Some((prepared, path))
    }

    pub fn provision_finish_export(&mut self, result: Result<std::path::PathBuf, String>) {
        self.provision.stage = ProvisionStage::Review;
        self.provision.message = Some(match result {
            Ok(path) => format!("镜像导出完成：{}", path.display()),
            Err(message) => message,
        });
    }

    pub fn provision_cancel_export(&mut self) {
        if self.provision.stage == ProvisionStage::ExportPath {
            self.provision.stage = ProvisionStage::Review;
            self.provision.message = None;
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

    pub fn provision_finish_write(&mut self, result: Result<String, String>) {
        self.critical_operation = false;
        self.provision.stage = ProvisionStage::Result;
        self.provision.message = Some(match result {
            Ok(message) => message,
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
        if self
            .provision
            .target_disk
            .is_some_and(|disk| !devices.iter().any(|row| row.disk == disk))
        {
            self.provision.target_disk = None;
        }
        self.devices = devices;
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
        self.provision.target_disk = Some(disk);
        self.switch_workspace(Workspace::Provision);
        self.pinned_disk = Some(disk);
        self.provision.stage = ProvisionStage::BackupPrompt;
        self.provision.message = None;
        self.selected = 0;
        self.set_item_count(2);
        Ok(disk)
    }

    pub fn provision_begin_offline(&mut self) {
        self.provision.target_disk = None;
        self.pinned_disk = None;
        self.provision.kind = ProvisionKind::Offline;
        self.provision.stage = ProvisionStage::OfflineForm;
        self.provision.message = None;
        self.provision.offline_result = None;
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
            let preserve_offline = matches!(
                self.provision.stage,
                ProvisionStage::OfflineForm
                    | ProvisionStage::OfflineRunning
                    | ProvisionStage::OfflineResult
            );
            if let Some(disk) = self.provision.target_disk {
                self.pinned_disk = Some(disk);
            } else if preserve_offline {
                self.pinned_disk = None;
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
                | ProvisionStage::Result
                | ProvisionStage::OfflineForm
                | ProvisionStage::OfflineRunning
                | ProvisionStage::OfflineResult => 0,
            },
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
            if self.workspace == Workspace::Provision {
                match self.provision.stage {
                    ProvisionStage::SelectDisk => {
                        self.switch_workspace(Workspace::Devices);
                    }
                    ProvisionStage::BackupPrompt => {
                        self.switch_workspace(Workspace::Devices);
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
                    ProvisionStage::OfflineForm | ProvisionStage::OfflineResult => {
                        self.provision_reset();
                    }
                    ProvisionStage::OfflineRunning => {
                        self.set_notice("离线转换正在后台执行，请等待完成。");
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
                | NavCommand::BeginRestore
                | NavCommand::BeginBackupCreate
                | NavCommand::BeginBackupCreateDeep
                | NavCommand::BeginBackupDelete
                | NavCommand::ToggleBackupSelection
                | NavCommand::BeginBackupBatchDelete
                | NavCommand::BeginBackupPrune
                | NavCommand::VerifyBackup
                | NavCommand::OpenInspect
                | NavCommand::OpenAdvancedInspect
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
            NavCommand::Left => {
                let target = match self.workspace {
                    Workspace::Devices => Workspace::Backups,
                    Workspace::Backups => Workspace::Devices,
                    Workspace::Provision => Workspace::Backups,
                };
                self.switch_workspace(target);
            }
            NavCommand::Right => {
                let target = match self.workspace {
                    Workspace::Devices => Workspace::Backups,
                    Workspace::Backups => Workspace::Devices,
                    Workspace::Provision => Workspace::Devices,
                };
                self.switch_workspace(target);
            }
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
            | NavCommand::OpenAdvancedInspect
            | NavCommand::NextMatch
            | NavCommand::PreviousMatch
            | NavCommand::Escape
            | NavCommand::Quit => {}
        }
        StateEffect::None
    }
}
