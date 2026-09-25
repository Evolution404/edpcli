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
    Running,
    Browser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedInspectPanel {
    Tree,
    Overview,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorInspectMode {
    Raw,
    Decode,
    Mixed,
}

impl SectorInspectMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Raw => "Raw",
            Self::Decode => "Decode",
            Self::Mixed => "Mixed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedInspectJumpUnit {
    Lba,
    ByteOffset,
}

impl AdvancedInspectJumpUnit {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Lba => "LBA",
            Self::ByteOffset => "byte offset",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvancedInspectPrompt {
    Jump {
        unit: AdvancedInspectJumpUnit,
        input: String,
    },
    Search {
        input: String,
    },
}

fn parse_inspect_jump_number(input: &str) -> Result<u64, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Jump 输入不能为空".into());
    }
    if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        if hex.is_empty() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("十六进制 Jump 必须使用 0x 前缀并只包含 0-9/A-F".into());
        }
        return u64::from_str_radix(hex, 16).map_err(|_| "Jump 数值超出 u64 范围".into());
    }
    if !input.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Jump 必须是十进制整数或 0x 前缀十六进制整数".into());
    }
    input
        .parse::<u64>()
        .map_err(|_| "Jump 数值超出 u64 范围".into())
}

fn inspect_row_path(node_path: &[String]) -> Vec<String> {
    let mut rows = Vec::with_capacity(node_path.len());
    let mut current = String::new();
    for node_id in node_path {
        if current.is_empty() {
            current.push_str(node_id);
        } else {
            current.push('/');
            current.push_str(node_id);
        }
        rows.push(current.clone());
    }
    rows
}

