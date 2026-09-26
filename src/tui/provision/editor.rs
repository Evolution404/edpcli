use super::*;

impl AppState {
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
}
