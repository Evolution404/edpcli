use super::*;

impl AppState {
    fn provision_preserved_volume_label(
        &self,
        role: crate::provision::PartitionRole,
    ) -> Option<&str> {
        let (resolved, _) = self.provision_resolved_prefill().ok()?;
        let target = resolved
            .draft_partitions(crate::common::SECTOR as u64)
            .ok()?
            .into_iter()
            .find(|partition| partition.role == role)?;
        self.selected_device()?
            .partition_table
            .as_ref()?
            .partitions
            .iter()
            .find(|partition| {
                partition.start_lba == target.start_lba
                    && partition.sector_count == target.sector_count
            })
            .and_then(|partition| partition.volume_label.as_deref())
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
                            ProvisionForm::quick_unit_label(part.quick_unit)
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
                    part.filesystem.display_name(),
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
                if self.provision_target_password_mode(crate::provision::KeyDomainRole::Share)
                    == password_verification::TargetPasswordMode::Passthrough
                {
                    "透传"
                } else {
                    self.provision.form.share_target_password.as_str()
                },
                self.provision_target_password_mode(crate::provision::KeyDomainRole::Share)
                    == password_verification::TargetPasswordMode::Explicit,
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
                if self.provision_target_password_mode(crate::provision::KeyDomainRole::Encrypt)
                    == password_verification::TargetPasswordMode::Passthrough
                {
                    "透传"
                } else {
                    self.provision.form.encrypt_target_password.as_str()
                },
                self.provision_target_password_mode(crate::provision::KeyDomainRole::Encrypt)
                    == password_verification::TargetPasswordMode::Explicit,
            ));
        }
        if matches!(mode, 0 | 3) {
            let exact =
                self.provision.form.boot_input_mode == crate::provision::CapacityInputMode::Exact;
            out.push((
                "启动区起点 LBA".into(),
                self.provision.form.boot_start_lba.as_str(),
                false,
            ));
            out.push((
                (if exact {
                    "启动区容量 (sector)".into()
                } else {
                    format!(
                        "启动区容量 ({})",
                        ProvisionForm::quick_unit_label(self.provision.form.boot_quick_unit)
                    )
                }),
                if exact {
                    self.provision.form.boot_sectors.as_str()
                } else {
                    self.provision.form.boot_mib.as_str()
                },
                false,
            ));
        }
        if matches!(mode, 0 | 1 | 3) {
            let exact =
                self.provision.form.share_input_mode == crate::provision::CapacityInputMode::Exact;
            let region = if mode == 1 {
                "二合一区"
            } else {
                "交换区"
            };
            out.push((
                format!("{region}起点 LBA"),
                self.provision.form.share_start_lba.as_str(),
                false,
            ));
            out.push((
                (if exact {
                    format!("{region}容量 (sector)")
                } else {
                    format!(
                        "{region}容量 ({})",
                        ProvisionForm::quick_unit_label(self.provision.form.share_quick_unit)
                    )
                }),
                if exact {
                    self.provision.form.share_sectors.as_str()
                } else {
                    self.provision.form.share_mib.as_str()
                },
                false,
            ));
        }
        if matches!(mode, 0..=2) {
            let exact = self.provision.form.encrypt_input_mode
                == crate::provision::CapacityInputMode::Exact;
            out.push((
                "保密区起点 LBA".into(),
                self.provision.form.encrypt_start_lba.as_str(),
                false,
            ));
            out.push((
                (if exact {
                    "保密区容量 (sector)".into()
                } else {
                    format!(
                        "保密区容量 ({})",
                        ProvisionForm::quick_unit_label(self.provision.form.encrypt_quick_unit)
                    )
                }),
                if exact {
                    self.provision.form.encrypt_sectors.as_str()
                } else {
                    self.provision.form.encrypt_mib.as_str()
                },
                false,
            ));
        }
        let preflight = self.provision_preflight().ok();
        for target in self.provision_format_template() {
            let role = target.role;
            if !target.format_capable {
                out.push((
                    role.label().into(),
                    if role == crate::provision::PartitionRole::CompatibilityReserve {
                        "固定 63 sector · 不格式化"
                    } else {
                        "固定，不格式化"
                    },
                    false,
                ));
                continue;
            }
            let disposition = preflight
                .as_ref()
                .and_then(|value| value.format_disposition(role))
                .unwrap_or_else(|| {
                    if self.provision_explicit_format_selected(role) {
                        preflight::ProvisionFormatDisposition::UserRequestedRebuild
                    } else {
                        preflight::ProvisionFormatDisposition::Preserve
                    }
                });
            let region = crate::disk_layout::DiskRegionKind::from_partition_role(role).label();
            let target_label = match role {
                crate::provision::PartitionRole::Boot
                | crate::provision::PartitionRole::BootShareCombined => {
                    self.provision.form.volume_label.as_str()
                }
                crate::provision::PartitionRole::Share => self.provision.form.share_label.as_str(),
                crate::provision::PartitionRole::Encrypt => {
                    self.provision.form.encrypt_label.as_str()
                }
                crate::provision::PartitionRole::CompatibilityReserve => unreachable!(),
            };
            let format_status = match disposition {
                preflight::ProvisionFormatDisposition::Preserve => "☐ 保留",
                preflight::ProvisionFormatDisposition::RequiredRebuild => "☑ 必须",
                preflight::ProvisionFormatDisposition::UserRequestedRebuild => "☑ 格式化",
                preflight::ProvisionFormatDisposition::NotApplicable => "固定，不格式化",
            };
            out.push((format!("{region}格式化"), format_status, false));
            out.push((
                format!("{region}文件系统"),
                target.filesystem.unwrap().display_name(),
                false,
            ));
            if disposition.selected() {
                out.push((format!("{region}格式化后卷标"), target_label, false));
            } else {
                out.push((
                    format!("{region}卷标（原样保留）"),
                    self.provision_preserved_volume_label(role)
                        .unwrap_or("未读取"),
                    false,
                ));
            }
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
