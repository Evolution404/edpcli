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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ProvisionPasswordVerificationState {
    #[default]
    Idle,
    Verifying,
    Failed,
}

#[path = "lba8_identity_field.rs"]
mod lba8_identity_field;
pub(crate) use lba8_identity_field::Lba8IdentityField;
#[path = "field_model.rs"]
mod field_model;
pub(crate) use field_model::{
    PlainProvisionFieldKind, ProvisionFieldCapabilities, ProvisionFieldDescriptor,
    ProvisionFieldId, ProvisionFieldSection, ProvisionRegionFocus,
};

#[path = "result_model.rs"]
mod result_model;
pub use result_model::{ProvisionResultPartition, ProvisionResultSnapshot};
#[path = "result_geometry.rs"]
mod result_geometry;
#[path = "result_interaction.rs"]
mod result_interaction;

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

    pub const fn disk_kind(self) -> crate::provision::DiskProvisionKind {
        match self {
            Self::Mode0 => crate::provision::DiskProvisionKind::Mode0,
            Self::Mode1 => crate::provision::DiskProvisionKind::Mode1,
            Self::Mode2 => crate::provision::DiskProvisionKind::Mode2,
            Self::Mode3 => crate::provision::DiskProvisionKind::Mode3,
            Self::Plain => crate::provision::DiskProvisionKind::Plain,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionStage {
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

#[path = "advanced_identity.rs"]
mod advanced_identity;
#[path = "editor.rs"]
mod editor;
#[path = "execution_state.rs"]
mod execution_state;
#[path = "field_input.rs"]
mod field_input;
#[path = "field_layout.rs"]
mod field_layout;
#[path = "field_presentation.rs"]
mod field_presentation;
#[path = "fields.rs"]
mod fields;
#[path = "fields_access.rs"]
mod fields_access;
#[path = "form.rs"]
mod form;
#[path = "insert_mode.rs"]
mod insert_mode;
#[path = "key_domains.rs"]
mod key_domains;
#[path = "option_editor.rs"]
mod option_editor;
#[path = "plan_completion.rs"]
mod plan_completion;
#[path = "transitions.rs"]
mod transitions;
use transitions::{ProvisionFormViewSnapshot, ProvisionReviewViewSnapshot};
#[path = "layout.rs"]
mod layout;
#[path = "layout_presentation.rs"]
mod layout_presentation;
#[path = "navigation_model.rs"]
mod navigation_model;
pub use navigation_model::ProvisionSurface;
#[path = "pane.rs"]
mod pane;
#[path = "password_model.rs"]
mod password_model;
#[path = "password_target_state.rs"]
mod password_target_state;
#[path = "password_verification.rs"]
mod password_verification;
#[path = "plain_editor.rs"]
mod plain_editor;
#[path = "preflight.rs"]
mod preflight;
#[cfg(test)]
#[path = "preflight_tests.rs"]
mod preflight_tests;
#[path = "review.rs"]
mod review;
#[path = "review_region_projection.rs"]
mod review_region_projection;
#[path = "run.rs"]
mod run;
#[path = "scheme_picker_state.rs"]
mod scheme_picker_state;
#[path = "source_password_state.rs"]
mod source_password_state;
pub(crate) use review::{
    ProvisionConfirmationAction, ProvisionConfirmationDataEffect,
    ProvisionConfirmationFilesystemEffect, ProvisionConfirmationPasswordEffect,
    ProvisionConfirmationRegion, ProvisionConfirmationViewModel,
};
#[path = "validation.rs"]
mod validation;

use form::{shift_supported_fs, toggle_supported_fs, ProvisionInputPolicy};
pub use form::{PlainPartitionForm, PlainProvisionForm, ProvisionForm};

#[derive(Debug, Clone)]
pub struct ProvisionState {
    pub stage: ProvisionStage,
    pub kind: ProvisionKind,
    pub scheme_selected: usize,
    pub scheme_picker_open: bool,
    pub field_selected: usize,
    pub field_cursor: usize,
    pub advanced_identity_open: bool,
    pub(crate) source_password_edit_dirty: bool,
    pub(crate) share_source_password_revision: u64,
    pub(crate) encrypt_source_password_revision: u64,
    pub(crate) share_source_verification: ProvisionPasswordVerificationState,
    pub(crate) encrypt_source_verification: ProvisionPasswordVerificationState,
    pub(super) target_password_modes: password_verification::TargetPasswordModeState,
    pub form: ProvisionForm,
    pub plain_form: PlainProvisionForm,
    pub prepared: Option<ProvisionPrepared>,
    pub confirmation: String,
    pub export_path: String,
    pub message: Option<crate::tui::ui::UiMessage>,
    pub result_status: Option<crate::application::provision::ProvisionExecutionStatus>,
    pub result_outcome: Option<crate::application::provision::ProvisionWriteOutcome>,
    pub result_plan: Option<ProvisionResultSnapshot>,
    pub result_workbench: crate::tui::result_workbench::ResultWorkbenchState,
    pub run: Option<crate::application::progress::OperationRunState>,
    pub pane_focus: crate::tui::pane::PaneFocus,
    pub review_region_selected: usize,
    pub review_details_expanded: bool,
    form_view_snapshot: Option<ProvisionFormViewSnapshot>,
    review_view_snapshot: Option<ProvisionReviewViewSnapshot>,
    pub(super) target_disk: Option<u32>,
    form_initialized_for: Option<(u32, u64, Option<String>, ProvisionKind)>,
}

impl Default for ProvisionState {
    fn default() -> Self {
        Self {
            stage: ProvisionStage::Form,
            kind: ProvisionKind::Mode0,
            scheme_selected: 0,
            scheme_picker_open: false,
            field_selected: 0,
            field_cursor: 0,
            advanced_identity_open: false,
            source_password_edit_dirty: false,
            share_source_password_revision: 0,
            encrypt_source_password_revision: 0,
            share_source_verification: ProvisionPasswordVerificationState::Idle,
            encrypt_source_verification: ProvisionPasswordVerificationState::Idle,
            target_password_modes: password_verification::TargetPasswordModeState::default(),
            form: ProvisionForm::default(),
            plain_form: PlainProvisionForm::default(),
            prepared: None,
            confirmation: String::new(),
            export_path: String::new(),
            message: None,
            result_status: None,
            result_outcome: None,
            result_plan: None,
            result_workbench: crate::tui::result_workbench::ResultWorkbenchState::default(),
            run: None,
            pane_focus: crate::tui::pane::PaneFocus::provision_form(),
            review_region_selected: 0,
            review_details_expanded: false,
            form_view_snapshot: None,
            review_view_snapshot: None,
            target_disk: None,
            form_initialized_for: None,
        }
    }
}

impl AppState {
    pub fn provision_mut(&mut self) -> &mut ProvisionState {
        &mut self.provision
    }

    pub const fn provision_target_disk(&self) -> Option<u32> {
        self.provision.target_disk
    }

    pub fn provision_reset(&mut self) {
        self.shell.input_mode = InputMode::Normal;
        let selected = self
            .provision
            .scheme_selected
            .min(ProvisionKind::ALL.len() - 1);
        let target_disk = self.provision.target_disk;
        self.provision = ProvisionState::default();
        self.provision.scheme_selected = selected;
        self.provision.kind = ProvisionKind::ALL[selected];
        self.provision.target_disk = target_disk;
        self.shell.pinned_disk = target_disk;
        if self.shell.workspace == Workspace::Provision {
            self.provision_transition_enter_form();
            self.provision.scheme_picker_open = false;
            self.set_item_count(0);
        }
    }

    pub fn provision_begin_selected(&mut self) -> ProvisionKind {
        let index = self
            .provision
            .scheme_selected
            .min(ProvisionKind::ALL.len() - 1);
        let kind = ProvisionKind::ALL[index];
        if self.selected_device().is_none() {
            self.provision.message = Some(crate::tui::ui::UiMessage::error(
                "目标 USB 已不存在，请返回设备列表重新选择。",
            ));
            return kind;
        }
        self.provision.scheme_selected = index;
        self.provision.scheme_picker_open = false;
        self.provision.kind = kind;
        self.provision.field_selected = 0;
        self.provision.field_cursor = 0;
        self.provision.confirmation.clear();
        self.provision.message = None;
        self.provision.prepared = None;
        self.provision.result_status = None;
        self.provision.result_outcome = None;
        self.provision.result_plan = None;
        self.provision.result_workbench =
            crate::tui::result_workbench::ResultWorkbenchState::default();
        self.provision.pane_focus = crate::tui::pane::PaneFocus::provision_form();
        self.provision.review_region_selected = 0;
        self.provision.review_details_expanded = false;
        let current_target = self
            .selected_device()
            .map(|row| (row.disk, row.size, row.device_id.clone(), kind));
        if self.provision.form_initialized_for == current_target {
            self.provision_transition_enter_form();
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
                    self.provision_transition_enter_form();
                    self.provision.message = None;
                    self.provision_sync_cursor_to_end();
                }
                Err(message) => {
                    self.provision.plain_form = PlainProvisionForm::default();
                    self.provision_transition_enter_form();
                    self.provision.message = Some(crate::tui::ui::UiMessage::error(message));
                }
            }
            return kind;
        }
        self.provision.form = ProvisionForm::default();
        self.provision_initialize_password_candidates(kind);
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
        if let Some(prefill) = self.provision_initial_prefill(kind) {
            self.provision.form.apply_prefill(&prefill);
        }
        self.provision.form_initialized_for = current_target;
        self.provision_transition_enter_form();
        self.provision_sync_cursor_to_end();
        kind
    }
}
