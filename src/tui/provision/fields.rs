use super::*;

impl AppState {
    pub fn provision_field_count(&self) -> usize {
        self.provision_field_descriptors().len()
    }

    pub fn provision_move_field(&mut self, delta: isize) {
        let count = self.provision_field_count();
        if count == 0 {
            return;
        }
        self.provision.field_selected = if delta < 0 {
            self.provision
                .field_selected
                .saturating_sub(delta.unsigned_abs())
        } else {
            (self.provision.field_selected + delta as usize).min(count - 1)
        };
        self.provision_sync_cursor_to_end();
    }

    pub(super) fn provision_total_sectors(&self) -> Option<u64> {
        self.selected_device()
            .map(|row| row.size / crate::common::SECTOR as u64)
    }

    pub(super) fn provision_field_descriptor(
        &self,
        display_index: usize,
    ) -> Option<ProvisionFieldDescriptor> {
        self.provision_field_descriptors()
            .get(display_index)
            .copied()
    }

    pub(super) fn provision_field_id(&self, display_index: usize) -> Option<ProvisionFieldId> {
        self.provision_field_descriptor(display_index)
            .map(|descriptor| descriptor.id)
    }

    fn provision_field_descriptors(&self) -> Vec<ProvisionFieldDescriptor> {
        let descriptor =
            |id, section, editable, secret, toggle, fill_capacity, verify_source_password| {
                ProvisionFieldDescriptor {
                    id,
                    section,
                    capabilities: ProvisionFieldCapabilities {
                        editable,
                        secret,
                        toggle,
                        fill_capacity,
                        verify_source_password,
                    },
                }
            };
        if self.provision.kind == ProvisionKind::Plain {
            let mut fields = Vec::with_capacity(self.provision.plain_form.partitions.len() * 4);
            for partition in 0..self.provision.plain_form.partitions.len() {
                let section = ProvisionFieldSection::PlainPartition(partition);
                fields.extend([
                    descriptor(
                        ProvisionFieldId::Plain {
                            partition,
                            kind: PlainProvisionFieldKind::StartLba,
                        },
                        section,
                        true,
                        false,
                        false,
                        false,
                        false,
                    ),
                    descriptor(
                        ProvisionFieldId::Plain {
                            partition,
                            kind: PlainProvisionFieldKind::Capacity,
                        },
                        section,
                        true,
                        false,
                        true,
                        true,
                        false,
                    ),
                    descriptor(
                        ProvisionFieldId::Plain {
                            partition,
                            kind: PlainProvisionFieldKind::Filesystem,
                        },
                        section,
                        false,
                        false,
                        true,
                        false,
                        false,
                    ),
                    descriptor(
                        ProvisionFieldId::Plain {
                            partition,
                            kind: PlainProvisionFieldKind::VolumeLabel,
                        },
                        section,
                        true,
                        false,
                        false,
                        false,
                        false,
                    ),
                ]);
            }
            return fields;
        }
        let Some(mode) = self.provision.kind.mode() else {
            return Vec::new();
        };
        let mut fields = Vec::with_capacity(34);
        for id in [
            ProvisionFieldId::LabelId,
            ProvisionFieldId::User,
            ProvisionFieldId::Department,
            ProvisionFieldId::Safe6Label,
        ] {
            fields.push(descriptor(
                id,
                ProvisionFieldSection::Identity,
                true,
                false,
                false,
                false,
                false,
            ));
        }
        if matches!(mode, 0 | 1 | 3) {
            let domain = crate::provision::KeyDomainRole::Share;
            let opaque = self.provision_domain_opaque_candidate(domain);
            fields.push(descriptor(
                ProvisionFieldId::SourcePassword(domain),
                ProvisionFieldSection::PasswordDomain,
                true,
                true,
                false,
                false,
                true,
            ));
            fields.push(descriptor(
                ProvisionFieldId::TargetPassword(domain),
                ProvisionFieldSection::PasswordDomain,
                !opaque,
                !opaque,
                false,
                false,
                false,
            ));
        }
        if matches!(mode, 0..=2) {
            let domain = crate::provision::KeyDomainRole::Encrypt;
            let opaque = self.provision_domain_opaque_candidate(domain);
            fields.push(descriptor(
                ProvisionFieldId::SourcePassword(domain),
                ProvisionFieldSection::PasswordDomain,
                true,
                true,
                false,
                false,
                true,
            ));
            fields.push(descriptor(
                ProvisionFieldId::TargetPassword(domain),
                ProvisionFieldSection::PasswordDomain,
                !opaque,
                !opaque,
                false,
                false,
                false,
            ));
        }
        if matches!(mode, 0 | 3) {
            for id in [
                ProvisionFieldId::Capacity(crate::provision::PartitionRole::Boot),
                ProvisionFieldId::StartLba(crate::provision::PartitionRole::Boot),
            ] {
                fields.push(descriptor(
                    id,
                    ProvisionFieldSection::PartitionLayout,
                    true,
                    false,
                    matches!(id, ProvisionFieldId::Capacity(_)),
                    matches!(id, ProvisionFieldId::Capacity(_)),
                    false,
                ));
            }
        }
        if matches!(mode, 0 | 1 | 3) {
            let role = if mode == 1 {
                crate::provision::PartitionRole::BootShareCombined
            } else {
                crate::provision::PartitionRole::Share
            };
            for id in [
                ProvisionFieldId::Capacity(role),
                ProvisionFieldId::StartLba(role),
            ] {
                fields.push(descriptor(
                    id,
                    ProvisionFieldSection::PartitionLayout,
                    true,
                    false,
                    matches!(id, ProvisionFieldId::Capacity(_)),
                    matches!(id, ProvisionFieldId::Capacity(_)),
                    false,
                ));
            }
        }
        if matches!(mode, 0..=2) {
            for id in [
                ProvisionFieldId::Capacity(crate::provision::PartitionRole::Encrypt),
                ProvisionFieldId::StartLba(crate::provision::PartitionRole::Encrypt),
            ] {
                fields.push(descriptor(
                    id,
                    ProvisionFieldSection::PartitionLayout,
                    true,
                    false,
                    matches!(id, ProvisionFieldId::Capacity(_)),
                    matches!(id, ProvisionFieldId::Capacity(_)),
                    false,
                ));
            }
        }
        for target in self.provision_format_template() {
            fields.push(descriptor(
                ProvisionFieldId::FormatEnabled(target.role),
                ProvisionFieldSection::Formatting,
                false,
                false,
                target.format_capable,
                false,
                false,
            ));
            if target.format_capable {
                fields.push(descriptor(
                    ProvisionFieldId::Filesystem(target.role),
                    ProvisionFieldSection::Formatting,
                    false,
                    false,
                    true,
                    false,
                    false,
                ));
                fields.push(descriptor(
                    ProvisionFieldId::VolumeLabel(target.role),
                    ProvisionFieldSection::Formatting,
                    true,
                    false,
                    false,
                    false,
                    false,
                ));
            }
        }
        for id in [
            ProvisionFieldId::ForceChangePassword,
            ProvisionFieldId::CancelPasswordComplexityCheck,
            ProvisionFieldId::MaxPasswordErrors(crate::provision::KeyDomainRole::Share),
            ProvisionFieldId::MaxPasswordErrors(crate::provision::KeyDomainRole::Encrypt),
        ] {
            fields.push(descriptor(
                id,
                ProvisionFieldSection::PasswordPolicy,
                matches!(id, ProvisionFieldId::MaxPasswordErrors(_)),
                false,
                matches!(
                    id,
                    ProvisionFieldId::ForceChangePassword
                        | ProvisionFieldId::CancelPasswordComplexityCheck
                ),
                false,
                false,
            ));
        }
        fields
    }

    pub(super) fn provision_format_template(&self) -> Vec<crate::provision::PartitionFormatTarget> {
        let Some(mode) = self.provision.kind.mode() else {
            return Vec::new();
        };
        let mode = match mode {
            0 => crate::provision::OfficialPartitionMode::DefaultThreePartition,
            1 => crate::provision::OfficialPartitionMode::BootShareCombined,
            2 => crate::provision::OfficialPartitionMode::WholeDiskEncrypted,
            _ => crate::provision::OfficialPartitionMode::IntranetExtranetDualPartition,
        };
        crate::provision::official_format_targets_with_filesystems(
            mode,
            crate::provision::OfficialPartitionSizes::new(32, 64, 128),
            512,
            crate::provision::OfficialPartitionFilesystems {
                boot: self.provision.form.boot_fs,
                share: self.provision.form.share_fs,
                encrypt: self.provision.form.encrypt_fs,
            },
        )
        .unwrap_or_default()
    }
}
