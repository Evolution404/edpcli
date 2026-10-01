use super::*;

impl AppState {
    pub fn provision_toggle_advanced_identity(&mut self) -> bool {
        if self.provision.kind == ProvisionKind::Plain {
            return false;
        }
        if self.provision_field_id(self.provision.field_selected)
            != Some(ProvisionFieldId::AdvancedSection)
        {
            return false;
        }
        self.provision.advanced_identity_open = !self.provision.advanced_identity_open;
        self.provision_sync_cursor_to_end();
        true
    }
}
