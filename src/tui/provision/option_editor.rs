use super::*;

impl AppState {
    pub fn provision_toggle_selected_option(&mut self) -> bool {
        let descriptor = self.provision_field_descriptor(self.provision.field_selected);
        if descriptor.is_some_and(|descriptor| !descriptor.capabilities.toggle) {
            return false;
        }
        let selected_id = descriptor.map(|descriptor| descriptor.id);
        if let Some(ProvisionFieldId::Plain { partition, kind }) = selected_id {
            let result = self
                .provision
                .plain_form
                .toggle_partition_option(partition, kind);
            match result {
                Ok(true) if kind == PlainProvisionFieldKind::Capacity => {
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
                    if kind == PlainProvisionFieldKind::Capacity {
                        self.provision_sync_cursor_to_end();
                    }
                    return true;
                }
            }
        }
        match selected_id {
            Some(ProvisionFieldId::Capacity(role)) => {
                match self.provision.form.toggle_capacity_input(role) {
                    Ok(()) => {
                        self.provision.message = None;
                        self.provision_sync_cursor_to_end();
                    }
                    Err(message) => self.provision.message = Some(message),
                }
                true
            }
            Some(ProvisionFieldId::ForceChangePassword) => {
                self.provision.form.force_change_password =
                    !self.provision.form.force_change_password;
                self.provision.message = None;
                true
            }
            Some(ProvisionFieldId::CancelPasswordComplexityCheck) => {
                self.provision.form.cancel_password_complexity_check =
                    !self.provision.form.cancel_password_complexity_check;
                self.provision.message = None;
                true
            }
            Some(ProvisionFieldId::FormatEnabled(role)) => {
                match role {
                    crate::provision::PartitionRole::Boot => {
                        self.provision.form.format_boot = !self.provision.form.format_boot;
                    }
                    crate::provision::PartitionRole::Share
                    | crate::provision::PartitionRole::BootShareCombined => {
                        self.provision.form.format_share = !self.provision.form.format_share;
                    }
                    crate::provision::PartitionRole::Encrypt => {
                        self.provision.form.format_encrypt = !self.provision.form.format_encrypt;
                    }
                    crate::provision::PartitionRole::CompatibilityReserve => {}
                }
                true
            }
            Some(ProvisionFieldId::Filesystem(role)) => {
                match role {
                    crate::provision::PartitionRole::Boot => {
                        self.provision.form.boot_fs =
                            toggle_supported_fs(self.provision.form.boot_fs);
                    }
                    crate::provision::PartitionRole::Share
                    | crate::provision::PartitionRole::BootShareCombined => {
                        self.provision.form.share_fs =
                            toggle_supported_fs(self.provision.form.share_fs);
                    }
                    crate::provision::PartitionRole::Encrypt => {
                        self.provision.form.encrypt_fs =
                            toggle_supported_fs(self.provision.form.encrypt_fs);
                    }
                    crate::provision::PartitionRole::CompatibilityReserve => return false,
                }
                true
            }
            _ => false,
        }
    }

    pub fn provision_shift_selected_option(&mut self, reverse: bool) -> bool {
        let descriptor = self.provision_field_descriptor(self.provision.field_selected);
        if descriptor.is_some_and(|descriptor| !descriptor.capabilities.toggle) {
            return false;
        }
        let selected_id = descriptor.map(|descriptor| descriptor.id);
        if let Some(ProvisionFieldId::Plain { partition, kind }) = selected_id {
            let result = self
                .provision
                .plain_form
                .shift_partition_option(partition, kind, reverse);
            match result {
                Ok(true) if kind == PlainProvisionFieldKind::Capacity => {
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
                    if kind == PlainProvisionFieldKind::Capacity {
                        self.provision_sync_cursor_to_end();
                    }
                    return true;
                }
            }
        }
        match selected_id {
            Some(ProvisionFieldId::Capacity(role)) => {
                match self.provision.form.shift_capacity_input(role, reverse) {
                    Ok(()) => {
                        self.provision.message = None;
                        self.provision_sync_cursor_to_end();
                    }
                    Err(message) => self.provision.message = Some(message),
                }
                true
            }
            Some(ProvisionFieldId::Filesystem(role)) => {
                match role {
                    crate::provision::PartitionRole::Boot => {
                        self.provision.form.boot_fs =
                            shift_supported_fs(self.provision.form.boot_fs, reverse);
                    }
                    crate::provision::PartitionRole::Share
                    | crate::provision::PartitionRole::BootShareCombined => {
                        self.provision.form.share_fs =
                            shift_supported_fs(self.provision.form.share_fs, reverse);
                    }
                    crate::provision::PartitionRole::Encrypt => {
                        self.provision.form.encrypt_fs =
                            shift_supported_fs(self.provision.form.encrypt_fs, reverse);
                    }
                    crate::provision::PartitionRole::CompatibilityReserve => return false,
                }
                self.provision.message = None;
                true
            }
            Some(
                ProvisionFieldId::ForceChangePassword
                | ProvisionFieldId::CancelPasswordComplexityCheck
                | ProvisionFieldId::FormatEnabled(_),
            ) => self.provision_toggle_selected_option(),
            _ => false,
        }
    }
}
