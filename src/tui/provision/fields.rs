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
                true,
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
                true,
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

    pub fn provision_visible_fields(&self) -> Vec<(String, &str, bool)> {
        let mut out = Vec::new();
        if self.provision.kind == ProvisionKind::Plain {
            for (index, part) in self.provision.plain_form.partitions.iter().enumerate() {
                let number = index + 1;
                let (capacity_label, capacity_value) = match part.input_mode {
                    crate::provision::CapacityInputMode::Exact => (
                        format!("P{number} 容量 (sector)"),
                        part.sector_count.as_str(),
                    ),
                    crate::provision::CapacityInputMode::Quick => (
                        format!(
                            "P{number} 容量 ({})",
                            match part.quick_unit {
                                crate::provision::QuickCapacityUnit::MiB => "MiB",
                                crate::provision::QuickCapacityUnit::GiB => "GiB",
                            }
                        ),
                        part.quick_capacity.as_str(),
                    ),
                };
                out.push((
                    format!("P{number} 起点 LBA"),
                    part.start_lba.as_str(),
                    false,
                ));
                out.push((capacity_label, capacity_value, false));
                out.push((
                    format!("P{number} 文件系统"),
                    part.filesystem.windows_format_name(),
                    false,
                ));
                out.push((format!("P{number} 卷标"), part.volume_label.as_str(), false));
            }
            return out;
        }
        let mode = match self.provision.kind.mode() {
            Some(value) => value,
            None => return out,
        };
        let knowledge_suffix =
            |knowledge: crate::provision::SourcePasswordKnowledge| match knowledge {
                crate::provision::SourcePasswordKnowledge::DefaultVerified => "✓ 默认已验证",
                crate::provision::SourcePasswordKnowledge::UserVerified => "✓ 用户已验证",
                crate::provision::SourcePasswordKnowledge::Unknown => "⚠ Unknown",
            };
        out.extend([
            (
                "标签标识".into(),
                self.provision.form.label_id.as_str(),
                false,
            ),
            ("用户名".into(), self.provision.form.user.as_str(), false),
            ("部门".into(), self.provision.form.dept.as_str(), false),
            (
                "SAFE6 标签".into(),
                self.provision.form.label.as_str(),
                false,
            ),
        ]);
        if matches!(mode, 0 | 1 | 3) {
            let domain = if mode == 1 {
                "二合一区"
            } else {
                "交换区"
            };
            out.push((
                format!(
                    "{domain}来源密码（可空） {}",
                    knowledge_suffix(self.provision.form.share_source_knowledge)
                ),
                self.provision.form.share_source_password.as_str(),
                true,
            ));
            let share_opaque =
                self.provision_domain_opaque_candidate(crate::provision::KeyDomainRole::Share);
            out.push((
                format!("{domain}目标密码"),
                if share_opaque {
                    "— PreserveOpaque 禁用"
                } else {
                    self.provision.form.share_target_password.as_str()
                },
                !share_opaque,
            ));
        }
        if matches!(mode, 0..=2) {
            out.push((
                format!(
                    "保密区来源密码（可空） {}",
                    knowledge_suffix(self.provision.form.encrypt_source_knowledge)
                ),
                self.provision.form.encrypt_source_password.as_str(),
                true,
            ));
            let encrypt_opaque =
                self.provision_domain_opaque_candidate(crate::provision::KeyDomainRole::Encrypt);
            out.push((
                "保密区目标密码".into(),
                if encrypt_opaque {
                    "— PreserveOpaque 禁用"
                } else {
                    self.provision.form.encrypt_target_password.as_str()
                },
                !encrypt_opaque,
            ));
        }
        if matches!(mode, 0 | 3) {
            let exact =
                self.provision.form.boot_input_mode == crate::provision::CapacityInputMode::Exact;
            out.push((
                (if exact {
                    "启动区容量 (sector)"
                } else {
                    match self.provision.form.boot_quick_unit {
                        crate::provision::QuickCapacityUnit::MiB => "启动区容量 (MiB)",
                        crate::provision::QuickCapacityUnit::GiB => "启动区容量 (GiB)",
                    }
                })
                .into(),
                if exact {
                    self.provision.form.boot_sectors.as_str()
                } else {
                    self.provision.form.boot_mib.as_str()
                },
                false,
            ));
            out.push((
                "启动区起点 LBA".into(),
                self.provision.form.boot_start_lba.as_str(),
                false,
            ));
        }
        if matches!(mode, 0 | 1 | 3) {
            let exact =
                self.provision.form.share_input_mode == crate::provision::CapacityInputMode::Exact;
            out.push((
                (if exact {
                    "交换区容量 (sector)"
                } else {
                    match self.provision.form.share_quick_unit {
                        crate::provision::QuickCapacityUnit::MiB => "交换区容量 (MiB)",
                        crate::provision::QuickCapacityUnit::GiB => "交换区容量 (GiB)",
                    }
                })
                .into(),
                if exact {
                    self.provision.form.share_sectors.as_str()
                } else {
                    self.provision.form.share_mib.as_str()
                },
                false,
            ));
            out.push((
                "交换区起点 LBA".into(),
                self.provision.form.share_start_lba.as_str(),
                false,
            ));
        }
        if matches!(mode, 0..=2) {
            let exact = self.provision.form.encrypt_input_mode
                == crate::provision::CapacityInputMode::Exact;
            out.push((
                (if exact {
                    "保密区容量 (sector)"
                } else {
                    match self.provision.form.encrypt_quick_unit {
                        crate::provision::QuickCapacityUnit::MiB => "保密区容量 (MiB)",
                        crate::provision::QuickCapacityUnit::GiB => "保密区容量 (GiB)",
                    }
                })
                .into(),
                if exact {
                    self.provision.form.encrypt_sectors.as_str()
                } else {
                    self.provision.form.encrypt_mib.as_str()
                },
                false,
            ));
            out.push((
                "保密区起点 LBA".into(),
                self.provision.form.encrypt_start_lba.as_str(),
                false,
            ));
        }
        for target in self.provision_format_template() {
            let role = target.role;
            if !target.format_capable {
                out.push((role.label().into(), "固定，不格式化", false));
                continue;
            }
            let (selected, label) = match role {
                crate::provision::PartitionRole::Boot => (
                    self.provision.form.format_boot,
                    self.provision.form.volume_label.as_str(),
                ),
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined => (
                    self.provision.form.format_share,
                    self.provision.form.share_label.as_str(),
                ),
                crate::provision::PartitionRole::Encrypt => (
                    self.provision.form.format_encrypt,
                    self.provision.form.encrypt_label.as_str(),
                ),
                crate::provision::PartitionRole::CompatibilityReserve => unreachable!(),
            };
            out.push((
                format!("{}格式化", role.label()),
                if selected { "☑ 是" } else { "☐ 否" },
                false,
            ));
            out.push((
                format!("{}文件系统", role.label()),
                target.filesystem.unwrap().windows_format_name(),
                false,
            ));
            out.push((format!("{}卷标", role.label()), label, false));
        }
        out.push((
            "初始化密码强制修改".into(),
            if self.provision.form.force_change_password {
                "☑ 是"
            } else {
                "☐ 否"
            },
            false,
        ));
        out.push((
            "取消密码复杂性验证".into(),
            if self.provision.form.cancel_password_complexity_check {
                "☑ 是"
            } else {
                "☐ 否"
            },
            false,
        ));
        out.push((
            "交换区密码最大错误次数".into(),
            self.provision.form.max_share_password_errors.as_str(),
            false,
        ));
        out.push((
            "保密区密码最大错误次数".into(),
            self.provision.form.max_encrypt_password_errors.as_str(),
            false,
        ));
        for (index, (_, _, secret)) in out.iter_mut().enumerate() {
            if let Some(descriptor) = self.provision_field_descriptor(index) {
                *secret = descriptor.capabilities.secret;
            }
        }
        out
    }

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

    pub(super) fn provision_sync_cursor_to_end(&mut self) {
        self.provision.field_cursor = self
            .provision_selected_field()
            .map(|value| value.chars().count())
            .unwrap_or(0);
    }

    pub fn provision_selected_field_is_editable(&self) -> bool {
        self.provision_field_descriptor(self.provision.field_selected)
            .is_some_and(|descriptor| descriptor.capabilities.editable)
            && self.provision_selected_field().is_some()
    }

    pub fn provision_field_cursor(&self) -> usize {
        let len = self
            .provision_selected_field()
            .map(|value| value.chars().count())
            .unwrap_or(0);
        self.provision.field_cursor.min(len)
    }

    pub fn provision_move_cursor(&mut self, delta: isize) {
        let Some(value) = self.provision_selected_field() else {
            return;
        };
        let len = value.chars().count();
        self.provision.field_cursor = if delta < 0 {
            self.provision
                .field_cursor
                .saturating_sub(delta.unsigned_abs())
        } else {
            (self.provision.field_cursor + delta as usize).min(len)
        };
    }

    pub fn provision_cursor_home(&mut self) {
        if self.provision_selected_field().is_some() {
            self.provision.field_cursor = 0;
        }
    }

    pub fn provision_cursor_end(&mut self) {
        self.provision_sync_cursor_to_end();
    }

    pub(crate) fn provision_field_section_typed(
        &self,
        display_index: usize,
    ) -> Option<ProvisionFieldSection> {
        self.provision_field_descriptor(display_index)
            .map(|descriptor| descriptor.section)
    }

    pub fn provision_field_section(&self, display_index: usize) -> Option<&'static str> {
        self.provision_field_section_typed(display_index)
            .map(ProvisionFieldSection::label)
    }

    pub(crate) fn provision_compact_field_rows_typed(
        &self,
    ) -> Vec<(ProvisionFieldSection, Vec<usize>)> {
        let fields = self.provision_visible_fields();
        let mut rows = Vec::new();
        let mut index = 0usize;
        while index < fields.len() {
            let Some(descriptor) = self.provision_field_descriptor(index) else {
                break;
            };
            let section = descriptor.section;
            let width = match (section, descriptor.id) {
                (ProvisionFieldSection::Identity, _) => 2,
                (ProvisionFieldSection::PartitionLayout, ProvisionFieldId::Capacity(_)) => 2,
                (ProvisionFieldSection::PartitionLayout, _) => 1,
                (
                    ProvisionFieldSection::PasswordPolicy | ProvisionFieldSection::PasswordDomain,
                    _,
                ) => 2,
                (
                    ProvisionFieldSection::Formatting,
                    ProvisionFieldId::FormatEnabled(
                        crate::provision::PartitionRole::CompatibilityReserve,
                    ),
                ) => 1,
                (ProvisionFieldSection::Formatting, ProvisionFieldId::FormatEnabled(_)) => 2,
                (ProvisionFieldSection::Formatting, _) => 1,
                (ProvisionFieldSection::PlainPartition(_), _) => 2,
            };
            let mut end = index + 1;
            while end < fields.len()
                && end < index + width
                && self.provision_field_section_typed(end) == Some(section)
            {
                end += 1;
            }
            rows.push((section, (index..end).collect()));
            index = end;
        }
        rows
    }

    pub fn provision_compact_field_rows(&self) -> Vec<(&'static str, Vec<usize>)> {
        self.provision_compact_field_rows_typed()
            .into_iter()
            .map(|(section, indexes)| (section.label(), indexes))
            .collect()
    }

    pub fn provision_field_hint(&self, display_index: usize) -> Option<String> {
        let id = self.provision_field_id(display_index)?;
        if let ProvisionFieldId::Plain { kind, .. } = id {
            return match kind {
                PlainProvisionFieldKind::StartLba => Some("精确 LBA；不会自动移动其它分区".into()),
                PlainProvisionFieldKind::Capacity => {
                    Some("Space 切换 MiB / GiB / sector · f 填满".into())
                }
                PlainProvisionFieldKind::Filesystem => Some("Space 切换 FAT16 / exFAT".into()),
                PlainProvisionFieldKind::VolumeLabel => Some("普通卷标".into()),
            };
        }
        match id {
            ProvisionFieldId::Capacity(_) => Some("Space 切换 MiB / GiB / sector · f 填满".into()),
            ProvisionFieldId::SourcePassword(_) => {
                Some("来源密码可留空表示 Unknown · v 验证当前域旧密码".into())
            }
            ProvisionFieldId::TargetPassword(domain) => {
                Some(if self.provision_domain_opaque_candidate(domain) {
                    "PreserveOpaque：目标密码禁用；先验证旧密码才能改密".into()
                } else {
                    match domain {
                        crate::provision::KeyDomainRole::Share => {
                            "目标密码只作用于交换密钥域，不会同步到保密域".into()
                        }
                        crate::provision::KeyDomainRole::Encrypt => {
                            "目标密码只作用于保密密钥域，不会同步到交换域".into()
                        }
                    }
                })
            }
            ProvisionFieldId::ForceChangePassword
            | ProvisionFieldId::CancelPasswordComplexityCheck
            | ProvisionFieldId::FormatEnabled(_)
            | ProvisionFieldId::Filesystem(_) => Some("Space 切换".into()),
            ProvisionFieldId::StartLba(_) => Some("通常无需修改；固定分区边界时再调整".into()),
            ProvisionFieldId::MaxPasswordErrors(_) => Some("范围 0–255".into()),
            _ => None,
        }
    }

    pub(super) fn provision_input_policy(&self, id: ProvisionFieldId) -> ProvisionInputPolicy {
        if let ProvisionFieldId::Plain { partition, kind } = id {
            return match kind {
                PlainProvisionFieldKind::StartLba => ProvisionInputPolicy::UnsignedInteger,
                PlainProvisionFieldKind::Capacity => self
                    .provision
                    .plain_form
                    .partitions
                    .get(partition)
                    .map(|part| {
                        if part.input_mode == crate::provision::CapacityInputMode::Exact {
                            ProvisionInputPolicy::UnsignedInteger
                        } else {
                            ProvisionInputPolicy::DecimalCapacity
                        }
                    })
                    .unwrap_or(ProvisionInputPolicy::UnsignedInteger),
                PlainProvisionFieldKind::Filesystem | PlainProvisionFieldKind::VolumeLabel => {
                    ProvisionInputPolicy::Text
                }
            };
        }
        match id {
            ProvisionFieldId::Capacity(crate::provision::PartitionRole::Boot) => {
                if self.provision.form.boot_input_mode == crate::provision::CapacityInputMode::Exact
                {
                    ProvisionInputPolicy::UnsignedInteger
                } else {
                    ProvisionInputPolicy::DecimalCapacity
                }
            }
            ProvisionFieldId::Capacity(
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined,
            ) => {
                if self.provision.form.share_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    ProvisionInputPolicy::UnsignedInteger
                } else {
                    ProvisionInputPolicy::DecimalCapacity
                }
            }
            ProvisionFieldId::Capacity(crate::provision::PartitionRole::Encrypt) => {
                if self.provision.form.encrypt_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    ProvisionInputPolicy::UnsignedInteger
                } else {
                    ProvisionInputPolicy::DecimalCapacity
                }
            }
            ProvisionFieldId::LabelId => ProvisionInputPolicy::OnlyId,
            ProvisionFieldId::StartLba(_) => ProvisionInputPolicy::UnsignedInteger,
            ProvisionFieldId::MaxPasswordErrors(_) => ProvisionInputPolicy::U8,
            _ => ProvisionInputPolicy::Text,
        }
    }

    pub(super) fn provision_mark_capacity_edit(&mut self, id: Option<ProvisionFieldId>) {
        let role = id.and_then(|id| match id {
            ProvisionFieldId::Capacity(role) => Some(role),
            _ => None,
        });
        self.provision.form.mark_quick_capacity_edit(role);
        if let Some(ProvisionFieldId::Plain {
            partition,
            kind: PlainProvisionFieldKind::Capacity,
        }) = id
        {
            if let Some(part) = self.provision.plain_form.partitions.get_mut(partition) {
                if part.input_mode == crate::provision::CapacityInputMode::Quick {
                    part.capacity_edited = true;
                }
            }
        }
    }

    fn provision_mark_source_password_unverified(&mut self, id: Option<ProvisionFieldId>) {
        match id {
            Some(ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Share)) => {
                self.provision.form.share_source_knowledge =
                    crate::provision::SourcePasswordKnowledge::Unknown;
            }
            Some(ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Encrypt)) => {
                self.provision.form.encrypt_source_knowledge =
                    crate::provision::SourcePasswordKnowledge::Unknown;
            }
            _ => {}
        }
    }

    pub fn provision_source_password_verify_request(
        &self,
    ) -> Result<Option<(crate::provision::KeyDomainRole, String)>, String> {
        let Some(descriptor) = self.provision_field_descriptor(self.provision.field_selected)
        else {
            return Ok(None);
        };
        if !descriptor.capabilities.verify_source_password {
            return Ok(None);
        }
        let id = descriptor.id;
        let (domain, password) = match id {
            ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Share) => (
                crate::provision::KeyDomainRole::Share,
                self.provision.form.share_source_password.as_str(),
            ),
            ProvisionFieldId::SourcePassword(crate::provision::KeyDomainRole::Encrypt) => (
                crate::provision::KeyDomainRole::Encrypt,
                self.provision.form.encrypt_source_password.as_str(),
            ),
            _ => return Ok(None),
        };
        if password.is_empty() {
            return Err("请先输入当前域来源密码，再按 v 验证。".into());
        }
        Ok(Some((domain, password.to_string())))
    }

    pub fn provision_push_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        let cursor = self.provision_field_cursor();
        let Some(id) = self.provision_field_id(self.provision.field_selected) else {
            return;
        };
        let Some(current) = self.provision_selected_field() else {
            return;
        };
        if current.chars().count() >= 128 {
            return;
        }
        let mut chars = current.chars().collect::<Vec<_>>();
        chars.insert(cursor.min(chars.len()), ch);
        let candidate = chars.into_iter().collect::<String>();
        let policy = self.provision_input_policy(id);
        if !policy.accepts(&candidate) {
            self.provision.message = Some(policy.rejection_message().into());
            return;
        }
        if let Some(field) = self.provision_selected_field_mut() {
            *field = candidate;
            self.provision.field_cursor = cursor + 1;
            self.provision_mark_capacity_edit(Some(id));
            self.provision_mark_source_password_unverified(Some(id));
            self.provision.message = None;
        }
    }

    pub fn provision_backspace(&mut self) {
        let cursor = self.provision_field_cursor();
        let id = self.provision_field_id(self.provision.field_selected);
        if cursor == 0 {
            return;
        }
        if let Some(field) = self.provision_selected_field_mut() {
            let mut chars = field.chars().collect::<Vec<_>>();
            if cursor <= chars.len() {
                chars.remove(cursor - 1);
                *field = chars.into_iter().collect();
                self.provision.field_cursor = cursor - 1;
                self.provision_mark_capacity_edit(id);
                self.provision_mark_source_password_unverified(id);
                self.provision.message = None;
            }
        }
    }

    pub fn provision_delete_char(&mut self) {
        let cursor = self.provision_field_cursor();
        let id = self.provision_field_id(self.provision.field_selected);
        if let Some(field) = self.provision_selected_field_mut() {
            let mut chars = field.chars().collect::<Vec<_>>();
            if cursor < chars.len() {
                chars.remove(cursor);
                *field = chars.into_iter().collect();
                self.provision_mark_capacity_edit(id);
                self.provision_mark_source_password_unverified(id);
                self.provision.message = None;
            }
        }
    }
}