#[derive(Debug, Clone)]
pub struct SectorInspectorState {
    pub lba: u64,
    pub mode: SectorInspectMode,
    pub cursor: usize,
    pub pending: bool,
    pub error: Option<String>,
    pub field_expanded: bool,
    pub pinned_field: Option<crate::application::inspect::InspectField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvancedInspectTreeAction {
    None,
    SetLazyOffset {
        extent_id: String,
        offset: u64,
        target_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancedInspectTreeRow {
    pub id: String,
    pub label: String,
    pub depth: usize,
    pub kind: crate::application::inspect_tree::InspectNodeKind,
    pub range: crate::application::inspect_tree::InspectNodeRange,
    pub decoder: Option<crate::application::inspect::InspectDecoderKind>,
    pub status: crate::edpb::SemanticStatus,
    pub expandable: bool,
    pub expanded: bool,
    pub action: AdvancedInspectTreeAction,
}

#[derive(Debug, Clone)]
pub struct AdvancedInspectState {
    pub source: AdvancedInspectSource,
    pub stage: AdvancedInspectStage,
    pub result: Option<crate::application::inspect::AdvancedInspectWorkspace>,
    pub tree_selected: usize,
    pub detail_scroll: usize,
    pub panel: AdvancedInspectPanel,
    pub expanded: std::collections::BTreeSet<String>,
    pub lazy_offsets: std::collections::BTreeMap<String, u64>,
    pub sector: Option<SectorInspectorState>,
    pub sector_cache_order: std::collections::VecDeque<u64>,
    pub yank_register: Option<String>,
    pub prompt: Option<AdvancedInspectPrompt>,
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
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionBarKind {
    Free,
    Plain,
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
        Self::Plain,
    ];

    pub const fn target(self) -> crate::provision::ProvisionTarget {
        match self {
            Self::Mode0 => crate::provision::ProvisionTarget::OFFICIAL[0],
            Self::Mode1 => crate::provision::ProvisionTarget::OFFICIAL[1],
            Self::Mode2 => crate::provision::ProvisionTarget::OFFICIAL[2],
            Self::Mode3 => crate::provision::ProvisionTarget::OFFICIAL[3],
            Self::Plain => crate::provision::ProvisionTarget::Plain,
        }
    }

    pub const fn mode(self) -> Option<u8> {
        match self {
            Self::Mode0 => Some(0),
            Self::Mode1 => Some(1),
            Self::Mode2 => Some(2),
            Self::Mode3 => Some(3),
            Self::Plain => None,
        }
    }

    pub const fn title(self) -> &'static str {
        self.target().full_name()
    }

    pub const fn description(self) -> &'static str {
        self.target().description()
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
}

pub type ProvisionPrepared = crate::application::provision::PreparedProvision;

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

#[derive(Debug, Clone)]
pub struct PlainPartitionForm {
    pub start_lba: String,
    pub input_mode: crate::provision::CapacityInputMode,
    pub quick_unit: crate::provision::QuickCapacityUnit,
    pub quick_capacity: String,
    pub sector_count: String,
    capacity_edited: bool,
    pub filesystem: crate::provision::OfficialFilesystemFormat,
    pub volume_label: String,
}

impl PlainPartitionForm {
    fn from_spec(spec: &crate::provision::PlainPartitionSpec) -> Self {
        Self {
            start_lba: spec.start_lba.to_string(),
            input_mode: crate::provision::CapacityInputMode::Quick,
            quick_unit: crate::provision::QuickCapacityUnit::GiB,
            quick_capacity: ProvisionForm::format_sector_unit_3(spec.sector_count, 2_097_152),
            sector_count: spec.sector_count.to_string(),
            capacity_edited: false,
            filesystem: spec.filesystem,
            volume_label: spec.volume_label.clone(),
        }
    }

    fn resolve_sector_count(&self, label: &str) -> Result<u64, String> {
        match self.input_mode {
            crate::provision::CapacityInputMode::Exact => self
                .sector_count
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| format!("{label} sector 必须是大于 0 的整数")),
            crate::provision::CapacityInputMode::Quick => ProvisionForm::resolve_quick_sectors(
                &self.quick_capacity,
                &self.sector_count,
                self.quick_unit,
                self.capacity_edited,
                label,
            ),
        }
    }

    fn set_sector_count(&mut self, sectors: u64) {
        self.sector_count = sectors.to_string();
        self.quick_capacity = match self.quick_unit {
            crate::provision::QuickCapacityUnit::MiB => {
                ProvisionForm::format_sector_unit_3(sectors, 2_048)
            }
            crate::provision::QuickCapacityUnit::GiB => {
                ProvisionForm::format_sector_unit_3(sectors, 2_097_152)
            }
        };
        self.capacity_edited = false;
    }

    fn cycle_capacity_unit(&mut self) -> Result<(), String> {
        use crate::provision::{CapacityInputMode, QuickCapacityUnit};
        let sectors = self.resolve_sector_count("普通分区容量")?;
        self.sector_count = sectors.to_string();
        match (self.input_mode, self.quick_unit) {
            (CapacityInputMode::Exact, _) => {
                self.input_mode = CapacityInputMode::Quick;
                self.quick_unit = QuickCapacityUnit::MiB;
                self.quick_capacity = ProvisionForm::format_sector_unit_3(sectors, 2_048);
            }
            (CapacityInputMode::Quick, QuickCapacityUnit::MiB) => {
                self.quick_unit = QuickCapacityUnit::GiB;
                self.quick_capacity = ProvisionForm::format_sector_unit_3(sectors, 2_097_152);
            }
            (CapacityInputMode::Quick, QuickCapacityUnit::GiB) => {
                self.input_mode = CapacityInputMode::Exact;
            }
        }
        self.capacity_edited = false;
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlainProvisionForm {
    pub partitions: Vec<PlainPartitionForm>,
}

impl PlainProvisionForm {
    fn default_for_disk(total_sectors: u64) -> Result<Self, String> {
        let plan = crate::provision::PlainProvisionPlan::default_for_disk(total_sectors)?;
        Ok(Self {
            partitions: plan
                .partitions
                .iter()
                .map(PlainPartitionForm::from_spec)
                .collect(),
        })
    }

    fn specs(&self) -> Result<Vec<crate::provision::PlainPartitionSpec>, String> {
        self.partitions
            .iter()
            .enumerate()
            .map(|(index, part)| {
                let number = index + 1;
                let start_lba = part
                    .start_lba
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| format!("P{number} 起点 LBA 必须是整数"))?;
                let sector_count = part.resolve_sector_count(&format!("P{number} 容量"))?;
                Ok(crate::provision::PlainPartitionSpec::new(
                    start_lba,
                    sector_count,
                    part.filesystem,
                    part.volume_label.trim(),
                ))
            })
            .collect()
    }

    fn plan(&self, total_sectors: u64) -> Result<crate::provision::PlainProvisionPlan, String> {
        crate::provision::PlainProvisionPlan::new(total_sectors, self.specs()?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProvisionInputPolicy {
    DecimalCapacity,
    UnsignedInteger,
    U8,
    OnlyId,
    Text,
}

impl ProvisionInputPolicy {
    fn accepts(self, candidate: &str) -> bool {
        match self {
            Self::DecimalCapacity => {
                candidate.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
                    && candidate.chars().filter(|ch| *ch == '.').count() <= 1
            }
            Self::UnsignedInteger => candidate.chars().all(|ch| ch.is_ascii_digit()),
            Self::U8 => {
                candidate.chars().all(|ch| ch.is_ascii_digit()) && candidate.parse::<u8>().is_ok()
            }
            Self::OnlyId => {
                candidate == "-"
                    || candidate.parse::<i32>().is_ok()
                    || candidate.parse::<u32>().is_ok()
            }
            Self::Text => candidate.chars().all(|ch| !ch.is_control()),
        }
    }

    fn rejection_message(self) -> &'static str {
        match self {
            Self::DecimalCapacity => "容量只允许输入数字和一个小数点",
            Self::UnsignedInteger => "当前字段仅允许输入整数",
            Self::U8 => "该字段仅允许 0–255",
            Self::OnlyId => "标签标识仅允许 u32 或 i32 整数",
            Self::Text => "当前字段包含不支持的字符",
        }
    }
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
    pub plain_form: PlainProvisionForm,
    pub prepared: Option<ProvisionPrepared>,
    pub confirmation: String,
    pub export_path: String,
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
            plain_form: PlainProvisionForm::default(),
            prepared: None,
            confirmation: String::new(),
            export_path: String::new(),
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
    inspect_data: Option<crate::application::inspect::AdvancedInspectWorkspace>,
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
            for (index, item) in workspace.items.iter().enumerate() {
                let mut text = format!(
                    "lba{} {}",
                    item.lba,
                    item.method.as_deref().unwrap_or("raw")
                );
                for field in &item.fields {
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
                for note in &item.notes {
                    text.push(' ');
                    text.push_str(note);
                }
                let ascii: String = item
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
                for byte in &item.raw {
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

    pub fn begin_advanced_inspect(&mut self, source: AdvancedInspectSource) -> bool {
        if self.critical_operation {
            self.set_notice("关键操作仍在执行，完成前不能启动全盘检查。");
            return false;
        }
        let mut expanded = std::collections::BTreeSet::new();
        expanded.insert("device".to_string());
        self.advanced_inspect = Some(AdvancedInspectState {
            source,
            stage: AdvancedInspectStage::Running,
            result: None,
            tree_selected: 0,
            detail_scroll: 0,
            panel: AdvancedInspectPanel::Tree,
            expanded,
            lazy_offsets: std::collections::BTreeMap::new(),
            sector: None,
            sector_cache_order: std::collections::VecDeque::new(),
            yank_register: None,
            prompt: None,
            message: Some("正在后台读取协议上下文并建立全盘结构树…".into()),
        });
        self.input_mode = InputMode::Normal;
        true
    }

    pub fn advanced_inspect_request(
        &self,
    ) -> Result<
        (
            AdvancedInspectSource,
            crate::application::inspect::AdvancedInspectRequest,
        ),
        String,
    > {
        let state = self
            .advanced_inspect
            .as_ref()
            .ok_or_else(|| "全盘检查未打开".to_string())?;
        let device_id_override = match &state.source {
            AdvancedInspectSource::Disk(disk) => self
                .devices
                .iter()
                .find(|row| row.disk == *disk)
                .and_then(|row| row.device_id.clone()),
            AdvancedInspectSource::Backup(_) => None,
        };
        Ok((
            state.source.clone(),
            crate::application::inspect::AdvancedInspectRequest {
                mode: crate::application::inspect::AdvancedInspectMode::Meta,
                lbas: (0..crate::common::METADATA_SECTOR_COUNT as u64).collect(),
                export_dir: None,
                device_id_override,
                fail_soft_decode: false,
            },
        ))
    }

    pub fn advanced_inspect_finish(
        &mut self,
        result: Result<crate::application::inspect::AdvancedInspectWorkspace, String>,
    ) {
        let Some(state) = self.advanced_inspect.as_mut() else {
            return;
        };
        state.stage = AdvancedInspectStage::Browser;
        state.tree_selected = 0;
        state.detail_scroll = 0;
        state.sector = None;
        state.sector_cache_order.clear();
        state.prompt = None;
        match result {
            Ok(workspace) => {
                state.result = Some(workspace);
                state.message = None;
            }
            Err(message) => {
                state.result = None;
                state.message = Some(message);
            }
        }
    }

    pub fn advanced_inspect_tree_rows(&self) -> Vec<AdvancedInspectTreeRow> {
        const SECTOR_PAGE: usize = 64;

        fn path_id(parent: Option<&str>, node_id: &str) -> String {
            match parent {
                Some(parent) => format!("{parent}/{node_id}"),
                None => node_id.to_string(),
            }
        }

        fn page_row(
            id: String,
            label: String,
            depth: usize,
            range: crate::application::inspect_tree::InspectNodeRange,
            decoder: Option<crate::application::inspect::InspectDecoderKind>,
            status: crate::edpb::SemanticStatus,
            extent_id: String,
            offset: u64,
            target_id: String,
        ) -> AdvancedInspectTreeRow {
            AdvancedInspectTreeRow {
                id,
                label,
                depth,
                kind: crate::application::inspect_tree::InspectNodeKind::Group,
                range,
                decoder,
                status,
                expandable: false,
                expanded: false,
                action: AdvancedInspectTreeAction::SetLazyOffset {
                    extent_id,
                    offset,
                    target_id,
                },
            }
        }

        fn push_rows(
            node: &crate::application::inspect_tree::InspectNode,
            depth: usize,
            parent_path: Option<&str>,
            expanded: &std::collections::BTreeSet<String>,
            lazy_offsets: &std::collections::BTreeMap<String, u64>,
            workspace: &crate::application::inspect::AdvancedInspectWorkspace,
            rows: &mut Vec<AdvancedInspectTreeRow>,
        ) {
            use crate::application::inspect_tree::{InspectChildren, InspectNodeKind};

            let row_id = path_id(parent_path, &node.id);
            let expandable = match &node.children {
                InspectChildren::None => false,
                InspectChildren::Materialized(children) => !children.is_empty(),
                InspectChildren::LazySectors { sector_count, .. } => *sector_count > 0,
            };
            let is_expanded = expandable && expanded.contains(&row_id);
            rows.push(AdvancedInspectTreeRow {
                id: row_id.clone(),
                label: node.label.clone(),
                depth,
                kind: node.kind,
                range: node.range,
                decoder: node.decoder,
                status: node.status,
                expandable,
                expanded: is_expanded,
                action: AdvancedInspectTreeAction::None,
            });
            if !is_expanded {
                return;
            }

            match &node.children {
                InspectChildren::None => {}
                InspectChildren::Materialized(children) => {
                    for child in children {
                        push_rows(
                            child,
                            depth + 1,
                            Some(&row_id),
                            expanded,
                            lazy_offsets,
                            workspace,
                            rows,
                        );
                    }
                }
                InspectChildren::LazySectors {
                    start_lba,
                    sector_count,
                } => {
                    let page = SECTOR_PAGE as u64;
                    let max_offset = sector_count.saturating_sub(1) / page * page;
                    let offset = lazy_offsets
                        .get(&row_id)
                        .copied()
                        .unwrap_or(0)
                        .min(max_offset);
                    if offset > 0 {
                        let previous_offset = offset.saturating_sub(page);
                        let previous_lba = start_lba.saturating_add(previous_offset);
                        rows.push(page_row(
                            format!("{row_id}/page.prev.{offset}"),
                            format!("← 上一页 · 从 LBA{previous_lba}"),
                            depth + 1,
                            crate::application::inspect_tree::InspectNodeRange::sectors(
                                previous_lba,
                                page.min(*sector_count - previous_offset),
                            ),
                            node.decoder,
                            node.status,
                            row_id.clone(),
                            previous_offset,
                            format!("{row_id}/sector.{previous_lba}"),
                        ));
                    }

                    let children = node.materialize_sector_page(offset, SECTOR_PAGE);
                    let materialized_count = children.len() as u64;
                    for child in children {
                        let child = if child.kind == InspectNodeKind::Sector {
                            workspace
                                .items
                                .iter()
                                .find(|item| item.lba == child.range.start_lba)
                                .map(|item| {
                                    crate::application::inspect_tree::sector_node_with_fields(
                                        child.range.start_lba,
                                        child.decoder,
                                        child.status,
                                        &item.fields,
                                    )
                                })
                                .unwrap_or(child)
                        } else {
                            child
                        };
                        push_rows(
                            &child,
                            depth + 1,
                            Some(&row_id),
                            expanded,
                            lazy_offsets,
                            workspace,
                            rows,
                        );
                    }

                    let next_offset = offset.saturating_add(materialized_count);
                    if next_offset < *sector_count {
                        let next_lba = start_lba.saturating_add(next_offset);
                        rows.push(page_row(
                            format!("{row_id}/page.next.{next_offset}"),
                            format!("下一页 → · 从 LBA{next_lba}"),
                            depth + 1,
                            crate::application::inspect_tree::InspectNodeRange::sectors(
                                next_lba,
                                page.min(*sector_count - next_offset),
                            ),
                            node.decoder,
                            node.status,
                            row_id.clone(),
                            next_offset,
                            format!("{row_id}/sector.{next_lba}"),
                        ));
                    }
                }
            }
        }

        let Some(state) = self.advanced_inspect.as_ref() else {
            return Vec::new();
        };
        let Some(workspace) = state.result.as_ref() else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        push_rows(
            &workspace.topology.root,
            0,
            None,
            &state.expanded,
            &state.lazy_offsets,
            workspace,
            &mut rows,
        );
        rows
    }

    pub fn advanced_inspect_prompt(&self) -> Option<&AdvancedInspectPrompt> {
        self.advanced_inspect.as_ref()?.prompt.as_ref()
    }

    pub fn advanced_inspect_begin_jump(&mut self) {
        if let Some(state) = self
            .advanced_inspect
            .as_mut()
            .filter(|state| state.stage == AdvancedInspectStage::Browser && state.sector.is_none())
        {
            state.prompt = Some(AdvancedInspectPrompt::Jump {
                unit: AdvancedInspectJumpUnit::Lba,
                input: String::new(),
            });
            state.message = None;
        }
    }

    pub fn advanced_inspect_begin_search(&mut self) {
        if let Some(state) = self
            .advanced_inspect
            .as_mut()
            .filter(|state| state.stage == AdvancedInspectStage::Browser && state.sector.is_none())
        {
            state.prompt = Some(AdvancedInspectPrompt::Search {
                input: String::new(),
            });
            state.message = None;
        }
    }

    pub fn advanced_inspect_cancel_prompt(&mut self) {
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.prompt = None;
        }
    }

    pub fn advanced_inspect_prompt_push(&mut self, ch: char) {
        let Some(prompt) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.prompt.as_mut())
        else {
            return;
        };
        match prompt {
            AdvancedInspectPrompt::Jump { input, .. } | AdvancedInspectPrompt::Search { input } => {
                input.push(ch);
            }
        }
    }

    pub fn advanced_inspect_prompt_backspace(&mut self) {
        let Some(prompt) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.prompt.as_mut())
        else {
            return;
        };
        match prompt {
            AdvancedInspectPrompt::Jump { input, .. } | AdvancedInspectPrompt::Search { input } => {
                input.pop();
            }
        }
    }

    pub fn advanced_inspect_toggle_jump_unit(&mut self) {
        let Some(AdvancedInspectPrompt::Jump { unit, .. }) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.prompt.as_mut())
        else {
            return;
        };
        *unit = match *unit {
            AdvancedInspectJumpUnit::Lba => AdvancedInspectJumpUnit::ByteOffset,
            AdvancedInspectJumpUnit::ByteOffset => AdvancedInspectJumpUnit::Lba,
        };
    }

    pub fn advanced_inspect_submit_prompt(
        &mut self,
    ) -> Result<Option<(AdvancedInspectSource, u64)>, String> {
        let prompt = self
            .advanced_inspect
            .as_ref()
            .and_then(|state| state.prompt.clone())
            .ok_or_else(|| "Inspect 输入面板未打开".to_string())?;

        let request = match prompt {
            AdvancedInspectPrompt::Jump { unit, input } => {
                let value = parse_inspect_jump_number(&input)?;
                match unit {
                    AdvancedInspectJumpUnit::Lba => {
                        self.advanced_inspect_jump_lba(value)?;
                        None
                    }
                    AdvancedInspectJumpUnit::ByteOffset => {
                        self.advanced_inspect_jump_byte_offset(value)?
                    }
                }
            }
            AdvancedInspectPrompt::Search { input } => {
                self.advanced_inspect_search(&input)?;
                None
            }
        };

        if let Some(state) = self.advanced_inspect.as_mut() {
            state.prompt = None;
        }
        Ok(request)
    }

    pub fn advanced_inspect_jump_lba(&mut self, lba: u64) -> Result<(), String> {
        const SECTOR_PAGE: u64 = 64;

        let location = {
            let state = self
                .advanced_inspect
                .as_ref()
                .filter(|state| state.stage == AdvancedInspectStage::Browser)
                .ok_or_else(|| "全盘检查未处于 Browser 状态".to_string())?;
            let workspace = state
                .result
                .as_ref()
                .ok_or_else(|| "Inspect workspace 不可用".to_string())?;
            if !workspace.topology.root.range.contains_lba(lba) {
                return Err(format!("LBA{lba} 超出磁盘范围"));
            }
            workspace
                .topology
                .lazy_sector_location(lba)
                .ok_or_else(|| format!("LBA{lba} 没有可定位的 lazy extent"))?
        };

        let relative = lba
            .checked_sub(location.start_lba)
            .ok_or_else(|| format!("LBA{lba} 位于 extent 起点之前"))?;
        if relative >= location.sector_count {
            return Err(format!("LBA{lba} 超出目标 extent"));
        }
        let row_path = inspect_row_path(&location.node_path);
        let extent_id = row_path
            .last()
            .cloned()
            .ok_or_else(|| format!("LBA{lba} 的 extent 路径为空"))?;
        let page_offset = relative / SECTOR_PAGE * SECTOR_PAGE;
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.expanded.extend(row_path);
            state.lazy_offsets.insert(extent_id.clone(), page_offset);
            state.sector = None;
            state.panel = AdvancedInspectPanel::Tree;
            state.detail_scroll = 0;
            state.message = None;
        }

        let target_id = format!("{extent_id}/sector.{lba}");
        let rows = self.advanced_inspect_tree_rows();
        let target = rows
            .iter()
            .position(|row| row.id == target_id)
            .ok_or_else(|| format!("LBA{lba} 已翻页但目标 Sector 未 materialize"))?;
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.tree_selected = target;
        }
        Ok(())
    }

    pub fn advanced_inspect_jump_byte_offset(
        &mut self,
        offset: u64,
    ) -> Result<Option<(AdvancedInspectSource, u64)>, String> {
        let total_sectors = self
            .advanced_inspect
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
            .and_then(|state| state.result.as_ref())
            .map(|workspace| workspace.topology.root.range.sector_count)
            .ok_or_else(|| "Inspect workspace 不可用".to_string())?;
        let total_bytes = total_sectors
            .checked_mul(crate::common::SECTOR as u64)
            .ok_or_else(|| "磁盘总字节数溢出 u64".to_string())?;
        if offset >= total_bytes {
            return Err(format!(
                "byte offset 0x{offset:X} 超出磁盘范围 0x0..0x{total_bytes:X}"
            ));
        }

        let lba = offset / crate::common::SECTOR as u64;
        let cursor = (offset % crate::common::SECTOR as u64) as usize;
        self.advanced_inspect_jump_lba(lba)?;
        self.advanced_inspect_open_sector_at(lba, cursor)
    }

    fn advanced_inspect_open_sector_at(
        &mut self,
        lba: u64,
        cursor: usize,
    ) -> Result<Option<(AdvancedInspectSource, u64)>, String> {
        if cursor >= crate::common::SECTOR {
            return Err(format!("sector-relative byte {cursor} 越界"));
        }
        let state = self
            .advanced_inspect
            .as_mut()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
            .ok_or_else(|| "全盘检查未处于 Browser 状态".to_string())?;
        let ready = state.result.as_ref().is_some_and(|workspace| {
            workspace.items.iter().any(|item| {
                item.lba == lba && (item.decoded.is_some() || item.decode_error.is_some())
            })
        });
        state.sector = Some(SectorInspectorState {
            lba,
            mode: SectorInspectMode::Mixed,
            cursor,
            pending: !ready,
            error: None,
            field_expanded: false,
            pinned_field: None,
        });
        state.panel = AdvancedInspectPanel::Detail;
        state.detail_scroll = 0;
        Ok((!ready).then(|| (state.source.clone(), lba)))
    }

    pub fn advanced_inspect_search(&mut self, query: &str) -> Result<(), String> {
        let query = query.trim();
        if query.is_empty() {
            return Err("结构化搜索不能为空".into());
        }

        let topology_path = {
            let state = self
                .advanced_inspect
                .as_ref()
                .filter(|state| state.stage == AdvancedInspectStage::Browser)
                .ok_or_else(|| "全盘检查未处于 Browser 状态".to_string())?;
            let workspace = state
                .result
                .as_ref()
                .ok_or_else(|| "Inspect workspace 不可用".to_string())?;
            workspace.topology.find_label_path(query)
        };
        if let Some(node_path) = topology_path {
            let row_path = inspect_row_path(&node_path);
            let target_id = row_path.last().cloned().unwrap_or_default();
            if let Some(state) = self.advanced_inspect.as_mut() {
                state
                    .expanded
                    .extend(row_path.into_iter().take(node_path.len().saturating_sub(1)));
                state.sector = None;
                state.panel = AdvancedInspectPanel::Tree;
                state.detail_scroll = 0;
                state.message = None;
            }
            let rows = self.advanced_inspect_tree_rows();
            let target = rows
                .iter()
                .position(|row| row.id == target_id)
                .ok_or_else(|| "搜索命中节点未能在 Tree 中定位".to_string())?;
            if let Some(state) = self.advanced_inspect.as_mut() {
                state.tree_selected = target;
            }
            return Ok(());
        }

        let cached_target = {
            let workspace = self
                .advanced_inspect
                .as_ref()
                .and_then(|state| state.result.as_ref())
                .ok_or_else(|| "Inspect workspace 不可用".to_string())?;
            workspace.items.iter().find_map(|item| {
                let region = workspace.topology.primary_region_for_lba(item.lba);
                crate::application::inspect_tree::find_sector_structured_path(
                    item.lba,
                    region.and_then(|value| value.decoder),
                    region
                        .map(|value| value.status)
                        .unwrap_or(crate::edpb::SemanticStatus::Unknown),
                    &item.fields,
                    query,
                )
                .map(|path| (item.lba, path))
            })
        };

        let Some((lba, relative_path)) = cached_target else {
            return Err(format!("未找到结构化匹配: {query}"));
        };
        self.advanced_inspect_jump_lba(lba)?;
        let rows = self.advanced_inspect_tree_rows();
        let base_id = rows
            .get(
                self.advanced_inspect
                    .as_ref()
                    .map(|state| state.tree_selected)
                    .unwrap_or(0),
            )
            .filter(|row| row.kind == crate::application::inspect_tree::InspectNodeKind::Sector)
            .map(|row| row.id.clone())
            .ok_or_else(|| format!("LBA{lba} Sector 定位失败"))?;

        let mut target_id = base_id.clone();
        if let Some(state) = self.advanced_inspect.as_mut() {
            if relative_path.len() > 1 {
                state.expanded.insert(base_id.clone());
            }
            for (index, node_id) in relative_path.iter().skip(1).enumerate() {
                target_id = format!("{target_id}/{node_id}");
                if index + 2 < relative_path.len() {
                    state.expanded.insert(target_id.clone());
                }
            }
        }
        let rows = self.advanced_inspect_tree_rows();
        let target = rows
            .iter()
            .position(|row| row.id == target_id)
            .ok_or_else(|| "搜索命中结构未能自动展开到目标节点".to_string())?;
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.tree_selected = target;
            state.panel = AdvancedInspectPanel::Tree;
            state.detail_scroll = 0;
            state.message = None;
        }
        Ok(())
    }

