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
        if let ProvisionFieldId::StartLba(role) = id {
            let (resolved, _) = match self.provision_resolved_prefill() {
                Ok(value) => value,
                Err(message) => {
                    self.provision.message = Some(message);
                    return true;
                }
            };
            let parts = match resolved.draft_partitions(crate::common::SECTOR as u64) {
                Ok(parts) => parts,
                Err(message) => {
                    self.provision.message = Some(message);
                    return true;
                }
            };
            let Some(current) = parts.iter().find(|part| part.role == role) else {
                return false;
            };
            let required = current.sector_count;
            let mut occupied = parts
                .iter()
                .filter(|part| part.role != role)
                .copied()
                .collect::<Vec<_>>();
            occupied.sort_by_key(|part| part.start_lba);
            let mut cursor = crate::provision::OFFICIAL_PARTITION_START_SECTOR;
            let mut found = None;
            for part in occupied {
                if part.start_lba >= cursor && part.start_lba.saturating_sub(cursor) >= required {
                    found = Some(cursor);
                    break;
                }
                match part.end_lba() {
                    Ok(end) => cursor = cursor.max(end),
                    Err(message) => {
                        self.provision.message = Some(message);
                        return true;
                    }
                }
            }
            if found.is_none() && resolved.usable_end_lba.saturating_sub(cursor) >= required {
                found = Some(cursor);
            }
            let Some(start) = found else {
                self.provision.message = Some(format!(
                    "无法为{}找到可容纳当前容量的最小可用起点",
                    role.label()
                ));
                return true;
            };
            match role {
                crate::provision::PartitionRole::Boot => {
                    self.provision.form.boot_start_lba = start.to_string()
                }
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined => {
                    self.provision.form.share_start_lba = start.to_string()
                }
                crate::provision::PartitionRole::Encrypt => {
                    self.provision.form.encrypt_start_lba = start.to_string()
                }
                crate::provision::PartitionRole::CompatibilityReserve => return false,
            }
            self.provision.message = Some(format!(
                "{}起点已自动填入最小可用位置 LBA {}",
                role.label(),
                start
            ));
            self.provision_sync_cursor_to_end();
            return true;
        }
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
        if let ProvisionFieldId::Plain {
            partition,
            kind: PlainProvisionFieldKind::StartLba,
        } = id
        {
            let Some(total_sectors) = self.provision_total_sectors() else {
                self.provision.message = Some("目标 USB 已不存在".into());
                return true;
            };
            match self
                .provision
                .plain_form
                .fill_partition_start(total_sectors, partition)
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
        let resolved_result = self.provision_resolved_prefill();
        self.provision.form = original_form;
        let (resolved, _) = match resolved_result {
            Ok(value) => value,
            Err(message) => {
                self.provision.message = Some(message);
                return true;
            }
        };
        let parts = match resolved.draft_partitions(crate::common::SECTOR as u64) {
            Ok(parts) => parts,
            Err(message) => {
                self.provision.message = Some(message);
                return true;
            }
        };
        let Some(current) = parts.iter().find(|part| part.role == role) else {
            return false;
        };
        let start = current.start_lba;
        let mut boundary = resolved.usable_end_lba;
        for other in parts.iter().filter(|part| part.role != role) {
            let end = match other.end_lba() {
                Ok(end) => end,
                Err(message) => {
                    self.provision.message = Some(message);
                    return true;
                }
            };
            if other.start_lba <= start && end > start {
                boundary = start;
                break;
            }
            if other.start_lba > start {
                boundary = boundary.min(other.start_lba);
            }
        }
        let max_sectors = boundary.saturating_sub(start);
        if max_sectors == 0 {
            self.provision.message = Some("当前起点没有可用连续空间".into());
            return true;
        }

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
