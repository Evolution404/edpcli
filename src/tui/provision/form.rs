#[derive(Debug, Clone)]
pub struct ProvisionForm {
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
    pub(super) capacity_edited: bool,
    pub filesystem: crate::provision::OfficialFilesystemFormat,
    pub volume_label: String,
}

impl PlainPartitionForm {
    pub(super) fn from_spec(spec: &crate::provision::PlainPartitionSpec) -> Self {
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

    pub(super) fn set_sector_count(&mut self, sectors: u64) {
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

    pub(super) fn cycle_capacity_unit(&mut self) -> Result<(), String> {
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
    pub(super) fn default_for_disk(total_sectors: u64) -> Result<Self, String> {
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

    pub(super) fn plan(
        &self,
        total_sectors: u64,
    ) -> Result<crate::provision::PlainProvisionPlan, String> {
        crate::provision::PlainProvisionPlan::new(total_sectors, self.specs()?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProvisionInputPolicy {
    DecimalCapacity,
    UnsignedInteger,
    U8,
    OnlyId,
    Text,
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
        }
    }

    pub(super) fn rejection_message(self) -> &'static str {
        match self {
            Self::DecimalCapacity => "容量只允许输入数字和一个小数点",
            Self::UnsignedInteger => "当前字段仅允许输入整数",
            Self::U8 => "该字段仅允许 0–255",
            Self::OnlyId => "标签标识仅允许 u32 或 i32 整数",
            Self::Text => "当前字段包含不支持的字符",
        }
    }
}

pub(super) fn toggle_supported_fs(
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
    pub(super) fn format_sector_unit_3(sectors: u64, sectors_per_unit: u64) -> String {
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

    pub(super) fn resolve_quick_sectors(
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

    pub(super) fn apply_prefill(&mut self, prefill: &crate::provision::ProvisionPrefill) {
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

    pub(super) fn toggle_capacity_input(&mut self, slot: usize) -> Result<(), String> {
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

    pub(super) fn mark_quick_capacity_edit(&mut self, slot: Option<usize>) {
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
