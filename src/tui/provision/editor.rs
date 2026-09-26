use super::*;

impl AppState {
    pub fn provision_fill_selected_capacity(&mut self) -> bool {
        let Some(descriptor) = self.provision_field_descriptor(self.provision.field_selected)
        else {
            return false;
        };
        if !descriptor.capabilities.fill_capacity {
            return false;
        }
        let id = descriptor.id;
        if let ProvisionFieldId::Plain {
            partition,
            kind: PlainProvisionFieldKind::Capacity,
        } = id
        {
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
        let ProvisionFieldId::Capacity(role) = id else {
            return false;
        };
        // The selected capacity itself does not determine its upper boundary. Use a
        // one-sector placeholder so f can recover even after the user clears
        // or partially edits the current capacity field. All other form values
        // remain subject to normal strict geometry validation.
        let original_form = self.provision.form.clone();
        match role {
            crate::provision::PartitionRole::Boot => {
                self.provision.form.boot_input_mode = crate::provision::CapacityInputMode::Exact;
                self.provision.form.boot_sectors = "1".into();
                self.provision.form.boot_capacity_edited = true;
            }
            crate::provision::PartitionRole::Share
            | crate::provision::PartitionRole::BootShareCombined => {
                self.provision.form.share_input_mode = crate::provision::CapacityInputMode::Exact;
                self.provision.form.share_sectors = "1".into();
                self.provision.form.share_capacity_edited = true;
            }
            crate::provision::PartitionRole::Encrypt => {
                self.provision.form.encrypt_input_mode = crate::provision::CapacityInputMode::Exact;
                self.provision.form.encrypt_sectors = "1".into();
                self.provision.form.encrypt_capacity_edited = true;
            }
            crate::provision::PartitionRole::CompatibilityReserve => return false,
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
        let (unit, quick, exact, edited, source) = match role {
            crate::provision::PartitionRole::Boot => (
                self.provision.form.boot_quick_unit,
                &mut self.provision.form.boot_mib,
                &mut self.provision.form.boot_sectors,
                &mut self.provision.form.boot_capacity_edited,
                &mut self.provision.form.boot_capacity_source,
            ),
            crate::provision::PartitionRole::Share
            | crate::provision::PartitionRole::BootShareCombined => (
                self.provision.form.share_quick_unit,
                &mut self.provision.form.share_mib,
                &mut self.provision.form.share_sectors,
                &mut self.provision.form.share_capacity_edited,
                &mut self.provision.form.share_capacity_source,
            ),
            crate::provision::PartitionRole::Encrypt => (
                self.provision.form.encrypt_quick_unit,
                &mut self.provision.form.encrypt_mib,
                &mut self.provision.form.encrypt_sectors,
                &mut self.provision.form.encrypt_capacity_edited,
                &mut self.provision.form.encrypt_capacity_source,
            ),
            crate::provision::PartitionRole::CompatibilityReserve => return false,
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
        let partition = match self.provision_field_id(self.provision.field_selected) {
            Some(ProvisionFieldId::Plain { partition, .. }) => Some(partition),
            _ => None,
        };
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
        if self.provision_field_id(self.provision.field_selected)
            != Some(ProvisionFieldId::ForceChangePassword)
        {
            return false;
        }
        self.provision.form.force_change_password = !self.provision.form.force_change_password;
        self.provision.message = None;
        true
    }
}
