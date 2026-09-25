use super::*;

type ProvisionCapacityLimit = (
    crate::provision::PartitionRole,
    u64,
    u64,
    Option<(crate::provision::PartitionRole, u64)>,
    u64,
);

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
    pub(super) target_disk: Option<u32>,
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

impl AppState {
    pub fn provision_mut(&mut self) -> &mut ProvisionState {
        &mut self.provision
    }

    pub fn provision_begin_insert(&mut self) -> bool {
        if self.workspace != Workspace::Provision
            || self.provision.stage != ProvisionStage::Form
            || !self.provision_selected_field_is_editable()
        {
            return false;
        }
        self.input_mode = InputMode::Insert;
        self.provision_sync_cursor_to_end();
        true
    }

    pub fn provision_end_insert(&mut self) {
        if self.input_mode == InputMode::Insert {
            self.input_mode = InputMode::Normal;
        }
    }

    pub fn provision_reset(&mut self) {
        self.input_mode = InputMode::Normal;
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

    fn provision_selected_capacity_limit(&self) -> Result<Option<ProvisionCapacityLimit>, String> {
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

    pub fn provision_delete_char(&mut self) {
        let cursor = self.provision_field_cursor();
        let slot = self.provision_field_slot(self.provision.field_selected);
        if let Some(field) = self.provision_selected_field_mut() {
            let mut chars = field.chars().collect::<Vec<_>>();
            if cursor < chars.len() {
                chars.remove(cursor);
                *field = chars.into_iter().collect();
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
        self.input_mode = InputMode::Normal;
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
        self.input_mode = InputMode::Insert;
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
        self.input_mode = InputMode::Normal;
        self.provision.message = Some(match result {
            Ok(path) => format!("镜像导出完成：{}", path.display()),
            Err(message) => message,
        });
    }

    pub fn provision_cancel_export(&mut self) {
        if self.provision.stage == ProvisionStage::ExportPath {
            self.provision.stage = ProvisionStage::Review;
            self.provision.message = None;
            self.input_mode = InputMode::Normal;
        }
    }

    pub fn provision_begin_confirm(&mut self) {
        if self.provision.prepared.is_some() {
            self.provision.stage = ProvisionStage::Confirm;
            self.input_mode = InputMode::Confirm;
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
        self.input_mode = InputMode::Normal;
        self.provision.message = Some("事务写盘进行中；退出请求会延迟到安全检查点".into());
        self.critical_operation = true;
        Some(prepared)
    }

    pub fn provision_finish_write(&mut self, result: Result<String, String>) {
        self.critical_operation = false;
        self.provision.stage = ProvisionStage::Result;
        self.input_mode = InputMode::Normal;
        self.provision.message = Some(match result {
            Ok(message) => message,
            Err(message) => message,
        });
    }
}
