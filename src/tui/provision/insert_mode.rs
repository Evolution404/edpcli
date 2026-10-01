use super::*;

impl AppState {
    pub fn provision_begin_insert(&mut self) -> bool {
        if self.shell.workspace != Workspace::Provision
            || self.provision.stage != ProvisionStage::Form
            || !self.provision_selected_field_is_editable()
        {
            return false;
        }
        if let Some(ProvisionFieldId::TargetPassword(domain)) =
            self.provision_field_id(self.provision.field_selected)
        {
            self.provision_prepare_target_password_edit(domain);
        }
        self.shell.input_mode = InputMode::Insert;
        self.provision.source_password_edit_dirty = false;
        self.provision_sync_cursor_to_end();
        true
    }

    pub fn provision_end_insert(&mut self) -> bool {
        if self.shell.input_mode != InputMode::Insert {
            return false;
        }
        let selected_id = self.provision_field_id(self.provision.field_selected);
        let source_password_dirty = self.provision.source_password_edit_dirty
            && matches!(selected_id, Some(ProvisionFieldId::SourcePassword(_)));
        self.shell.input_mode = InputMode::Normal;
        self.provision.source_password_edit_dirty = false;
        if let Some(ProvisionFieldId::TargetPassword(domain)) = selected_id {
            self.provision_normalize_target_password_mode(domain);
        }
        source_password_dirty
    }

    pub(super) fn provision_sync_cursor_to_end(&mut self) {
        self.provision.field_cursor = self
            .provision_selected_field()
            .map(|value| value.chars().count())
            .unwrap_or(0);
    }
}