    pub fn advanced_inspect_move_tree(&mut self, delta: isize) {
        let count = self.advanced_inspect_tree_rows().len();
        let Some(state) = self.advanced_inspect.as_mut() else {
            return;
        };
        if state.stage != AdvancedInspectStage::Browser || count == 0 {
            return;
        }
        state.tree_selected = if delta < 0 {
            state.tree_selected.saturating_sub(delta.unsigned_abs())
        } else {
            (state.tree_selected + delta as usize).min(count - 1)
        };
        state.detail_scroll = 0;
        state.sector = None;
    }

    pub fn advanced_inspect_toggle_selected(&mut self) {
        let rows = self.advanced_inspect_tree_rows();
        let selected = self
            .advanced_inspect
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
            .map(|state| state.tree_selected);
        let Some(row) = selected.and_then(|index| rows.get(index)).cloned() else {
            return;
        };

        match row.action {
            AdvancedInspectTreeAction::SetLazyOffset {
                extent_id,
                offset,
                target_id,
            } => {
                if let Some(state) = self.advanced_inspect.as_mut() {
                    state.lazy_offsets.insert(extent_id, offset);
                    state.detail_scroll = 0;
                }
                let rows = self.advanced_inspect_tree_rows();
                if let Some(target_index) = rows.iter().position(|row| row.id == target_id) {
                    if let Some(state) = self.advanced_inspect.as_mut() {
                        state.tree_selected = target_index;
                    }
                }
            }
            AdvancedInspectTreeAction::None => {
                if !row.expandable {
                    return;
                }
                if let Some(state) = self.advanced_inspect.as_mut() {
                    if !state.expanded.remove(&row.id) {
                        state.expanded.insert(row.id.clone());
                    }
                    state.detail_scroll = 0;
                }
                let count = self.advanced_inspect_tree_rows().len();
                if let Some(state) = self.advanced_inspect.as_mut() {
                    state.tree_selected = state.tree_selected.min(count.saturating_sub(1));
                }
            }
        }
    }

