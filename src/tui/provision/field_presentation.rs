use super::*;

impl AppState {
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
}
