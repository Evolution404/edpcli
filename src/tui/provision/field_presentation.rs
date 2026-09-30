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
        out.push((
            "高级设置".into(),
            if self.provision.advanced_identity_open {
                "▾  o 收起"
            } else {
                "▸  o 展开"
            },
            false,
        ));
        if self.provision.advanced_identity_open {
            for field in Lba8IdentityField::ALL {
                out.push((
                    field.label().into(),
                    field.value(&self.provision.form.lba8_identity),
                    false,
                ));
            }
        }
        if matches!(mode, 0 | 1 | 3) {
            out.push((
                "原密码".into(),
                self.provision.form.share_source_password.as_str(),
                true,
            ));
            out.push((
                "新密码".into(),
                self.provision.form.share_target_password.as_str(),
                true,
            ));
        }
        if matches!(mode, 0..=2) {
            out.push((
                "原密码".into(),
                self.provision.form.encrypt_source_password.as_str(),
                true,
            ));
            out.push((
                "新密码".into(),
                self.provision.form.encrypt_target_password.as_str(),
                true,
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
}
