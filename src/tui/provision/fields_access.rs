use super::*;

impl AppState {
    pub(super) fn provision_selected_field_mut(&mut self) -> Option<&mut String> {
        let id = self.provision_field_id(self.provision.field_selected)?;
        if let ProvisionFieldId::Plain { partition, kind } = id {
            let part = self.provision.plain_form.partitions.get_mut(partition)?;
            return match kind {
                PlainProvisionFieldKind::StartLba => Some(&mut part.start_lba),
                PlainProvisionFieldKind::Capacity => Some(
                    if part.input_mode == crate::provision::CapacityInputMode::Exact {
                        &mut part.sector_count
                    } else {
                        &mut part.quick_capacity
                    },
                ),
                PlainProvisionFieldKind::Filesystem => None,
                PlainProvisionFieldKind::VolumeLabel => Some(&mut part.volume_label),
            };
        }
        let share_opaque =
            self.provision_domain_opaque_candidate(crate::provision::KeyDomainRole::Share);
        let encrypt_opaque =
            self.provision_domain_opaque_candidate(crate::provision::KeyDomainRole::Encrypt);
        match id {
            ProvisionFieldId::Capacity(crate::provision::PartitionRole::Boot) => Some(
                if self.provision.form.boot_input_mode == crate::provision::CapacityInputMode::Exact
                {
                    &mut self.provision.form.boot_sectors
                } else {
                    &mut self.provision.form.boot_mib
                },
            ),
            ProvisionFieldId::Capacity(
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined,
            ) => Some(
                if self.provision.form.share_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    &mut self.provision.form.share_sectors
                } else {
                    &mut self.provision.form.share_mib
                },
            ),
            ProvisionFieldId::Capacity(crate::provision::PartitionRole::Encrypt) => Some(
                if self.provision.form.encrypt_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    &mut self.provision.form.encrypt_sectors
                } else {
                    &mut self.provision.form.encrypt_mib
                },
            ),
            ProvisionFieldId::LabelId => Some(&mut self.provision.form.label_id),
            ProvisionFieldId::User => Some(&mut self.provision.form.user),
            ProvisionFieldId::Department => Some(&mut self.provision.form.dept),
            ProvisionFieldId::Safe6Label => Some(&mut self.provision.form.label),
            ProvisionFieldId::VolumeLabel(crate::provision::PartitionRole::Boot) => {
                Some(&mut self.provision.form.volume_label)
            }
            ProvisionFieldId::VolumeLabel(
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined,
            ) => Some(&mut self.provision.form.share_label),
            ProvisionFieldId::VolumeLabel(crate::provision::PartitionRole::Encrypt) => {
                Some(&mut self.provision.form.encrypt_label)
            }
            ProvisionFieldId::StartLba(crate::provision::PartitionRole::Boot) => {
                Some(&mut self.provision.form.boot_start_lba)
            }
            ProvisionFieldId::StartLba(
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined,
            ) => Some(&mut self.provision.form.share_start_lba),
            ProvisionFieldId::StartLba(crate::provision::PartitionRole::Encrypt) => {
                Some(&mut self.provision.form.encrypt_start_lba)
            }
            ProvisionFieldId::MaxPasswordErrors(crate::provision::KeyDomainRole::Share) => {
                Some(&mut self.provision.form.max_share_password_errors)
            }
            ProvisionFieldId::MaxPasswordErrors(crate::provision::KeyDomainRole::Encrypt) => {
                Some(&mut self.provision.form.max_encrypt_password_errors)
            }
            ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Share) => {
                Some(&mut self.provision.form.share_source_password)
            }
            ProvisionFieldId::TargetPassword(crate::provision::KeyDomainRole::Share)
                if !share_opaque =>
            {
                Some(&mut self.provision.form.share_target_password)
            }
            ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Encrypt) => {
                Some(&mut self.provision.form.encrypt_source_password)
            }
            ProvisionFieldId::TargetPassword(crate::provision::KeyDomainRole::Encrypt)
                if !encrypt_opaque =>
            {
                Some(&mut self.provision.form.encrypt_target_password)
            }
            _ => None,
        }
    }

    pub(super) fn provision_selected_field(&self) -> Option<&str> {
        let id = self.provision_field_id(self.provision.field_selected)?;
        if let ProvisionFieldId::Plain { partition, kind } = id {
            let part = self.provision.plain_form.partitions.get(partition)?;
            return match kind {
                PlainProvisionFieldKind::StartLba => Some(part.start_lba.as_str()),
                PlainProvisionFieldKind::Capacity => Some(
                    if part.input_mode == crate::provision::CapacityInputMode::Exact {
                        part.sector_count.as_str()
                    } else {
                        part.quick_capacity.as_str()
                    },
                ),
                PlainProvisionFieldKind::Filesystem => None,
                PlainProvisionFieldKind::VolumeLabel => Some(part.volume_label.as_str()),
            };
        }
        let share_opaque =
            self.provision_domain_opaque_candidate(crate::provision::KeyDomainRole::Share);
        let encrypt_opaque =
            self.provision_domain_opaque_candidate(crate::provision::KeyDomainRole::Encrypt);
        match id {
            ProvisionFieldId::Capacity(crate::provision::PartitionRole::Boot) => Some(
                if self.provision.form.boot_input_mode == crate::provision::CapacityInputMode::Exact
                {
                    self.provision.form.boot_sectors.as_str()
                } else {
                    self.provision.form.boot_mib.as_str()
                },
            ),
            ProvisionFieldId::Capacity(
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined,
            ) => Some(
                if self.provision.form.share_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    self.provision.form.share_sectors.as_str()
                } else {
                    self.provision.form.share_mib.as_str()
                },
            ),
            ProvisionFieldId::Capacity(crate::provision::PartitionRole::Encrypt) => Some(
                if self.provision.form.encrypt_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    self.provision.form.encrypt_sectors.as_str()
                } else {
                    self.provision.form.encrypt_mib.as_str()
                },
            ),
            ProvisionFieldId::LabelId => Some(self.provision.form.label_id.as_str()),
            ProvisionFieldId::User => Some(self.provision.form.user.as_str()),
            ProvisionFieldId::Department => Some(self.provision.form.dept.as_str()),
            ProvisionFieldId::Safe6Label => Some(self.provision.form.label.as_str()),
            ProvisionFieldId::VolumeLabel(crate::provision::PartitionRole::Boot) => {
                Some(self.provision.form.volume_label.as_str())
            }
            ProvisionFieldId::VolumeLabel(
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined,
            ) => Some(self.provision.form.share_label.as_str()),
            ProvisionFieldId::VolumeLabel(crate::provision::PartitionRole::Encrypt) => {
                Some(self.provision.form.encrypt_label.as_str())
            }
            ProvisionFieldId::StartLba(crate::provision::PartitionRole::Boot) => {
                Some(self.provision.form.boot_start_lba.as_str())
            }
            ProvisionFieldId::StartLba(
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined,
            ) => Some(self.provision.form.share_start_lba.as_str()),
            ProvisionFieldId::StartLba(crate::provision::PartitionRole::Encrypt) => {
                Some(self.provision.form.encrypt_start_lba.as_str())
            }
            ProvisionFieldId::MaxPasswordErrors(crate::provision::KeyDomainRole::Share) => {
                Some(self.provision.form.max_share_password_errors.as_str())
            }
            ProvisionFieldId::MaxPasswordErrors(crate::provision::KeyDomainRole::Encrypt) => {
                Some(self.provision.form.max_encrypt_password_errors.as_str())
            }
            ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Share) => {
                Some(self.provision.form.share_source_password.as_str())
            }
            ProvisionFieldId::TargetPassword(crate::provision::KeyDomainRole::Share)
                if !share_opaque =>
            {
                Some(self.provision.form.share_target_password.as_str())
            }
            ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Encrypt) => {
                Some(self.provision.form.encrypt_source_password.as_str())
            }
            ProvisionFieldId::TargetPassword(crate::provision::KeyDomainRole::Encrypt)
                if !encrypt_opaque =>
            {
                Some(self.provision.form.encrypt_target_password.as_str())
            }
            _ => None,
        }
    }
}
