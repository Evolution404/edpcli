pub use crate::domain::secret::SecretText;
#[derive(Clone)]
pub struct ProvisionForm {
    pub logical_sector_bytes: u32,
    pub boot_input_mode: crate::provision::CapacityInputMode,
    pub share_input_mode: crate::provision::CapacityInputMode,
    pub encrypt_input_mode: crate::provision::CapacityInputMode,
    pub boot_quick_unit: crate::provision::QuickCapacityUnit,
    pub share_quick_unit: crate::provision::QuickCapacityUnit,
    pub encrypt_quick_unit: crate::provision::QuickCapacityUnit,
    pub(super) boot_capacity_edited: bool,
    pub(super) share_capacity_edited: bool,
    pub(super) encrypt_capacity_edited: bool,
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
    pub lba8_identity: crate::provision::Lba8Identity,
    pub encryption_algorithm: crate::provision::OfficialLabelAlgorithm,
    pub share_source_algorithm: Option<crate::provision::OfficialLabelAlgorithm>,
    pub encrypt_source_algorithm: Option<crate::provision::OfficialLabelAlgorithm>,
    /// A mixed-mode source cannot be automatically represented by one target selector.
    pub source_algorithm_mixed: bool,
    pub algorithm_user_edited: bool,
    pub share_source_password: SecretText,
    pub share_source_knowledge: crate::provision::SourcePasswordKnowledge,
    pub share_opaque_profile: bool,
    pub share_target_password: SecretText,
    pub encrypt_source_password: SecretText,
    pub encrypt_source_knowledge: crate::provision::SourcePasswordKnowledge,
    pub encrypt_opaque_profile: bool,
    pub encrypt_target_password: SecretText,
    pub volume_label: String,
    pub format_boot: bool,
    pub format_share: bool,
    pub format_encrypt: bool,
    pub share_label: String,
    pub encrypt_label: String,
    pub boot_fs: crate::filesystem::FilesystemKind,
    pub share_fs: crate::filesystem::FilesystemKind,
    pub encrypt_fs: crate::filesystem::FilesystemKind,
    pub force_change_password: bool,
    pub cancel_password_complexity_check: bool,
    pub max_share_password_errors: String,
    pub max_encrypt_password_errors: String,
}

