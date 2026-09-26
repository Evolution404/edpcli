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
    Unknown,
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

#[path = "fields.rs"]
mod fields;
#[path = "form.rs"]
mod form;
#[path = "plain_editor.rs"]
mod plain_editor;

use form::{toggle_supported_fs, ProvisionInputPolicy};
pub use form::{PlainPartitionForm, PlainProvisionForm, ProvisionForm};
use plain_editor::plain_field_parts;

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
            if let Some((partition, _)) = plain_field_parts(slot) {
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

    pub fn provision_layout_model(&self) -> crate::tui::disk_layout::DiskLayoutModel {
        use crate::provision::PartitionRole;
        use crate::tui::disk_layout::{DiskLayoutModel, DiskLayoutSegment};

        if self.provision.kind == ProvisionKind::Plain {
            let Ok(plan) = self.provision_plain_plan() else {
                return DiskLayoutModel::new(0, Vec::new());
            };
            let mut segments = Vec::new();
            segments.extend(plan.gaps.iter().map(|gap| DiskLayoutSegment {
                label: "空闲".into(),
                start_lba: gap.start_lba,
                sector_count: gap.sector_count,
                kind: ProvisionBarKind::Free,
            }));
            segments.extend(plan.partitions.iter().enumerate().map(|(index, part)| {
                DiskLayoutSegment {
                    label: format!("普通分区[{}]", index),
                    start_lba: part.start_lba,
                    sector_count: part.sector_count,
                    kind: ProvisionBarKind::Plain,
                }
            }));
            return DiskLayoutModel::new(plan.total_sectors, segments);
        }

        let Ok((resolved, _)) = self.provision_resolved_prefill() else {
            return DiskLayoutModel::new(0, Vec::new());
        };
        let Ok(mut parts) = resolved.target_partitions(crate::common::SECTOR as u64) else {
            return DiskLayoutModel::new(0, Vec::new());
        };
        parts.sort_by_key(|part| part.start_lba);
        let usable_start = crate::provision::OFFICIAL_PARTITION_START_SECTOR;
        let usable_sectors = resolved.usable_end_lba.saturating_sub(usable_start);
        if usable_sectors == 0 {
            return DiskLayoutModel::new(0, Vec::new());
        }

        let mut segments = Vec::<DiskLayoutSegment>::new();
        let mut cursor = usable_start;
        for part in &parts {
            if part.start_lba > cursor {
                segments.push(DiskLayoutSegment {
                    label: "空闲".into(),
                    start_lba: cursor,
                    sector_count: part.start_lba.saturating_sub(cursor),
                    kind: ProvisionBarKind::Free,
                });
            }
            let kind = match part.role {
                PartitionRole::Boot => ProvisionBarKind::Boot,
                PartitionRole::Share | PartitionRole::BootShareCombined => ProvisionBarKind::Share,
                PartitionRole::Encrypt => ProvisionBarKind::Encrypt,
                PartitionRole::CompatibilityReserve => ProvisionBarKind::Compatibility,
            };
            segments.push(DiskLayoutSegment {
                label: part.role.label().into(),
                start_lba: part.start_lba,
                sector_count: part.sector_count,
                kind,
            });
            cursor = part.start_lba.saturating_add(part.sector_count);
        }
        if cursor < resolved.usable_end_lba {
            segments.push(DiskLayoutSegment {
                label: "空闲".into(),
                start_lba: cursor,
                sector_count: resolved.usable_end_lba.saturating_sub(cursor),
                kind: ProvisionBarKind::Free,
            });
        }
        DiskLayoutModel::new(usable_sectors, segments)
    }

    pub fn provision_layout_bar(&self, width: usize) -> Vec<ProvisionBarKind> {
        self.provision_layout_model().bar(width)
    }

    pub fn provision_fill_selected_capacity(&mut self) -> bool {
        let Some(slot) = self.provision_field_slot(self.provision.field_selected) else {
            return false;
        };
        if let Some((partition, 1)) = plain_field_parts(slot) {
            let Some(total_sectors) = self.provision_total_sectors() else {
                self.provision.message = Some("目标 USB 已不存在".into());
                return true;
            };
            match self
                .provision
                .plain_form
                .fill_partition_capacity(total_sectors, partition)
            {
                Ok(()) => {
                    self.provision.message = None;
                    self.provision_sync_cursor_to_end();
                }
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
            if let Some((partition, field)) = plain_field_parts(slot) {
                let result = self
                    .provision
                    .plain_form
                    .toggle_partition_option(partition, field);
                match result {
                    Ok(true) if field == 1 => {
                        self.provision.message = None;
                        self.provision_sync_cursor_to_end();
                        return true;
                    }
                    Ok(true) => {
                        self.provision.message = None;
                        return true;
                    }
                    Ok(false) => return false,
                    Err(message) => {
                        self.provision.message = Some(message);
                        if field == 1 {
                            self.provision_sync_cursor_to_end();
                        }
                        return true;
                    }
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
        let total_sectors = self.provision_total_sectors();
        match self.provision.plain_form.add_partition(total_sectors) {
            Ok(index) => {
                self.provision.field_selected = index * 4;
                self.provision.message = None;
                self.provision_sync_cursor_to_end();
            }
            Err(message) => {
                self.provision.message = Some(message);
            }
        }
        true
    }

    pub fn provision_plain_delete_selected_partition(&mut self) -> bool {
        if self.provision.kind != ProvisionKind::Plain {
            return false;
        }
        let partition = self
            .provision_field_slot(self.provision.field_selected)
            .and_then(plain_field_parts)
            .map(|(partition, _)| partition);
        match self.provision.plain_form.delete_partition(partition) {
            Ok(true) => {
                let count = self.provision_field_count();
                self.provision.field_selected =
                    self.provision.field_selected.min(count.saturating_sub(1));
                self.provision.message = None;
                self.provision_sync_cursor_to_end();
            }
            Ok(false) => {}
            Err(message) => self.provision.message = Some(message),
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
