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

    pub(crate) fn provision_total_sectors(&self) -> Option<u64> {
        self.selected_device()
            .and_then(|row| row.layout_geometry().ok())
            .map(|geometry| geometry.native_sector_count)
    }

    pub(super) fn provision_field_descriptor(
        &self,
        display_index: usize,
    ) -> Option<ProvisionFieldDescriptor> {
        self.provision_field_descriptors()
            .get(display_index)
            .copied()
    }

    pub(crate) fn provision_field_id(&self, display_index: usize) -> Option<ProvisionFieldId> {
        self.provision_field_descriptor(display_index)
            .map(|descriptor| descriptor.id)
    }

    pub(super) fn provision_field_descriptors(&self) -> Vec<ProvisionFieldDescriptor> {
        let descriptor =
            |id, section, editable, secret, toggle, fill_capacity| ProvisionFieldDescriptor {
                id,
                section,
                capabilities: ProvisionFieldCapabilities {
                    editable,
                    secret,
                    toggle,
                    fill_capacity,
                },
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
                        true,
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
                    ),
                ]);
            }
            return fields;
        }
        let Some(mode) = self.provision.kind.mode() else {
            return Vec::new();
        };
        let mut fields = Vec::with_capacity(48);
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
                matches!(id, ProvisionFieldId::LabelId),
                false,
            ));
        }
        fields.push(descriptor(
            ProvisionFieldId::AdvancedSection,
            ProvisionFieldSection::Identity,
            false,
            false,
            false,
            false,
        ));
        if self.provision.advanced_identity_open {
            for field in Lba8IdentityField::ALL {
                fields.push(descriptor(
                    ProvisionFieldId::Lba8Identity(field),
                    ProvisionFieldSection::AdvancedIdentity,
                    true,
                    false,
                    false,
                    false,
                ));
            }
        }
        if matches!(mode, 0 | 1 | 3) {
            let domain = crate::provision::KeyDomainRole::Share;
            let source_password_editable = self.provision_source_has_password_domain(domain);
            fields.push(descriptor(
                ProvisionFieldId::SourcePassword(domain),
                ProvisionFieldSection::PasswordDomain,
                source_password_editable,
                true,
                false,
                false,
            ));
            fields.push(descriptor(
                ProvisionFieldId::TargetPassword(domain),
                ProvisionFieldSection::PasswordDomain,
                true,
                true,
                source_password_editable,
                false,
            ));
        }
        if matches!(mode, 0..=2) {
            let domain = crate::provision::KeyDomainRole::Encrypt;
            let source_password_editable = self.provision_source_has_password_domain(domain);
            fields.push(descriptor(
                ProvisionFieldId::SourcePassword(domain),
                ProvisionFieldSection::PasswordDomain,
                source_password_editable,
                true,
                false,
                false,
            ));
            fields.push(descriptor(
                ProvisionFieldId::TargetPassword(domain),
                ProvisionFieldSection::PasswordDomain,
                true,
                true,
                source_password_editable,
                false,
            ));
        }
        fields.push(descriptor(
            ProvisionFieldId::EncryptionAlgorithm,
            ProvisionFieldSection::PasswordDomain,
            false,
            false,
            true,
            false,
        ));
        if matches!(mode, 0 | 3) {
            for id in [
                ProvisionFieldId::StartLba(crate::provision::PartitionRole::Boot),
                ProvisionFieldId::Capacity(crate::provision::PartitionRole::Boot),
            ] {
                fields.push(descriptor(
                    id,
                    ProvisionFieldSection::PartitionLayout,
                    true,
                    false,
                    matches!(id, ProvisionFieldId::Capacity(_)),
                    matches!(
                        id,
                        ProvisionFieldId::Capacity(_) | ProvisionFieldId::StartLba(_)
                    ),
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
                ProvisionFieldId::StartLba(role),
                ProvisionFieldId::Capacity(role),
            ] {
                fields.push(descriptor(
                    id,
                    ProvisionFieldSection::PartitionLayout,
                    true,
                    false,
                    matches!(id, ProvisionFieldId::Capacity(_)),
                    matches!(
                        id,
                        ProvisionFieldId::Capacity(_) | ProvisionFieldId::StartLba(_)
                    ),
                ));
            }
        }
        if matches!(mode, 0..=2) {
            for id in [
                ProvisionFieldId::StartLba(crate::provision::PartitionRole::Encrypt),
                ProvisionFieldId::Capacity(crate::provision::PartitionRole::Encrypt),
            ] {
                fields.push(descriptor(
                    id,
                    ProvisionFieldSection::PartitionLayout,
                    true,
                    false,
                    matches!(id, ProvisionFieldId::Capacity(_)),
                    matches!(
                        id,
                        ProvisionFieldId::Capacity(_) | ProvisionFieldId::StartLba(_)
                    ),
                ));
            }
        }
        let preflight = self.provision_preflight().ok();
        for target in self.provision_format_template() {
            let disposition = preflight
                .as_ref()
                .and_then(|value| value.format_disposition(target.role))
                .unwrap_or_else(|| {
                    if self.provision_explicit_format_selected(target.role) {
                        preflight::ProvisionFormatDisposition::UserRequestedRebuild
                    } else {
                        preflight::ProvisionFormatDisposition::Preserve
                    }
                });
            let format_selected = disposition.selected();
            fields.push(descriptor(
                ProvisionFieldId::FormatEnabled(target.role),
                ProvisionFieldSection::Formatting,
                false,
                false,
                disposition.toggle_allowed(),
                false,
            ));
            fields.push(descriptor(
                ProvisionFieldId::Filesystem(target.role),
                ProvisionFieldSection::Formatting,
                false,
                false,
                format_selected,
                false,
            ));
            fields.push(descriptor(
                ProvisionFieldId::VolumeLabel(target.role),
                ProvisionFieldSection::Formatting,
                format_selected,
                false,
                false,
                false,
            ));
        }
        for id in [
            ProvisionFieldId::ForceChangePassword,
            ProvisionFieldId::CancelPasswordComplexityCheck,
        ] {
            fields.push(descriptor(
                id,
                ProvisionFieldSection::PasswordPolicy,
                false,
                false,
                true,
                false,
            ));
        }
        for domain in [
            crate::provision::KeyDomainRole::Share,
            crate::provision::KeyDomainRole::Encrypt,
        ] {
            if self.provision.kind.disk_kind().has_key_domain(domain) {
                fields.push(descriptor(
                    ProvisionFieldId::MaxPasswordErrors(domain),
                    ProvisionFieldSection::PasswordPolicy,
                    true,
                    false,
                    false,
                    false,
                ));
            }
        }
        fields
    }

    // Fixed protocol regions remain in the layout and plan, outside form controls.
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
                boot: self
                    .provision_resolved_prefill()
                    .ok()
                    .and_then(|(prefill, _)| {
                        Some((prefill.boot_start_lba?, prefill.boot?.sectors()))
                    })
                    .and_then(|(start, sectors)| {
                        self.provision
                            .form
                            .effective_boot_filesystem(start, sectors)
                            .ok()
                    })
                    .unwrap_or(self.provision.form.boot_fs),
                share: self.provision.form.share_fs,
                encrypt: self.provision.form.encrypt_fs,
            },
        )
        .unwrap_or_default()
        .into_iter()
        .filter(|target| target.format_capable)
        .collect()
    }
}