impl std::fmt::Debug for ProvisionForm {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProvisionForm")
            .field("label_id", &self.label_id)
            .field("user", &self.user)
            .field("dept", &self.dept)
            .field("label", &self.label)
            .field("encryption_algorithm", &self.encryption_algorithm)
            .field("share_source_knowledge", &self.share_source_knowledge)
            .field("share_source_password", &"[REDACTED]")
            .field("share_target_password", &"[REDACTED]")
            .field("encrypt_source_knowledge", &self.encrypt_source_knowledge)
            .field("encrypt_source_password", &"[REDACTED]")
            .field("encrypt_target_password", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct PlainPartitionForm {
    pub logical_sector_bytes: u32,
    pub start_lba: String,
    pub input_mode: crate::provision::CapacityInputMode,
    pub quick_unit: crate::provision::QuickCapacityUnit,
    pub quick_capacity: String,
    pub sector_count: String,
    pub(super) capacity_edited: bool,
    pub filesystem: crate::filesystem::FilesystemKind,
    pub volume_label: String,
}

impl PlainPartitionForm {
    pub(super) fn from_spec_native(
        spec: &crate::provision::PlainPartitionSpec,
        logical_sector_bytes: u32,
    ) -> Self {
        Self {
            logical_sector_bytes,
            start_lba: spec.start_lba.to_string(),
            input_mode: crate::provision::CapacityInputMode::Quick,
            quick_unit: crate::provision::QuickCapacityUnit::GiB,
            quick_capacity: ProvisionForm::format_sector_unit_3_native(
                spec.sector_count,
                crate::provision::QuickCapacityUnit::GiB,
                logical_sector_bytes,
            ),
            sector_count: spec.sector_count.to_string(),
            capacity_edited: false,
            filesystem: spec.filesystem,
            volume_label: spec.volume_label.clone(),
        }
    }

    pub(super) fn resolve_sector_count(&self, label: &str) -> Result<u64, String> {
        match self.input_mode {
            crate::provision::CapacityInputMode::Exact => self
                .sector_count
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| format!("{label} sector 必须是大于 0 的整数")),
            crate::provision::CapacityInputMode::Quick => {
                ProvisionForm::resolve_quick_sectors_native(
                    &self.quick_capacity,
                    &self.sector_count,
                    self.quick_unit,
                    self.capacity_edited,
                    label,
                    self.logical_sector_bytes,
                )
            }
        }
    }

    pub(super) fn set_sector_count(&mut self, sectors: u64) {
        self.sector_count = sectors.to_string();
        self.quick_capacity = ProvisionForm::format_sector_unit_3_native(
            sectors,
            self.quick_unit,
            self.logical_sector_bytes,
        );
        self.capacity_edited = false;
    }

    pub(super) fn shift_capacity_unit(&mut self, reverse: bool) -> Result<(), String> {
        use crate::provision::{CapacityInputMode, QuickCapacityUnit};
        let sectors = self.resolve_sector_count("普通分区容量")?;
        self.sector_count = sectors.to_string();
        match (reverse, self.input_mode, self.quick_unit) {
            (true, CapacityInputMode::Exact, _) => {
                self.input_mode = CapacityInputMode::Quick;
                self.quick_unit = QuickCapacityUnit::GiB;
                self.quick_capacity = ProvisionForm::format_sector_unit_3_native(
                    sectors,
                    QuickCapacityUnit::GiB,
                    self.logical_sector_bytes,
                );
            }
            (true, CapacityInputMode::Quick, QuickCapacityUnit::MiB) => {
                self.input_mode = CapacityInputMode::Exact;
            }
            (true, CapacityInputMode::Quick, QuickCapacityUnit::GiB) => {
                self.quick_unit = QuickCapacityUnit::MiB;
                self.quick_capacity = ProvisionForm::format_sector_unit_3_native(
                    sectors,
                    QuickCapacityUnit::MiB,
                    self.logical_sector_bytes,
                );
            }
            (false, CapacityInputMode::Exact, _) => {
                self.input_mode = CapacityInputMode::Quick;
                self.quick_unit = QuickCapacityUnit::MiB;
                self.quick_capacity = ProvisionForm::format_sector_unit_3_native(
                    sectors,
                    QuickCapacityUnit::MiB,
                    self.logical_sector_bytes,
                );
            }
            (false, CapacityInputMode::Quick, QuickCapacityUnit::MiB) => {
                self.quick_unit = QuickCapacityUnit::GiB;
                self.quick_capacity = ProvisionForm::format_sector_unit_3_native(
                    sectors,
                    QuickCapacityUnit::GiB,
                    self.logical_sector_bytes,
                );
            }
            (false, CapacityInputMode::Quick, QuickCapacityUnit::GiB) => {
                self.input_mode = CapacityInputMode::Exact;
            }
        }
        self.capacity_edited = false;
        Ok(())
    }

    pub(super) fn cycle_capacity_unit(&mut self) -> Result<(), String> {
        self.shift_capacity_unit(false)
    }
}

#[derive(Debug, Clone)]
pub struct PlainProvisionForm {
    pub logical_sector_bytes: u32,
    pub partitions: Vec<PlainPartitionForm>,
}

impl Default for PlainProvisionForm {
    fn default() -> Self {
        Self {
            logical_sector_bytes: 512,
            partitions: Vec::new(),
        }
    }
}
impl PlainProvisionForm {
    pub(super) fn default_for_disk_native(
        total_sectors: u64,
        sector_bytes: u32,
    ) -> Result<Self, String> {
        if !crate::domain::hardware::valid_native_sector_bytes(sector_bytes) {
            return Err("普通盘原生逻辑扇区大小无效".into());
        }
        let plan = crate::provision::PlainProvisionPlan::default_for_disk(total_sectors)?;
        Ok(Self {
            logical_sector_bytes: sector_bytes,
            partitions: plan
                .partitions
                .iter()
                .map(|part| PlainPartitionForm::from_spec_native(part, sector_bytes))
                .collect(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProvisionInputPolicy {
    DecimalCapacity,
    UnsignedInteger,
    U8,
    OnlyId,
    Text,
    Lba8Text,
}

impl ProvisionInputPolicy {
    pub(super) fn accepts(self, candidate: &str) -> bool {
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
            Self::Lba8Text => candidate
                .chars()
                .all(|ch| !ch.is_control() && ch != '|' && ch != '='),
        }
    }

    pub(super) fn rejection_message(self) -> &'static str {
        match self {
            Self::DecimalCapacity => "容量只允许输入数字和一个小数点",
            Self::UnsignedInteger => "当前字段仅允许输入整数",
            Self::U8 => "该字段仅允许 0–255",
            Self::OnlyId => "标签标识仅允许 u32 或 i32 整数",
            Self::Text => "当前字段包含不支持的字符",
            Self::Lba8Text => "LBA8 字段不允许控制字符、| 或 =",
        }
    }
}

pub(super) fn toggle_supported_fs(
    value: crate::filesystem::FilesystemKind,
) -> crate::filesystem::FilesystemKind {
    shift_supported_fs(value, false)
}

pub(super) fn shift_supported_fs(
    value: crate::filesystem::FilesystemKind,
    reverse: bool,
) -> crate::filesystem::FilesystemKind {
    crate::filesystem::shift_writable_filesystem(value, reverse)
}

impl Default for ProvisionForm {
    fn default() -> Self {
        Self {
            logical_sector_bytes: 512,
            boot_input_mode: crate::provision::CapacityInputMode::Exact,
            share_input_mode: crate::provision::CapacityInputMode::Quick,
            encrypt_input_mode: crate::provision::CapacityInputMode::Quick,
            boot_quick_unit: crate::provision::QuickCapacityUnit::MiB,
            share_quick_unit: crate::provision::QuickCapacityUnit::GiB,
            encrypt_quick_unit: crate::provision::QuickCapacityUnit::GiB,
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
            share_mib: ProvisionForm::format_sector_unit_3(
                2_097_152,
                crate::provision::QuickCapacityUnit::GiB,
            ),
            share_sectors: String::new(),
            encrypt_mib: ProvisionForm::format_sector_unit_3(
                2_097_152,
                crate::provision::QuickCapacityUnit::GiB,
            ),
            encrypt_sectors: String::new(),
            label_id: crate::provision::OnlyId::random_candidate()
                .map(|value| value.text().to_string())
                .unwrap_or_else(|_| "1".into()),
            user: String::new(),
            dept: String::new(),
            label: crate::provision::DEFAULT_SAFE6_LABEL.into(),
            lba8_identity: crate::provision::Lba8Identity::default(),
            encryption_algorithm: crate::provision::OfficialLabelAlgorithm::Sms4,
            share_source_algorithm: None,
            encrypt_source_algorithm: None,
            source_algorithm_mixed: false,
            algorithm_user_edited: false,
            share_source_password: SecretText::default(),
            share_source_knowledge: crate::provision::SourcePasswordKnowledge::Unknown,
            share_opaque_profile: false,
            share_target_password: crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD_TEXT.into(),
            encrypt_source_password: SecretText::default(),
            encrypt_source_knowledge: crate::provision::SourcePasswordKnowledge::Unknown,
            encrypt_opaque_profile: false,
            encrypt_target_password: crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD_TEXT.into(),
            volume_label: "启动区".into(),
            format_boot: false,
            format_share: false,
            format_encrypt: false,
            share_label: "交换区".into(),
            encrypt_label: "保密区".into(),
            boot_fs: crate::filesystem::FilesystemKind::Fat16,
            share_fs: crate::filesystem::FilesystemKind::ExFat,
            encrypt_fs: crate::filesystem::FilesystemKind::ExFat,
            force_change_password: false,
            cancel_password_complexity_check: false,
            max_share_password_errors: u8::MAX.to_string(),
            max_encrypt_password_errors: u8::MAX.to_string(),
        }
    }
}

impl ProvisionForm {
    pub(crate) fn lba8_identity(&self) -> crate::provision::Lba8Identity {
        self.lba8_identity.clone()
    }

    pub(super) const fn quick_unit_label(
        unit: crate::provision::QuickCapacityUnit,
    ) -> &'static str {
        use crate::common::{CapacityUnitSystem, CAPACITY_UNIT_SYSTEM};
        use crate::provision::QuickCapacityUnit;
        match (CAPACITY_UNIT_SYSTEM, unit) {
            (CapacityUnitSystem::Decimal, QuickCapacityUnit::MiB) => "MB",
            (CapacityUnitSystem::Decimal, QuickCapacityUnit::GiB) => "GB",
            (CapacityUnitSystem::Binary, QuickCapacityUnit::MiB) => "MiB",
            (CapacityUnitSystem::Binary, QuickCapacityUnit::GiB) => "GiB",
        }
    }

    pub(super) const fn quick_unit_bytes(unit: crate::provision::QuickCapacityUnit) -> u64 {
        use crate::common::{CapacityUnitSystem, CAPACITY_UNIT_SYSTEM};
        use crate::provision::QuickCapacityUnit;
        match (CAPACITY_UNIT_SYSTEM, unit) {
            (CapacityUnitSystem::Decimal, QuickCapacityUnit::MiB) => 1_000_000,
            (CapacityUnitSystem::Decimal, QuickCapacityUnit::GiB) => 1_000_000_000,
            (CapacityUnitSystem::Binary, QuickCapacityUnit::MiB) => 1_048_576,
            (CapacityUnitSystem::Binary, QuickCapacityUnit::GiB) => 1_073_741_824,
        }
    }

    pub(super) fn format_sector_unit_3(
        sectors: u64,
        unit: crate::provision::QuickCapacityUnit,
    ) -> String {
        Self::format_sector_unit_3_native(sectors, unit, 512)
    }

    pub(super) fn format_sector_unit_3_native(
        sectors: u64,
        unit: crate::provision::QuickCapacityUnit,
        logical_sector_bytes: u32,
    ) -> String {
        let bytes = (sectors as u128) * u128::from(logical_sector_bytes);
        let unit_bytes = Self::quick_unit_bytes(unit) as u128;
        let scaled = (bytes * 1_000 + unit_bytes / 2) / unit_bytes;
        format!("{}.{:03}", scaled / 1_000, scaled % 1_000)
    }

    fn parse_decimal_unit_to_sectors_rounded(
        value: &str,
        unit: crate::provision::QuickCapacityUnit,
        label: &str,
        logical_sector_bytes: u32,
    ) -> Result<u64, String> {
        if !crate::domain::hardware::valid_native_sector_bytes(logical_sector_bytes) {
            return Err("容量输入的原生逻辑扇区大小无效".into());
        }
        let unit_name = Self::quick_unit_label(unit);
        let unit_bytes = Self::quick_unit_bytes(unit) as u128;
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
        let scaled_bytes = numerator
            .checked_mul(unit_bytes)
            .ok_or_else(|| format!("{label} {unit_name} 容量溢出"))?;
        let sector_denominator = denominator
            .checked_mul(u128::from(logical_sector_bytes))
            .ok_or_else(|| format!("{label} {unit_name} 容量溢出"))?;
        let quotient = scaled_bytes / sector_denominator;
        let remainder = scaled_bytes % sector_denominator;
        let rounded = quotient
            + if logical_sector_bytes == 512 {
                // Preserve the legacy 512B UI rounding contract.
                u128::from(remainder.saturating_mul(2) >= sector_denominator)
            } else {
                // Non-512B requests must never silently allocate less than entered.
                u128::from(remainder != 0)
            };
        u64::try_from(rounded).map_err(|_| format!("{label} {unit_name} 容量溢出"))
    }

    pub(super) fn resolve_quick_sectors_native(
        quick: &str,
        exact: &str,
        unit: crate::provision::QuickCapacityUnit,
        edited: bool,
        label: &str,
        logical_sector_bytes: u32,
    ) -> Result<u64, String> {
        if !crate::domain::hardware::valid_native_sector_bytes(logical_sector_bytes) {
            return Err("容量输入的原生逻辑扇区大小无效".into());
        }
        if let Some(sectors) = exact.parse::<u64>().ok().filter(|_| !edited) {
            let generated = Self::format_sector_unit_3_native(sectors, unit, logical_sector_bytes);
            if quick == generated {
                return Ok(sectors);
            }
        }
        Self::parse_decimal_unit_to_sectors_rounded(quick, unit, label, logical_sector_bytes)
    }

    pub(super) fn apply_prefill(&mut self, prefill: &crate::provision::ProvisionPrefill) {
        use crate::provision::CapacityInputMode;
        self.logical_sector_bytes = prefill.logical_sector_bytes;
        self.boot_capacity_edited = false;
        self.share_capacity_edited = false;
        self.encrypt_capacity_edited = false;
        let set = |input: Option<crate::provision::CapacityInput>,
                   mode: &mut CapacityInputMode,
                   unit: crate::provision::QuickCapacityUnit,
                   quick: &mut String,
                   sectors: &mut String,
                   source: &mut crate::provision::CapacitySource| {
            if let Some(input) = input {
                *mode = input.mode();
                *sectors = input.sectors().to_string();
                *source = input.source();
                *quick = Self::format_sector_unit_3_native(
                    input.sectors(),
                    unit,
                    prefill.logical_sector_bytes,
                );
            }
        };
        set(
            prefill.boot,
            &mut self.boot_input_mode,
            self.boot_quick_unit,
            &mut self.boot_mib,
            &mut self.boot_sectors,
            &mut self.boot_capacity_source,
        );
        set(
            prefill.share,
            &mut self.share_input_mode,
            self.share_quick_unit,
            &mut self.share_mib,
            &mut self.share_sectors,
            &mut self.share_capacity_source,
        );
        set(
            prefill.encrypt,
            &mut self.encrypt_input_mode,
            self.encrypt_quick_unit,
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

    pub(super) fn toggle_capacity_input(
        &mut self,
        role: crate::provision::PartitionRole,
    ) -> Result<(), String> {
        self.shift_capacity_input(role, false)
    }

    pub(super) fn shift_capacity_input(
        &mut self,
        role: crate::provision::PartitionRole,
        reverse: bool,
    ) -> Result<(), String> {
        use crate::provision::{CapacityInputMode, QuickCapacityUnit};
        let native_bytes = self.logical_sector_bytes;
        let (mode, unit, quick, exact, edited) = match role {
            crate::provision::PartitionRole::Boot => (
                &mut self.boot_input_mode,
                &mut self.boot_quick_unit,
                &mut self.boot_mib,
                &mut self.boot_sectors,
                &mut self.boot_capacity_edited,
            ),
            crate::provision::PartitionRole::Share
            | crate::provision::PartitionRole::BootShareCombined => (
                &mut self.share_input_mode,
                &mut self.share_quick_unit,
                &mut self.share_mib,
                &mut self.share_sectors,
                &mut self.share_capacity_edited,
            ),
            crate::provision::PartitionRole::Encrypt => (
                &mut self.encrypt_input_mode,
                &mut self.encrypt_quick_unit,
                &mut self.encrypt_mib,
                &mut self.encrypt_sectors,
                &mut self.encrypt_capacity_edited,
            ),
            crate::provision::PartitionRole::CompatibilityReserve => {
                return Err("兼容保留区没有可编辑容量字段".into())
            }
        };
        match (reverse, *mode, *unit) {
            (true, CapacityInputMode::Exact, _) => {
                let sectors = exact
                    .parse::<u64>()
                    .map_err(|_| "请先输入有效的 sector 数".to_string())?;
                *quick = Self::format_sector_unit_3_native(
                    sectors,
                    QuickCapacityUnit::GiB,
                    native_bytes,
                );
                *mode = CapacityInputMode::Quick;
                *unit = QuickCapacityUnit::GiB;
            }
            (true, CapacityInputMode::Quick, QuickCapacityUnit::MiB) => {
                let sectors = Self::resolve_quick_sectors_native(
                    quick,
                    exact,
                    *unit,
                    *edited,
                    "当前容量",
                    native_bytes,
                )?;
                *exact = sectors.to_string();
                *mode = CapacityInputMode::Exact;
            }
            (true, CapacityInputMode::Quick, QuickCapacityUnit::GiB) => {
                let sectors = Self::resolve_quick_sectors_native(
                    quick,
                    exact,
                    *unit,
                    *edited,
                    "当前容量",
                    native_bytes,
                )?;
                *exact = sectors.to_string();
                *quick = Self::format_sector_unit_3_native(
                    sectors,
                    QuickCapacityUnit::MiB,
                    native_bytes,
                );
                *unit = QuickCapacityUnit::MiB;
            }
            (false, CapacityInputMode::Exact, _) => {
                let sectors = exact
                    .parse::<u64>()
                    .map_err(|_| "请先输入有效的 sector 数".to_string())?;
                *quick = Self::format_sector_unit_3_native(
                    sectors,
                    QuickCapacityUnit::MiB,
                    native_bytes,
                );
                *mode = CapacityInputMode::Quick;
                *unit = QuickCapacityUnit::MiB;
            }
            (false, CapacityInputMode::Quick, QuickCapacityUnit::MiB) => {
                let sectors = Self::resolve_quick_sectors_native(
                    quick,
                    exact,
                    *unit,
                    *edited,
                    "当前容量",
                    native_bytes,
                )?;
                *exact = sectors.to_string();
                *quick = Self::format_sector_unit_3_native(
                    sectors,
                    QuickCapacityUnit::GiB,
                    native_bytes,
                );
                *unit = QuickCapacityUnit::GiB;
            }
            (false, CapacityInputMode::Quick, QuickCapacityUnit::GiB) => {
                let sectors = Self::resolve_quick_sectors_native(
                    quick,
                    exact,
                    *unit,
                    *edited,
                    "当前容量",
                    native_bytes,
                )?;
                *exact = sectors.to_string();
                *mode = CapacityInputMode::Exact;
            }
        }
        *edited = false;
        Ok(())
    }

    pub(super) fn mark_quick_capacity_edit(
        &mut self,
        role: Option<crate::provision::PartitionRole>,
    ) {
        use crate::provision::CapacityInputMode;
        match role {
            Some(crate::provision::PartitionRole::Boot)
                if self.boot_input_mode == CapacityInputMode::Quick =>
            {
                self.boot_capacity_edited = true;
            }
            Some(
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined,
            ) if self.share_input_mode == CapacityInputMode::Quick => {
                self.share_capacity_edited = true;
            }
            Some(crate::provision::PartitionRole::Encrypt)
                if self.encrypt_input_mode == CapacityInputMode::Quick =>
            {
                self.encrypt_capacity_edited = true;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod native_capacity_tests {
    use super::{PlainPartitionForm, PlainProvisionForm, ProvisionForm};
    use crate::filesystem::FilesystemKind;
    use crate::provision::{CapacityInputMode, PlainPartitionSpec, QuickCapacityUnit as Unit};

    #[test]
    fn native_quick_capacity_uses_actual_sector_bytes_and_never_undercounts() {
        for sector_bytes in [512u32, 1024, 1536, 2048, 2560, 3072, 4096, 8192] {
            let sectors = 131_071u64;
            let display =
                ProvisionForm::format_sector_unit_3_native(sectors, Unit::GiB, sector_bytes);
            let preserved = ProvisionForm::resolve_quick_sectors_native(
                &display,
                &sectors.to_string(),
                Unit::GiB,
                false,
                "测试容量",
                sector_bytes,
            )
            .unwrap();
            assert_eq!(preserved, sectors);

            let requested = ProvisionForm::quick_unit_bytes(Unit::MiB);
            let parsed = ProvisionForm::resolve_quick_sectors_native(
                "1",
                "",
                Unit::MiB,
                true,
                "测试容量",
                sector_bytes,
            )
            .unwrap();
            if sector_bytes == 512 {
                let approximate = (requested + 256) / 512;
                assert_eq!(parsed, approximate);
            } else {
                assert_eq!(parsed, requested.div_ceil(u64::from(sector_bytes)));
                assert!(parsed * u64::from(sector_bytes) >= requested);
            }
        }
        for invalid in [0, 511, 513, 4097] {
            assert!(ProvisionForm::resolve_quick_sectors_native(
                "1",
                "",
                Unit::MiB,
                true,
                "测试容量",
                invalid
            )
            .is_err());
        }
        assert_eq!(
            ProvisionForm::format_sector_unit_3(2048, Unit::MiB),
            ProvisionForm::format_sector_unit_3_native(2048, Unit::MiB, 512)
        );
    }

    #[test]
    fn plain_native_default_and_mode_toggles_keep_native_lba_count() {
        for sector_bytes in [512u32, 1024, 2048, 4096] {
            let total = 120_000;
            let form = PlainProvisionForm::default_for_disk_native(total, sector_bytes).unwrap();
            assert_eq!(form.logical_sector_bytes, sector_bytes);
            assert_eq!(form.plan(total).unwrap().total_sectors, total);
            assert!(form
                .partitions
                .iter()
                .all(|part| part.logical_sector_bytes == sector_bytes));
            let mut part = PlainPartitionForm::from_spec_native(
                &PlainPartitionSpec::new(2048, 4096, FilesystemKind::Fat16, "TEST"),
                sector_bytes,
            );
            assert_eq!(part.resolve_sector_count("普通分区").unwrap(), 4096);
            part.shift_capacity_unit(false).unwrap();
            assert_eq!(part.resolve_sector_count("普通分区").unwrap(), 4096);
            part.input_mode = CapacityInputMode::Quick;
            part.quick_unit = Unit::MiB;
            part.quick_capacity = "1".into();
            part.capacity_edited = true;
            let unit_bytes = ProvisionForm::quick_unit_bytes(Unit::MiB);
            let expected = if sector_bytes == 512 {
                (unit_bytes + 256) / 512
            } else {
                unit_bytes.div_ceil(u64::from(sector_bytes))
            };
            assert_eq!(part.resolve_sector_count("普通分区").unwrap(), expected);
        }
    }
}
