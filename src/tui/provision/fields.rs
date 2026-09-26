use super::*;

impl AppState {
    pub fn provision_field_count(&self) -> usize {
        (0..)
            .take_while(|&index| self.provision_field_slot(index).is_some())
            .count()
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

    pub(super) fn provision_field_slot(&self, display_index: usize) -> Option<usize> {
        if self.provision.kind == ProvisionKind::Plain {
            let field_count = self.provision.plain_form.partitions.len() * 4;
            return (display_index < field_count).then_some(100 + display_index);
        }
        let mode = self.provision.kind.mode()?;
        let mut slots = Vec::with_capacity(34);
        slots.extend([3, 4, 5, 6]);
        if matches!(mode, 0 | 1 | 3) {
            slots.extend([30, 31]);
        }
        if matches!(mode, 0..=2) {
            slots.extend([32, 33]);
        }
        if matches!(mode, 0 | 3) {
            slots.extend([0, 24]);
        }
        if matches!(mode, 0 | 1 | 3) {
            slots.extend([1, 25]);
        }
        if matches!(mode, 0..=2) {
            slots.extend([2, 26]);
        }
        for target in self.provision_format_template() {
            let (toggle, filesystem, label) = match target.role {
                crate::provision::PartitionRole::Boot => (11, Some(18), Some(14)),
                crate::provision::PartitionRole::Share
                | crate::provision::PartitionRole::BootShareCombined => (12, Some(19), Some(15)),
                crate::provision::PartitionRole::Encrypt => (13, Some(20), Some(16)),
                crate::provision::PartitionRole::CompatibilityReserve => (17, None, None),
            };
            slots.push(toggle);
            if let Some(filesystem) = filesystem {
                slots.push(filesystem);
            }
            if let Some(label) = label {
                slots.push(label);
            }
        }
        slots.extend([9, 27, 28, 29]);
        slots.get(display_index).copied()
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
        out
    }

    pub(super) fn provision_selected_field_mut(&mut self) -> Option<&mut String> {
        let slot = self.provision_field_slot(self.provision.field_selected)?;
        if let Some((partition, field)) = plain_field_parts(slot) {
            let part = self.provision.plain_form.partitions.get_mut(partition)?;
            return match field {
                0 => Some(&mut part.start_lba),
                1 => Some(
                    if part.input_mode == crate::provision::CapacityInputMode::Exact {
                        &mut part.sector_count
                    } else {
                        &mut part.quick_capacity
                    },
                ),
                2 => None,
                3 => Some(&mut part.volume_label),
                _ => None,
            };
        }
        let share_opaque =
            self.provision_domain_opaque_candidate(crate::provision::KeyDomainRole::Share);
        let encrypt_opaque =
            self.provision_domain_opaque_candidate(crate::provision::KeyDomainRole::Encrypt);
        match slot {
            0 => Some(
                if self.provision.form.boot_input_mode == crate::provision::CapacityInputMode::Exact
                {
                    &mut self.provision.form.boot_sectors
                } else {
                    &mut self.provision.form.boot_mib
                },
            ),
            1 => Some(
                if self.provision.form.share_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    &mut self.provision.form.share_sectors
                } else {
                    &mut self.provision.form.share_mib
                },
            ),
            2 => Some(
                if self.provision.form.encrypt_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    &mut self.provision.form.encrypt_sectors
                } else {
                    &mut self.provision.form.encrypt_mib
                },
            ),
            3 => Some(&mut self.provision.form.label_id),
            4 => Some(&mut self.provision.form.user),
            5 => Some(&mut self.provision.form.dept),
            6 => Some(&mut self.provision.form.label),
            14 => Some(&mut self.provision.form.volume_label),
            15 => Some(&mut self.provision.form.share_label),
            16 => Some(&mut self.provision.form.encrypt_label),
            24 => Some(&mut self.provision.form.boot_start_lba),
            25 => Some(&mut self.provision.form.share_start_lba),
            26 => Some(&mut self.provision.form.encrypt_start_lba),
            28 => Some(&mut self.provision.form.max_share_password_errors),
            29 => Some(&mut self.provision.form.max_encrypt_password_errors),
            30 => Some(&mut self.provision.form.share_source_password),
            31 if !share_opaque => Some(&mut self.provision.form.share_target_password),
            32 => Some(&mut self.provision.form.encrypt_source_password),
            33 if !encrypt_opaque => Some(&mut self.provision.form.encrypt_target_password),
            _ => None,
        }
    }

    pub(super) fn provision_selected_field(&self) -> Option<&str> {
        let slot = self.provision_field_slot(self.provision.field_selected)?;
        if let Some((partition, field)) = plain_field_parts(slot) {
            let part = self.provision.plain_form.partitions.get(partition)?;
            return match field {
                0 => Some(part.start_lba.as_str()),
                1 => Some(
                    if part.input_mode == crate::provision::CapacityInputMode::Exact {
                        part.sector_count.as_str()
                    } else {
                        part.quick_capacity.as_str()
                    },
                ),
                2 => None,
                3 => Some(part.volume_label.as_str()),
                _ => None,
            };
        }
        let share_opaque =
            self.provision_domain_opaque_candidate(crate::provision::KeyDomainRole::Share);
        let encrypt_opaque =
            self.provision_domain_opaque_candidate(crate::provision::KeyDomainRole::Encrypt);
        match slot {
            0 => Some(
                if self.provision.form.boot_input_mode == crate::provision::CapacityInputMode::Exact
                {
                    self.provision.form.boot_sectors.as_str()
                } else {
                    self.provision.form.boot_mib.as_str()
                },
            ),
            1 => Some(
                if self.provision.form.share_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    self.provision.form.share_sectors.as_str()
                } else {
                    self.provision.form.share_mib.as_str()
                },
            ),
            2 => Some(
                if self.provision.form.encrypt_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    self.provision.form.encrypt_sectors.as_str()
                } else {
                    self.provision.form.encrypt_mib.as_str()
                },
            ),
            3 => Some(self.provision.form.label_id.as_str()),
            4 => Some(self.provision.form.user.as_str()),
            5 => Some(self.provision.form.dept.as_str()),
            6 => Some(self.provision.form.label.as_str()),
            14 => Some(self.provision.form.volume_label.as_str()),
            15 => Some(self.provision.form.share_label.as_str()),
            16 => Some(self.provision.form.encrypt_label.as_str()),
            24 => Some(self.provision.form.boot_start_lba.as_str()),
            25 => Some(self.provision.form.share_start_lba.as_str()),
            26 => Some(self.provision.form.encrypt_start_lba.as_str()),
            28 => Some(self.provision.form.max_share_password_errors.as_str()),
            29 => Some(self.provision.form.max_encrypt_password_errors.as_str()),
            30 => Some(self.provision.form.share_source_password.as_str()),
            31 if !share_opaque => Some(self.provision.form.share_target_password.as_str()),
            32 => Some(self.provision.form.encrypt_source_password.as_str()),
            33 if !encrypt_opaque => Some(self.provision.form.encrypt_target_password.as_str()),
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
        self.provision_selected_field().is_some()
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

    pub fn provision_field_section(&self, display_index: usize) -> Option<&'static str> {
        let slot = self.provision_field_slot(display_index)?;
        if let Some((partition, _)) = plain_field_parts(slot) {
            return Some(match partition {
                0 => "普通分区 P1",
                1 => "普通分区 P2",
                2 => "普通分区 P3",
                3 => "普通分区 P4",
                _ => return None,
            });
        }
        match slot {
            0..=2 | 24..=26 => Some("分区布局"),
            3..=6 => Some("身份信息"),
            30..=33 => Some("密码域"),
            11..=20 => Some("格式化（可选）"),
            9 | 27..=29 => Some("密码策略"),
            _ => None,
        }
    }

    pub fn provision_compact_field_rows(&self) -> Vec<(&'static str, Vec<usize>)> {
        let fields = self.provision_visible_fields();
        let mut rows = Vec::new();
        let mut index = 0usize;
        while index < fields.len() {
            let section = self.provision_field_section(index).unwrap_or("其他");
            let slot = self.provision_field_slot(index).unwrap_or(usize::MAX);
            let width = match section {
                "身份信息" => 2,
                "分区布局" if matches!(slot, 0..=2) => 2,
                "分区布局" => 1,
                "密码策略" | "密码域" => 2,
                "格式化（可选）" if slot == 17 => 1,
                "格式化（可选）" if matches!(slot, 11..=13) => 2,
                "格式化（可选）" => 1,
                _ if section.starts_with("普通分区 P") => 2,
                _ => 1,
            };
            let mut end = index + 1;
            while end < fields.len()
                && end < index + width
                && self.provision_field_section(end) == Some(section)
            {
                end += 1;
            }
            rows.push((section, (index..end).collect()));
            index = end;
        }
        rows
    }

    pub fn provision_field_hint(&self, display_index: usize) -> Option<String> {
        let slot = self.provision_field_slot(display_index)?;
        if let Some((_, field)) = plain_field_parts(slot) {
            return match field {
                0 => Some("精确 LBA；不会自动移动其它分区".into()),
                1 => Some("Space 切换 MiB / GiB / sector · f 填满".into()),
                2 => Some("Space 切换 FAT16 / exFAT".into()),
                3 => Some("普通卷标".into()),
                _ => None,
            };
        }
        match slot {
            0..=2 => Some("Space 切换 MiB / GiB / sector · f 填满".into()),
            30 | 32 => Some("来源密码可留空表示 Unknown · v 验证当前域旧密码".into()),
            31 => Some(if self
                .provision_domain_opaque_candidate(crate::provision::KeyDomainRole::Share)
            {
                "PreserveOpaque：目标密码禁用；先验证旧密码才能改密".into()
            } else {
                "目标密码只作用于交换密钥域，不会同步到保密域".into()
            }),
            33 => Some(if self
                .provision_domain_opaque_candidate(crate::provision::KeyDomainRole::Encrypt)
            {
                "PreserveOpaque：目标密码禁用；先验证旧密码才能改密".into()
            } else {
                "目标密码只作用于保密密钥域，不会同步到交换域".into()
            }),
            9 | 11..=13 | 18..=20 | 27 => Some("Space 切换".into()),
            24..=26 => Some("通常无需修改；固定分区边界时再调整".into()),
            28 | 29 => Some("范围 0–255".into()),
            _ => None,
        }
    }

    pub(super) fn provision_input_policy(&self, slot: usize) -> ProvisionInputPolicy {
        if let Some((partition, field)) = plain_field_parts(slot) {
            return match field {
                0 => ProvisionInputPolicy::UnsignedInteger,
                1 => self
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
                3 => ProvisionInputPolicy::Text,
                _ => ProvisionInputPolicy::Text,
            };
        }
        match slot {
            0 => {
                if self.provision.form.boot_input_mode == crate::provision::CapacityInputMode::Exact
                {
                    ProvisionInputPolicy::UnsignedInteger
                } else {
                    ProvisionInputPolicy::DecimalCapacity
                }
            }
            1 => {
                if self.provision.form.share_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    ProvisionInputPolicy::UnsignedInteger
                } else {
                    ProvisionInputPolicy::DecimalCapacity
                }
            }
            2 => {
                if self.provision.form.encrypt_input_mode
                    == crate::provision::CapacityInputMode::Exact
                {
                    ProvisionInputPolicy::UnsignedInteger
                } else {
                    ProvisionInputPolicy::DecimalCapacity
                }
            }
            3 => ProvisionInputPolicy::OnlyId,
            24..=26 => ProvisionInputPolicy::UnsignedInteger,
            28 | 29 => ProvisionInputPolicy::U8,
            _ => ProvisionInputPolicy::Text,
        }
    }

    pub(super) fn provision_mark_capacity_edit(&mut self, slot: Option<usize>) {
        self.provision.form.mark_quick_capacity_edit(slot);
        let Some(slot) = slot else {
            return;
        };
        let Some((partition, 1)) = plain_field_parts(slot) else {
            return;
        };
        if let Some(part) = self.provision.plain_form.partitions.get_mut(partition) {
            if part.input_mode == crate::provision::CapacityInputMode::Quick {
                part.capacity_edited = true;
            }
        }
    }

    fn provision_mark_source_password_unverified(&mut self, slot: Option<usize>) {
        match slot {
            Some(30) => {
                self.provision.form.share_source_knowledge =
                    crate::provision::SourcePasswordKnowledge::Unknown;
            }
            Some(32) => {
                self.provision.form.encrypt_source_knowledge =
                    crate::provision::SourcePasswordKnowledge::Unknown;
            }
            _ => {}
        }
    }

    pub fn provision_source_password_verify_request(
        &self,
    ) -> Result<Option<(crate::provision::KeyDomainRole, String)>, String> {
        let Some(slot) = self.provision_field_slot(self.provision.field_selected) else {
            return Ok(None);
        };
        let (domain, password) = match slot {
            30 => (
                crate::provision::KeyDomainRole::Share,
                self.provision.form.share_source_password.as_str(),
            ),
            32 => (
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
        let Some(slot) = self.provision_field_slot(self.provision.field_selected) else {
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
        let policy = self.provision_input_policy(slot);
        if !policy.accepts(&candidate) {
            self.provision.message = Some(policy.rejection_message().into());
            return;
        }
        if let Some(field) = self.provision_selected_field_mut() {
            *field = candidate;
            self.provision.field_cursor = cursor + 1;
            self.provision_mark_capacity_edit(Some(slot));
            self.provision_mark_source_password_unverified(Some(slot));
            self.provision.message = None;
        }
    }

    pub fn provision_backspace(&mut self) {
        let cursor = self.provision_field_cursor();
        let slot = self.provision_field_slot(self.provision.field_selected);
        if cursor == 0 {
            return;
        }
        if let Some(field) = self.provision_selected_field_mut() {
            let mut chars = field.chars().collect::<Vec<_>>();
            if cursor <= chars.len() {
                chars.remove(cursor - 1);
                *field = chars.into_iter().collect();
                self.provision.field_cursor = cursor - 1;
                self.provision_mark_capacity_edit(slot);
                self.provision_mark_source_password_unverified(slot);
                self.provision.message = None;
            }
        }
    }

    pub fn provision_delete_char(&mut self) {
        let cursor = self.provision_field_cursor();
        let slot = self.provision_field_slot(self.provision.field_selected);
        if let Some(field) = self.provision_selected_field_mut() {
            let mut chars = field.chars().collect::<Vec<_>>();
            if cursor < chars.len() {
                chars.remove(cursor);
                *field = chars.into_iter().collect();
                self.provision_mark_capacity_edit(slot);
                self.provision_mark_source_password_unverified(slot);
                self.provision.message = None;
            }
        }
    }
}
