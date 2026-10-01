use super::*;

impl AppState {
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
        self.provision.review_projection = None;
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