    pub fn advanced_inspect_shift_panel(&mut self, reverse: bool) {
        if let Some(state) = self.advanced_inspect.as_mut() {
            if state.stage == AdvancedInspectStage::Browser {
                state.panel = if reverse {
                    match state.panel {
                        AdvancedInspectPanel::Tree => AdvancedInspectPanel::Detail,
                        AdvancedInspectPanel::Overview => AdvancedInspectPanel::Tree,
                        AdvancedInspectPanel::Detail => AdvancedInspectPanel::Overview,
                    }
                } else {
                    match state.panel {
                        AdvancedInspectPanel::Tree => AdvancedInspectPanel::Overview,
                        AdvancedInspectPanel::Overview => AdvancedInspectPanel::Detail,
                        AdvancedInspectPanel::Detail => AdvancedInspectPanel::Tree,
                    }
                };
            }
        }
    }

    pub fn advanced_inspect_enter_selected(&mut self) {
        // Sector rows are opened by `advanced_inspect_open_selected_sector()`
        // so the event loop can schedule the read without putting I/O in state.
        let rows = self.advanced_inspect_tree_rows();
        let selected = self
            .advanced_inspect
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
            .map(|state| state.tree_selected);
        let Some(row) = selected.and_then(|index| rows.get(index)) else {
            return;
        };
        if row.expandable || row.action != AdvancedInspectTreeAction::None {
            self.advanced_inspect_toggle_selected();
        } else if let Some(state) = self.advanced_inspect.as_mut() {
            state.panel = AdvancedInspectPanel::Overview;
            state.detail_scroll = 0;
        }
    }

