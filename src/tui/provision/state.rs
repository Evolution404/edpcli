use super::*;

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

#[path = "editor.rs"]
mod editor;
#[path = "fields.rs"]
mod fields;
#[path = "form.rs"]
mod form;
#[path = "layout.rs"]
mod layout;
#[path = "pane.rs"]
mod pane;
#[path = "plain_editor.rs"]
mod plain_editor;
#[path = "review.rs"]
mod review;
#[path = "validation.rs"]
mod validation;

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
    pub pane_focus: crate::tui::pane::PaneFocus,
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
            pane_focus: crate::tui::pane::PaneFocus::provision_form(),
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
        self.provision.pane_focus = crate::tui::pane::PaneFocus::provision_form();
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

    pub fn provision_finish_key_probe(
        &mut self,
        result: Result<crate::application::provision::ProvisionKeyProbe, String>,
    ) {
        if self.provision.stage != ProvisionStage::Form {
            return;
        }
        match result {
            Ok(probe) => {
                let mut status = Vec::new();
                self.provision.form.share_opaque_profile = probe.share_opaque_profile;
                self.provision.form.encrypt_opaque_profile = probe.encrypt_opaque_profile;
                if let Some(knowledge) = probe.share {
                    if self.provision.form.share_source_password.is_empty() {
                        self.provision.form.share_source_knowledge = knowledge;
                        if knowledge == crate::provision::SourcePasswordKnowledge::DefaultVerified {
                            self.provision.form.share_source_password = "0000aaaa".into();
                        }
                    }
                    status.push(format!(
                        "交换域:{}",
                        match self.provision.form.share_source_knowledge {
                            crate::provision::SourcePasswordKnowledge::DefaultVerified => {
                                "默认密码已验证"
                            }
                            crate::provision::SourcePasswordKnowledge::UserVerified => {
                                "用户密码已验证"
                            }
                            crate::provision::SourcePasswordKnowledge::Unknown => "Unknown",
                        }
                    ));
                }
                if let Some(knowledge) = probe.encrypt {
                    if self.provision.form.encrypt_source_password.is_empty() {
                        self.provision.form.encrypt_source_knowledge = knowledge;
                        if knowledge == crate::provision::SourcePasswordKnowledge::DefaultVerified {
                            self.provision.form.encrypt_source_password = "0000aaaa".into();
                        }
                    }
                    status.push(format!(
                        "保密域:{}",
                        match self.provision.form.encrypt_source_knowledge {
                            crate::provision::SourcePasswordKnowledge::DefaultVerified => {
                                "默认密码已验证"
                            }
                            crate::provision::SourcePasswordKnowledge::UserVerified => {
                                "用户密码已验证"
                            }
                            crate::provision::SourcePasswordKnowledge::Unknown => "Unknown",
                        }
                    ));
                }
                self.provision.message = Some(if status.is_empty() {
                    format!(
                        "来源状态: {} · 无 EDP 用户密码域",
                        probe.source_kind.short_name()
                    )
                } else {
                    format!(
                        "来源状态: {} · {}",
                        probe.source_kind.short_name(),
                        status.join(" · ")
                    )
                });
                self.provision_sync_cursor_to_end();
            }
            Err(message) => {
                self.provision.message = Some(format!("来源密码域只读探测失败: {message}"));
            }
        }
    }

    pub fn provision_finish_source_password_verify(
        &mut self,
        domain: crate::provision::KeyDomainRole,
        result: Result<crate::provision::SourcePasswordKnowledge, String>,
    ) {
        if self.provision.stage != ProvisionStage::Form {
            return;
        }
        match (domain, result) {
            (crate::provision::KeyDomainRole::Share, Ok(knowledge)) => {
                self.provision.form.share_source_knowledge = knowledge;
                self.provision.message = Some("交换域来源密码验证通过。".into());
            }
            (crate::provision::KeyDomainRole::Encrypt, Ok(knowledge)) => {
                self.provision.form.encrypt_source_knowledge = knowledge;
                self.provision.message = Some("保密域来源密码验证通过。".into());
            }
            (crate::provision::KeyDomainRole::Share, Err(message)) => {
                self.provision.form.share_source_knowledge =
                    crate::provision::SourcePasswordKnowledge::Unknown;
                self.provision.message = Some(message);
            }
            (crate::provision::KeyDomainRole::Encrypt, Err(message)) => {
                self.provision.form.encrypt_source_knowledge =
                    crate::provision::SourcePasswordKnowledge::Unknown;
                self.provision.message = Some(message);
            }
        }
    }

    pub fn provision_set_planning(&mut self) {
        self.provision.stage = ProvisionStage::Planning;
        self.input_mode = InputMode::Normal;
        self.provision.message = Some("正在只读检查目标并生成精确制盘计划…".into());
    }

    pub fn provision_finish_plan(&mut self, result: Result<ProvisionPrepared, String>) {
        match result {
            Ok(prepared) => {
                if let ProvisionPrepared::Official(official) = &prepared {
                    if let Some(target_plan) = &official.target_plan {
                        for part in &target_plan.partitions {
                            match crate::provision::KeyDomainRole::from_partition_role(
                                part.geometry.role,
                            ) {
                                Some(crate::provision::KeyDomainRole::Share) => {
                                    if let Some(knowledge) = part.source_password_knowledge {
                                        self.provision.form.share_source_knowledge = knowledge;
                                    }
                                }
                                Some(crate::provision::KeyDomainRole::Encrypt) => {
                                    if let Some(knowledge) = part.source_password_knowledge {
                                        self.provision.form.encrypt_source_knowledge = knowledge;
                                    }
                                }
                                None => {}
                            }
                        }
                    }
                }
                self.provision.prepared = Some(prepared);
                self.provision.stage = ProvisionStage::Review;
                self.provision.pane_focus = crate::tui::pane::PaneFocus::provision_review();
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