    pub fn advanced_inspect_selected_field(
        &self,
    ) -> Option<crate::application::inspect::InspectField> {
        let state = self
            .advanced_inspect
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)?;
        let rows = self.advanced_inspect_tree_rows();
        let row = rows.get(state.tree_selected)?;
        if row.kind != crate::application::inspect_tree::InspectNodeKind::Field {
            return None;
        }
        let range = row.range.byte_range?;
        state
            .result
            .as_ref()?
            .items
            .iter()
            .flat_map(|item| item.fields.iter())
            .find(|field| field.range == range)
            .cloned()
    }

    pub fn advanced_inspect_open_selected_field(&mut self) -> Option<(AdvancedInspectSource, u64)> {
        let field = self.advanced_inspect_selected_field()?;
        let lba = field.range.start / crate::common::SECTOR as u64;
        let cursor = (field.range.start % crate::common::SECTOR as u64) as usize;
        let state = self.advanced_inspect.as_mut()?;
        let ready = state.result.as_ref().is_some_and(|workspace| {
            workspace.items.iter().any(|item| {
                item.lba == lba && (item.decoded.is_some() || item.decode_error.is_some())
            })
        });
        state.sector = Some(SectorInspectorState {
            lba,
            mode: SectorInspectMode::Mixed,
            cursor,
            pending: !ready,
            error: None,
            field_expanded: true,
            pinned_field: Some(field),
        });
        state.panel = AdvancedInspectPanel::Detail;
        state.detail_scroll = 0;
        (!ready).then(|| (state.source.clone(), lba))
    }

    pub fn advanced_inspect_selected_sector_lba(&self) -> Option<u64> {
        let state = self
            .advanced_inspect
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)?;
        let rows = self.advanced_inspect_tree_rows();
        let row = rows.get(state.tree_selected)?;
        (row.kind == crate::application::inspect_tree::InspectNodeKind::Sector)
            .then_some(row.range.start_lba)
    }

    pub fn advanced_inspect_open_selected_sector(
        &mut self,
    ) -> Option<(AdvancedInspectSource, u64)> {
        let lba = self.advanced_inspect_selected_sector_lba()?;
        let state = self.advanced_inspect.as_mut()?;
        let ready = state.result.as_ref().is_some_and(|workspace| {
            workspace.items.iter().any(|item| {
                item.lba == lba && (item.decoded.is_some() || item.decode_error.is_some())
            })
        });
        state.sector = Some(SectorInspectorState {
            lba,
            mode: SectorInspectMode::Mixed,
            cursor: 0,
            pending: !ready,
            error: None,
            field_expanded: false,
            pinned_field: None,
        });
        state.panel = AdvancedInspectPanel::Detail;
        state.detail_scroll = 0;
        (!ready).then(|| (state.source.clone(), lba))
    }

    pub fn advanced_inspect_sector(&self) -> Option<&SectorInspectorState> {
        self.advanced_inspect.as_ref()?.sector.as_ref()
    }

    pub fn advanced_inspect_sector_item(
        &self,
    ) -> Option<&crate::application::inspect::AdvancedInspectItem> {
        let state = self.advanced_inspect.as_ref()?;
        let sector = state.sector.as_ref()?;
        state
            .result
            .as_ref()?
            .items
            .iter()
            .find(|item| item.lba == sector.lba)
    }

    pub fn advanced_inspect_sector_finish(
        &mut self,
        lba: u64,
        result: Result<crate::application::inspect::AdvancedInspectItem, String>,
    ) {
        let Some(state) = self.advanced_inspect.as_mut() else {
            return;
        };
        match result {
            Ok(mut item) => {
                const ON_DEMAND_CACHE_LIMIT: usize = 5;
                if let Some(workspace) = state.result.as_mut() {
                    if let Some(index) = workspace.items.iter().position(|old| old.lba == lba) {
                        if item.meta_text.is_none() {
                            item.meta_text = workspace.items[index].meta_text.clone();
                        }
                        workspace.items[index] = item;
                    } else {
                        workspace.items.push(item);
                        workspace.items.sort_by_key(|item| item.lba);
                    }
                    if lba >= crate::common::METADATA_SECTOR_COUNT as u64 {
                        state.sector_cache_order.retain(|cached| *cached != lba);
                        state.sector_cache_order.push_back(lba);
                        while state.sector_cache_order.len() > ON_DEMAND_CACHE_LIMIT {
                            let Some(evicted) = state.sector_cache_order.pop_front() else {
                                break;
                            };
                            if state
                                .sector
                                .as_ref()
                                .is_some_and(|sector| sector.lba == evicted)
                            {
                                state.sector_cache_order.push_back(evicted);
                                continue;
                            }
                            workspace.items.retain(|value| value.lba != evicted);
                        }
                    }
                }
                if let Some(sector) = state.sector.as_mut().filter(|sector| sector.lba == lba) {
                    sector.pending = false;
                    sector.error = state
                        .result
                        .as_ref()
                        .and_then(|workspace| workspace.items.iter().find(|item| item.lba == lba))
                        .and_then(|item| item.decode_error.clone());
                }
            }
            Err(message) => {
                if let Some(sector) = state.sector.as_mut().filter(|sector| sector.lba == lba) {
                    sector.pending = false;
                    sector.error = Some(message);
                }
            }
        }
    }

    pub fn advanced_inspect_sector_move_cursor(&mut self, delta: isize) {
        let Some(sector) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.sector.as_mut())
        else {
            return;
        };
        sector.cursor = if delta < 0 {
            sector.cursor.saturating_sub(delta.unsigned_abs())
        } else {
            sector
                .cursor
                .saturating_add(delta as usize)
                .min(crate::common::SECTOR - 1)
        };
        sector.pinned_field = None;
        sector.field_expanded = false;
    }

    pub fn advanced_inspect_sector_active_field(
        &self,
    ) -> Option<crate::application::inspect::InspectField> {
        let state = self.advanced_inspect.as_ref()?;
        let sector = state.sector.as_ref()?;
        let absolute = sector
            .lba
            .checked_mul(crate::common::SECTOR as u64)?
            .checked_add(sector.cursor as u64)?;
        if let Some(field) = sector
            .pinned_field
            .as_ref()
            .filter(|field| absolute >= field.range.start && absolute < field.range.end_exclusive)
        {
            return Some(field.clone());
        }
        state
            .result
            .as_ref()?
            .items
            .iter()
            .find(|item| item.lba == sector.lba)?
            .fields
            .iter()
            .find(|field| absolute >= field.range.start && absolute < field.range.end_exclusive)
            .cloned()
    }

    pub fn advanced_inspect_sector_yank(&mut self, raw_range: bool) -> Option<String> {
        let field = self.advanced_inspect_sector_active_field();
        let byte = self.advanced_inspect_sector_item().and_then(|item| {
            let cursor = self.advanced_inspect_sector()?.cursor;
            item.raw.get(cursor).copied()
        });
        let value = match (raw_range, field) {
            (true, Some(field)) => field
                .raw
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
            (false, Some(field)) => format!("{} = {}", field.label, field.value),
            (_, None) => format!("0x{:02X}", byte?),
        };
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.yank_register = Some(value.clone());
        }
        Some(value)
    }

    pub fn advanced_inspect_yank_register(&self) -> Option<&str> {
        self.advanced_inspect.as_ref()?.yank_register.as_deref()
    }

    pub fn advanced_inspect_sector_set_mode(&mut self, mode: SectorInspectMode) {
        if let Some(sector) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.sector.as_mut())
        {
            sector.mode = mode;
        }
    }

    pub fn advanced_inspect_sector_toggle_field(&mut self) {
        if let Some(sector) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.sector.as_mut())
        {
            sector.field_expanded = !sector.field_expanded;
        }
    }

    pub fn advanced_inspect_shift_sector(
        &mut self,
        delta: i64,
    ) -> Option<(AdvancedInspectSource, u64)> {
        let state = self.advanced_inspect.as_mut()?;
        let sector = state.sector.as_mut()?;
        let total = state.result.as_ref()?.topology.root.range.sector_count;
        if total == 0 {
            return None;
        }
        let next = if delta < 0 {
            sector.lba.saturating_sub(delta.unsigned_abs())
        } else {
            sector.lba.saturating_add(delta as u64).min(total - 1)
        };
        if next == sector.lba {
            return None;
        }
        sector.lba = next;
        sector.error = None;
        sector.field_expanded = false;
        if let Some(field) = sector.pinned_field.as_ref() {
            let sector_start = next.saturating_mul(crate::common::SECTOR as u64);
            let sector_end = sector_start.saturating_add(crate::common::SECTOR as u64);
            if field.range.start < sector_end && field.range.end_exclusive > sector_start {
                sector.cursor = field
                    .range
                    .start
                    .max(sector_start)
                    .saturating_sub(sector_start) as usize;
            } else {
                sector.pinned_field = None;
            }
        }
        let ready = state.result.as_ref().is_some_and(|workspace| {
            workspace.items.iter().any(|item| {
                item.lba == next && (item.decoded.is_some() || item.decode_error.is_some())
            })
        });
        sector.pending = !ready;
        (!ready).then(|| (state.source.clone(), next))
    }

    pub fn advanced_inspect_close_sector(&mut self) -> bool {
        let Some(state) = self.advanced_inspect.as_mut() else {
            return false;
        };
        if state.sector.take().is_some() {
            state.panel = AdvancedInspectPanel::Overview;
            state.detail_scroll = 0;
            true
        } else {
            false
        }
    }

    pub fn advanced_inspect_scroll_detail(&mut self, delta: isize) {
        if let Some(state) = self.advanced_inspect.as_mut() {
            if state.stage == AdvancedInspectStage::Browser {
                state.detail_scroll = if delta < 0 {
                    state.detail_scroll.saturating_sub(delta.unsigned_abs())
                } else {
                    state.detail_scroll.saturating_add(delta as usize)
                };
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
    pub fn inspect_data(&self) -> Option<&crate::application::inspect::AdvancedInspectWorkspace> {
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

    pub fn replace_inspect(
        &mut self,
        workspace: crate::application::inspect::AdvancedInspectWorkspace,
    ) {
        self.clear_search_matches();
        self.search_query.clear();
        let count = workspace.items.len();
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
        if self.selected_device().is_none() {
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
        let current_target = self
            .selected_device()
            .map(|row| (row.disk, row.size, row.device_id.clone(), kind));
        if self.provision.form_initialized_for == current_target {
            self.provision.stage = ProvisionStage::Form;
            self.provision_sync_cursor_to_end();
            return kind;
        }
        if kind == ProvisionKind::Plain {
            let total_sectors = self
                .selected_device()
                .map(|row| row.size / crate::common::SECTOR as u64)
                .unwrap_or_default();
            match PlainProvisionForm::default_for_disk(total_sectors) {
                Ok(form) => {
                    self.provision.plain_form = form;
                    self.provision.form_initialized_for = current_target;
                    self.provision.stage = ProvisionStage::Form;
                    self.provision.message = None;
                    self.provision_sync_cursor_to_end();
                }
                Err(message) => {
                    self.provision.plain_form = PlainProvisionForm::default();
                    self.provision.stage = ProvisionStage::Form;
                    self.provision.message = Some(message);
                }
            }
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
        let target_mode = kind
            .target()
            .official_mode()
            .expect("TUI official mode menu cannot select Plain");
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
        kind
    }

    pub fn provision_field_count(&self) -> usize {
        (0..)
            .take_while(|&index| self.provision_field_slot(index).is_some())
            .count()
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

    fn plain_field_parts(slot: usize) -> Option<(usize, usize)> {
        let relative = slot.checked_sub(100)?;
        let partition = relative / 4;
        let field = relative % 4;
        (partition < crate::provision::MAX_PLAIN_PARTITIONS).then_some((partition, field))
    }

    fn provision_total_sectors(&self) -> Option<u64> {
        self.selected_device()
            .map(|row| row.size / crate::common::SECTOR as u64)
    }

    fn provision_field_slot(&self, display_index: usize) -> Option<usize> {
        if self.provision.kind == ProvisionKind::Plain {
            let field_count = self.provision.plain_form.partitions.len() * 4;
            return (display_index < field_count).then_some(100 + display_index);
        }
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
        if self.provision.kind == ProvisionKind::Plain {
            for (index, part) in self.provision.plain_form.partitions.iter().enumerate() {
                let number = index + 1;
                let (capacity_label, capacity_value) = match part.input_mode {
                    crate::provision::CapacityInputMode::Exact => (
                        format!("P{number} 容量 (sector)"),
                        part.sector_count.as_str(),
                    ),
                    crate::provision::CapacityInputMode::Quick => (
                        format!(
                            "P{number} 容量 ({})",
                            match part.quick_unit {
                                crate::provision::QuickCapacityUnit::MiB => "MiB",
                                crate::provision::QuickCapacityUnit::GiB => "GiB",
                            }
                        ),
                        part.quick_capacity.as_str(),
                    ),
                };
                out.push((
                    format!("P{number} 起点 LBA"),
                    part.start_lba.as_str(),
                    false,
                ));
                out.push((capacity_label, capacity_value, false));
                out.push((
                    format!("P{number} 文件系统"),
                    part.filesystem.windows_format_name(),
                    false,
                ));
                out.push((format!("P{number} 卷标"), part.volume_label.as_str(), false));
            }
            return out;
        }
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
        let slot = self.provision_field_slot(self.provision.field_selected)?;
        if let Some((partition, field)) = Self::plain_field_parts(slot) {
            let part = self.provision.plain_form.partitions.get_mut(partition)?;
            return match field {
                0 => Some(&mut part.start_lba),
                1 => Some(
                    if part.input_mode == crate::provision::CapacityInputMode::Exact {
                        &mut part.sector_count
                    } else {
                        &mut part.quick_capacity
                    },
                ),
                2 => None,
                3 => Some(&mut part.volume_label),
                _ => None,
            };
        }
        match slot {
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
        let slot = self.provision_field_slot(self.provision.field_selected)?;
        if let Some((partition, field)) = Self::plain_field_parts(slot) {
            let part = self.provision.plain_form.partitions.get(partition)?;
            return match field {
                0 => Some(part.start_lba.as_str()),
                1 => Some(
                    if part.input_mode == crate::provision::CapacityInputMode::Exact {
                        part.sector_count.as_str()
                    } else {
                        part.quick_capacity.as_str()
                    },
                ),
                2 => None,
                3 => Some(part.volume_label.as_str()),
                _ => None,
            };
        }
        match slot {
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
        self.provision.kind.target().official_mode()
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
        if let Some((partition, _)) = Self::plain_field_parts(slot) {
            return Some(match partition {
                0 => "普通分区 P1",
                1 => "普通分区 P2",
                2 => "普通分区 P3",
                3 => "普通分区 P4",
                _ => return None,
            });
        }
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
                _ if section.starts_with("普通分区 P") => 2,
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

    fn provision_selected_capacity_limit(
        &self,
    ) -> Result<
        Option<(
            crate::provision::PartitionRole,
            u64,
            u64,
            Option<(crate::provision::PartitionRole, u64)>,
            u64,
        )>,
        String,
    > {
        use crate::provision::PartitionRole;

        let Some(role) = self.provision_selected_partition_role() else {
            return Ok(None);
        };
        let (resolved, source) = self.provision_resolved_prefill()?;
        let mut parts = resolved.target_partitions(crate::common::SECTOR as u64)?;
        parts.sort_by_key(|part| part.start_lba);
        let Some((index, current)) = parts.iter().enumerate().find(|(_, part)| part.role == role)
        else {
            return Ok(None);
        };
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

        let mut boundary = resolved.usable_end_lba;
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
        Ok(Some((
            current.role,
            current.sector_count,
            max_sectors,
            limiter,
            resolved.usable_end_lba,
        )))
    }

    fn provision_plain_layout_editor_lines(&self) -> Vec<String> {
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
        let plan = match self.provision.plain_form.plan(total_sectors) {
            Ok(plan) => plan,
            Err(message) => {
                lines.push(format!("✗ 布局无效: {message}"));
                lines.push("Insert 添加分区 · Delete 删除当前分区".into());
                return lines;
            }
        };

        let allocated = plan
            .partitions
            .iter()
            .map(|part| part.sector_count)
            .sum::<u64>();
        let unallocated = plan.gaps.iter().map(|gap| gap.sector_count).sum::<u64>();
        lines.push(format!(
            "已分配 {}  ·  空闲 {}",
            Self::format_sector_size(allocated),
            Self::format_sector_size(unallocated)
        ));
        lines.push(String::new());

        let mut entries = Vec::<(u64, String)>::new();
        for gap in &plan.gaps {
            entries.push((
                gap.start_lba,
                format!(
                    "空闲  LBA {}–{}  ·  {}",
                    gap.start_lba,
                    gap.end_lba(),
                    Self::format_sector_size(gap.sector_count)
                ),
            ));
        }
        for (index, part) in plan.partitions.iter().enumerate() {
            entries.push((
                part.start_lba,
                format!(
                    "P{}  LBA {}–{}  ·  {}  ·  {}",
                    index + 1,
                    part.start_lba,
                    part.end_lba().unwrap_or(part.start_lba),
                    Self::format_sector_size(part.sector_count),
                    part.filesystem.windows_format_name()
                ),
            ));
        }
        entries.sort_by_key(|(start, _)| *start);
        lines.extend(entries.into_iter().map(|(_, text)| text));

        lines.push(String::new());
        if let Some(slot) = self.provision_field_slot(self.provision.field_selected) {
            if let Some((partition, _)) = Self::plain_field_parts(slot) {
                if let Some(part) = plan.partitions.get(partition) {
                    if let Ok(max_sectors) = plan.max_sector_count(partition) {
                        lines.push(format!("当前: P{}", partition + 1));
                        lines.push(format!(
                            "大小 {} ({} sector)",
                            Self::format_sector_size(part.sector_count),
                            part.sector_count
                        ));
                        lines.push(format!(
                            "最大可设 {} ({} sector)",
                            Self::format_sector_size(max_sectors),
                            max_sectors
                        ));
                        if let Some(next) = plan
                            .partitions
                            .iter()
                            .filter(|candidate| candidate.start_lba > part.start_lba)
                            .min_by_key(|candidate| candidate.start_lba)
                        {
                            lines.push(format!("限制: 下一分区固定起点 LBA {}", next.start_lba));
                        } else {
                            lines.push(format!(
                                "限制: 磁盘末端 LBA {}",
                                total_sectors.saturating_sub(1)
                            ));
                        }
                    }
                }
            }
        }
        lines.push("✓ 当前布局无重叠、未越界；编辑一个分区不会移动其它分区".into());
        lines.push("Insert 添加分区 · Delete 删除当前分区".into());
        lines
    }

    fn provision_bar_from_segments(
        width: usize,
        segments: Vec<(ProvisionBarKind, u64)>,
    ) -> Vec<ProvisionBarKind> {
        let width = width.clamp(8, 96);
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

    pub fn provision_layout_editor_lines(&self) -> Vec<String> {
        use crate::provision::PartitionRole;

        if self.provision.kind == ProvisionKind::Plain {
            return self.provision_plain_layout_editor_lines();
        }

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
        match self.provision_selected_capacity_limit() {
            Ok(Some((role, current_sectors, max_sectors, limiter, usable_end_lba))) => {
                let grow = max_sectors.saturating_sub(current_sectors);
                lines.push(format!("当前: {}", role.label()));
                lines.push(format!(
                    "大小 {} ({} sector)",
                    Self::format_sector_size(current_sectors),
                    current_sectors
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
                        usable_end_lba.saturating_sub(1)
                    ));
                }
            }
            Ok(None) => lines.push("选中分区容量/单位/起点，可查看最大可设范围".into()),
            Err(message) => lines.push(format!("布局限制无效: {message}")),
        }
        lines.push("✓ 当前布局无重叠、未越界".into());
        lines
    }

    pub fn provision_layout_bar(&self, width: usize) -> Vec<ProvisionBarKind> {
        use crate::provision::PartitionRole;

        if self.provision.kind == ProvisionKind::Plain {
            let Ok(plan) = self.provision_plain_plan() else {
                return vec![ProvisionBarKind::Free; width.clamp(8, 96)];
            };
            let mut ordered = Vec::<(u64, ProvisionBarKind, u64)>::new();
            ordered.extend(
                plan.gaps
                    .iter()
                    .map(|gap| (gap.start_lba, ProvisionBarKind::Free, gap.sector_count)),
            );
            ordered.extend(
                plan.partitions
                    .iter()
                    .map(|part| (part.start_lba, ProvisionBarKind::Plain, part.sector_count)),
            );
            ordered.sort_by_key(|(start, _, _)| *start);
            return Self::provision_bar_from_segments(
                width,
                ordered
                    .into_iter()
                    .map(|(_, kind, sectors)| (kind, sectors))
                    .collect(),
            );
        }

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
        Self::provision_bar_from_segments(width, segments)
    }

    pub fn provision_field_hint(&self, display_index: usize) -> Option<String> {
        let slot = self.provision_field_slot(display_index)?;
        if let Some((_, field)) = Self::plain_field_parts(slot) {
            return match field {
                0 => Some("精确 LBA；不会自动移动其它分区".into()),
                1 => Some("Space 切换 MiB / GiB / sector · f 填满".into()),
                2 => Some("Space 切换 FAT16 / exFAT".into()),
                3 => Some("普通卷标".into()),
                _ => None,
            };
        }
        match slot {
            0..=2 => Some("Space 切换 MiB / GiB / sector · f 填满".into()),
            7 => Some("交换区和保密区的初始密码".into()),
            9 | 11..=13 | 18..=20 | 27 => Some("Space 切换".into()),
            24..=26 => Some("通常无需修改；固定分区边界时再调整".into()),
            28 | 29 => Some("范围 0–255".into()),
            _ => None,
        }
    }

    pub fn provision_fill_selected_capacity(&mut self) -> bool {
        let Some(slot) = self.provision_field_slot(self.provision.field_selected) else {
            return false;
        };
        if let Some((partition, 1)) = Self::plain_field_parts(slot) {
            let Some(total_sectors) = self.provision_total_sectors() else {
                self.provision.message = Some("目标 USB 已不存在".into());
                return true;
            };
            let specs = self
                .provision
                .plain_form
                .partitions
                .iter()
                .enumerate()
                .map(|(index, part)| {
                    let start_lba = part
                        .start_lba
                        .trim()
                        .parse::<u64>()
                        .map_err(|_| format!("P{} 起点 LBA 必须是整数", index + 1))?;
                    Ok(crate::provision::PlainPartitionSpec::new(
                        start_lba,
                        1,
                        part.filesystem,
                        part.volume_label.clone(),
                    ))
                })
                .collect::<Result<Vec<_>, String>>();
            let max_sectors = specs.and_then(|specs| {
                crate::provision::max_plain_sector_count(total_sectors, &specs, partition)
            });
            match max_sectors {
                Ok(max_sectors) if max_sectors > 0 => {
                    if let Some(part) = self.provision.plain_form.partitions.get_mut(partition) {
                        part.set_sector_count(max_sectors);
                        self.provision.message = None;
                        self.provision_sync_cursor_to_end();
                    }
                }
                Ok(_) => self.provision.message = Some("当前普通分区没有可填满的空间".into()),
                Err(message) => self.provision.message = Some(message),
            }
            return true;
        }
        let Some(slot @ 0..=2) = Some(slot) else {
            return false;
        };
        // The selected capacity itself does not determine its upper boundary. Use a
        // one-sector placeholder so f can recover even after the user clears
        // or partially edits the current capacity field. All other form values
        // remain subject to normal strict geometry validation.
        let original_form = self.provision.form.clone();
        match slot {
            0 => {
                self.provision.form.boot_input_mode = crate::provision::CapacityInputMode::Exact;
                self.provision.form.boot_sectors = "1".into();
                self.provision.form.boot_capacity_edited = true;
            }
            1 => {
                self.provision.form.share_input_mode = crate::provision::CapacityInputMode::Exact;
                self.provision.form.share_sectors = "1".into();
                self.provision.form.share_capacity_edited = true;
            }
            2 => {
                self.provision.form.encrypt_input_mode = crate::provision::CapacityInputMode::Exact;
                self.provision.form.encrypt_sectors = "1".into();
                self.provision.form.encrypt_capacity_edited = true;
            }
            _ => unreachable!(),
        }
        let capacity_limit = self.provision_selected_capacity_limit();
        self.provision.form = original_form;

        let max_sectors = match capacity_limit {
            Ok(Some((_, _, max_sectors, _, _))) if max_sectors > 0 => max_sectors,
            Ok(_) => {
                self.provision.message = Some("当前容量没有可填满的有效空间".into());
                return true;
            }
            Err(message) => {
                self.provision.message = Some(message);
                return true;
            }
        };

        use crate::provision::{CapacitySource, QuickCapacityUnit};
        let (unit, quick, exact, edited, source) = match slot {
            0 => (
                self.provision.form.boot_quick_unit,
                &mut self.provision.form.boot_mib,
                &mut self.provision.form.boot_sectors,
                &mut self.provision.form.boot_capacity_edited,
                &mut self.provision.form.boot_capacity_source,
            ),
            1 => (
                self.provision.form.share_quick_unit,
                &mut self.provision.form.share_mib,
                &mut self.provision.form.share_sectors,
                &mut self.provision.form.share_capacity_edited,
                &mut self.provision.form.share_capacity_source,
            ),
            2 => (
                self.provision.form.encrypt_quick_unit,
                &mut self.provision.form.encrypt_mib,
                &mut self.provision.form.encrypt_sectors,
                &mut self.provision.form.encrypt_capacity_edited,
                &mut self.provision.form.encrypt_capacity_source,
            ),
            _ => unreachable!(),
        };
        *exact = max_sectors.to_string();
        *quick = match unit {
            QuickCapacityUnit::MiB => ProvisionForm::format_sector_unit_3(max_sectors, 2_048),
            QuickCapacityUnit::GiB => ProvisionForm::format_sector_unit_3(max_sectors, 2_097_152),
        };
        *edited = false;
        *source = CapacitySource::UserEdited;
        self.provision.message = None;
        self.provision_sync_cursor_to_end();
        true
    }

    pub fn provision_toggle_selected_option(&mut self) -> bool {
        let selected_slot = self.provision_field_slot(self.provision.field_selected);
        if let Some(slot) = selected_slot {
            if let Some((partition, field)) = Self::plain_field_parts(slot) {
                let Some(part) = self.provision.plain_form.partitions.get_mut(partition) else {
                    return false;
                };
                match field {
                    1 => {
                        match part.cycle_capacity_unit() {
                            Ok(()) => self.provision.message = None,
                            Err(message) => self.provision.message = Some(message),
                        }
                        self.provision_sync_cursor_to_end();
                        return true;
                    }
                    2 => {
                        part.filesystem = toggle_supported_fs(part.filesystem);
                        self.provision.message = None;
                        return true;
                    }
                    _ => return false,
                }
            }
        }
        match selected_slot {
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

    pub fn provision_plain_plan(&self) -> Result<crate::provision::PlainProvisionPlan, String> {
        if self.provision.kind != ProvisionKind::Plain {
            return Err("当前不是普通盘目标".into());
        }
        let total_sectors = self
            .provision_total_sectors()
            .ok_or_else(|| "目标 USB 已不存在".to_string())?;
        self.provision.plain_form.plan(total_sectors)
    }

    pub fn provision_plain_add_partition(&mut self) -> bool {
        if self.provision.kind != ProvisionKind::Plain {
            return false;
        }
        if self.provision.plain_form.partitions.len() >= crate::provision::MAX_PLAIN_PARTITIONS {
            self.provision.message = Some("普通盘最多支持 4 个 MBR 主分区".into());
            return true;
        }
        let plan = match self.provision_plain_plan() {
            Ok(plan) => plan,
            Err(message) => {
                self.provision.message = Some(format!("先修正当前布局: {message}"));
                return true;
            }
        };
        let next_start = plan
            .partitions
            .iter()
            .filter_map(|part| part.end_exclusive().ok())
            .max()
            .unwrap_or(crate::provision::DEFAULT_PLAIN_START_LBA);
        if next_start >= plan.total_sectors {
            self.provision.message =
                Some("当前最后一个分区已占满盘尾；请先缩小它再添加分区".into());
            return true;
        }
        let number = self.provision.plain_form.partitions.len() + 1;
        let spec = crate::provision::PlainPartitionSpec::new(
            next_start,
            plan.total_sectors - next_start,
            crate::provision::OfficialFilesystemFormat::ExFat,
            format!("普通卷{number}"),
        );
        self.provision
            .plain_form
            .partitions
            .push(PlainPartitionForm::from_spec(&spec));
        self.provision.field_selected = (number - 1) * 4;
        self.provision.message = None;
        self.provision_sync_cursor_to_end();
        true
    }

    pub fn provision_plain_delete_selected_partition(&mut self) -> bool {
        if self.provision.kind != ProvisionKind::Plain {
            return false;
        }
        if self.provision.plain_form.partitions.len() <= 1 {
            self.provision.message = Some("普通盘至少保留 1 个分区".into());
            return true;
        }
        let Some(slot) = self.provision_field_slot(self.provision.field_selected) else {
            return true;
        };
        let Some((partition, _)) = Self::plain_field_parts(slot) else {
            return true;
        };
        if partition < self.provision.plain_form.partitions.len() {
            self.provision.plain_form.partitions.remove(partition);
            let count = self.provision_field_count();
            self.provision.field_selected =
                self.provision.field_selected.min(count.saturating_sub(1));
            self.provision.message = None;
            self.provision_sync_cursor_to_end();
        }
        true
    }

    pub fn provision_toggle_force_change_password(&mut self) -> bool {
        if self.provision_field_slot(self.provision.field_selected) != Some(9) {
            return false;
        }
        self.provision.form.force_change_password = !self.provision.form.force_change_password;
        self.provision.message = None;
        true
    }

    fn provision_input_policy(&self, slot: usize) -> ProvisionInputPolicy {
        if let Some((partition, field)) = Self::plain_field_parts(slot) {
            return match field {
                0 => ProvisionInputPolicy::UnsignedInteger,
                1 => self
                    .provision
                    .plain_form
                    .partitions
                    .get(partition)
                    .map(|part| {
                        if part.input_mode == crate::provision::CapacityInputMode::Exact {
                            ProvisionInputPolicy::UnsignedInteger
                        } else {
                            ProvisionInputPolicy::DecimalCapacity
                        }
                    })
                    .unwrap_or(ProvisionInputPolicy::UnsignedInteger),
                3 => ProvisionInputPolicy::Text,
                _ => ProvisionInputPolicy::Text,
            };
        }
        match slot {
            0 => {
                if self.provision.form.boot_input_mode == crate::provision::CapacityInputMode::Exact
                {
                    ProvisionInputPolicy::UnsignedInteger
                } else {
                    ProvisionInputPolicy::DecimalCapacity
                }
            }
            1 => {
                if self.provision.form.share_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    ProvisionInputPolicy::UnsignedInteger
                } else {
                    ProvisionInputPolicy::DecimalCapacity
                }
            }
            2 => {
                if self.provision.form.encrypt_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    ProvisionInputPolicy::UnsignedInteger
                } else {
                    ProvisionInputPolicy::DecimalCapacity
                }
            }
            3 => ProvisionInputPolicy::OnlyId,
            24..=26 => ProvisionInputPolicy::UnsignedInteger,
            28 | 29 => ProvisionInputPolicy::U8,
            _ => ProvisionInputPolicy::Text,
        }
    }

    fn provision_mark_capacity_edit(&mut self, slot: Option<usize>) {
        self.provision.form.mark_quick_capacity_edit(slot);
        let Some(slot) = slot else {
            return;
        };
        let Some((partition, 1)) = Self::plain_field_parts(slot) else {
            return;
        };
        if let Some(part) = self.provision.plain_form.partitions.get_mut(partition) {
            if part.input_mode == crate::provision::CapacityInputMode::Quick {
                part.capacity_edited = true;
            }
        }
    }

    pub fn provision_push_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        let cursor = self.provision_field_cursor();
        let Some(slot) = self.provision_field_slot(self.provision.field_selected) else {
            return;
        };
        let Some(current) = self.provision_selected_field() else {
            return;
        };
        if current.chars().count() >= 128 {
            return;
        }
        let mut chars = current.chars().collect::<Vec<_>>();
        chars.insert(cursor.min(chars.len()), ch);
        let candidate = chars.into_iter().collect::<String>();
        let policy = self.provision_input_policy(slot);
        if !policy.accepts(&candidate) {
            self.provision.message = Some(policy.rejection_message().into());
            return;
        }
        if let Some(field) = self.provision_selected_field_mut() {
            *field = candidate;
            self.provision.field_cursor = cursor + 1;
            self.provision_mark_capacity_edit(Some(slot));
            self.provision.message = None;
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
                self.provision_mark_capacity_edit(slot);
                self.provision.message = None;
            }
        }
    }

    pub fn provision_request(
        &mut self,
    ) -> Result<crate::application::provision::OfficialProvisionRequest, String> {
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
        Ok(crate::application::provision::OfficialProvisionRequest {
            target: self.provision.kind.target(),
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
        if self.provision.stage != ProvisionStage::Review || self.provision.prepared.is_none() {
            return;
        }
        self.provision.export_path = match self.provision.kind.mode() {
            Some(mode) => format!("./edp-mode{mode}.img"),
            None => "./edp-plain.img".into(),
        };
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
        crate::application::provision::PreparedProvision,
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
        let prepared = self.provision.prepared.as_ref()?.clone();
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
