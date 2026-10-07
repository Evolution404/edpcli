use super::*;

impl AppState {
    pub(crate) fn provision_field_rows_for_width(
        &self,
        width: usize,
    ) -> Vec<(ProvisionFieldSection, Vec<usize>)> {
        let rows = self.provision_compact_field_rows_typed();
        if width >= 68 {
            return rows;
        }
        rows.into_iter()
            .flat_map(|(section, indexes)| {
                indexes.into_iter().map(move |index| (section, vec![index]))
            })
            .collect()
    }

    pub(crate) fn provision_field_section_typed(
        &self,
        display_index: usize,
    ) -> Option<ProvisionFieldSection> {
        self.provision_field_descriptor(display_index)
            .map(|descriptor| descriptor.section)
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
                (ProvisionFieldSection::Identity, ProvisionFieldId::AdvancedSection) => 1,
                (ProvisionFieldSection::Identity, _) => 2,
                (ProvisionFieldSection::AdvancedIdentity, _) => 1,
                (ProvisionFieldSection::PartitionLayout, ProvisionFieldId::StartLba(_)) => 2,
                (ProvisionFieldSection::PartitionLayout, _) => 1,
                (
                    ProvisionFieldSection::PasswordPolicy | ProvisionFieldSection::PasswordDomain,
                    _,
                ) => 2,
                (ProvisionFieldSection::Formatting, ProvisionFieldId::FormatEnabled(_)) => 3,
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

    pub fn provision_field_hint(&self, display_index: usize) -> Option<String> {
        let descriptor = self.provision_field_descriptor(display_index)?;
        let id = descriptor.id;
        if let ProvisionFieldId::Plain { kind, .. } = id {
            return match kind {
                PlainProvisionFieldKind::StartLba => Some("精确 LBA；不会自动移动其它分区".into()),
                PlainProvisionFieldKind::Capacity => Some(format!(
                    "Space 切换 {} / {} / sector · f 填满",
                    ProvisionForm::quick_unit_label(crate::provision::QuickCapacityUnit::MiB),
                    ProvisionForm::quick_unit_label(crate::provision::QuickCapacityUnit::GiB)
                )),
                PlainProvisionFieldKind::Filesystem => {
                    Some("Space 切换 FAT16 / FAT32 / exFAT".into())
                }
                PlainProvisionFieldKind::VolumeLabel => Some("普通卷标".into()),
            };
        }
        match id {
            ProvisionFieldId::AdvancedSection => None,
            ProvisionFieldId::LabelId => Some("Space 生成随机新标识".into()),
            ProvisionFieldId::Lba8Identity(_) => Some("高级身份字段".into()),
            ProvisionFieldId::Capacity(_) => Some(format!(
                "Space 切换 {} / {} / sector · f 最大可用容量",
                ProvisionForm::quick_unit_label(crate::provision::QuickCapacityUnit::MiB),
                ProvisionForm::quick_unit_label(crate::provision::QuickCapacityUnit::GiB)
            )),
            ProvisionFieldId::SourcePassword(domain) if self.provision_source_password_not_applicable(domain) => {
                Some("来源没有该密码域，无需原密码".into())
            }
            ProvisionFieldId::SourcePassword(_) => {
                Some("修改原密码后，Enter / Esc 结束输入会自动只读验证".into())
            }
            ProvisionFieldId::TargetPassword(_) if !descriptor.capabilities.toggle => {
                Some("输入新密码，用于初始化该密码域".into())
            }
            ProvisionFieldId::TargetPassword(_) => Some(
                "Space 切换透传/设置密码；原密码已验证且新密码相同会自动归一化为透传；无法无损改密时系统会自动转为必须重建并格式化".into(),
            ),
            ProvisionFieldId::FormatEnabled(role) => {
                let hint = self
                    .provision_preflight()
                    .ok()
                    .and_then(|preflight| preflight.format_disposition(role))
                    .map(|disposition| match disposition {
                        preflight::ProvisionFormatDisposition::Preserve => {
                            "Space 主动重新格式化"
                        }
                        preflight::ProvisionFormatDisposition::RequiredRebuild => {
                            "当前区域必须重建，格式化不可取消"
                        }
                        preflight::ProvisionFormatDisposition::UserRequestedRebuild => {
                            "Space 取消重新格式化并恢复原样保留"
                        }
                        preflight::ProvisionFormatDisposition::NotApplicable => {
                            "固定协议区域，不格式化"
                        }
                    })
                    .unwrap_or("格式化状态暂不可判定");
                Some(hint.into())
            }
            ProvisionFieldId::Filesystem(_) if !descriptor.capabilities.toggle => {
                Some("保留现有文件系统；选择重新格式化后可切换".into())
            }
            ProvisionFieldId::Filesystem(_) => {
                Some("Space 切换 FAT16 / FAT32 / exFAT".into())
            }
            ProvisionFieldId::ForceChangePassword
            | ProvisionFieldId::CancelPasswordComplexityCheck => Some("Space 切换".into()),
            ProvisionFieldId::StartLba(_) => Some("f 自动寻找最小可用起点".into()),
            ProvisionFieldId::MaxPasswordErrors(_) => Some("范围 0–255".into()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provision_form_sections_are_compact_and_user_facing() {
        let mut state = AppState::new();
        state.provision_mut().kind = ProvisionKind::Mode0;

        let fields = state.provision_visible_fields();
        let mut sections = Vec::new();
        for index in 0..fields.len() {
            if let Some(section) = state
                .provision_field_section_typed(index)
                .map(ProvisionFieldSection::label)
            {
                if sections.last().copied() != Some(section) {
                    sections.push(section);
                }
            }
            if let Some(hint) = state.provision_field_hint(index) {
                for internal in ["canonical", "PassInfo", "XOR", "Preserve", "Rebuild"] {
                    assert!(
                        !hint.contains(internal),
                        "internal term leaked in hint: {hint}"
                    );
                }
            }
        }
        assert_eq!(
            sections,
            vec!["身份信息", "密码域", "分区布局", "格式化", "密码策略",]
        );
    }

    #[test]
    fn provision_compact_rows_keep_partition_capacity_and_start_together() {
        let mut state = AppState::new();
        state.provision_mut().kind = ProvisionKind::Mode0;

        let fields = state.provision_visible_fields();
        let rows = state.provision_field_rows_for_width(68);
        let share_row = rows
            .iter()
            .find(|(_, indexes)| {
                indexes
                    .iter()
                    .any(|index| fields[*index].0.starts_with("交换区容量"))
            })
            .expect("share row");
        let labels = share_row
            .1
            .iter()
            .map(|index| fields[*index].0.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0], "交换区起点 LBA");
        assert!(labels[1].starts_with("交换区容量"));
        let narrow = state.provision_field_rows_for_width(67);
        assert!(narrow.iter().all(|(_, indexes)| indexes.len() == 1));
        assert_eq!(
            narrow
                .iter()
                .flat_map(|(_, indexes)| indexes)
                .copied()
                .collect::<Vec<_>>(),
            rows.iter()
                .flat_map(|(_, indexes)| indexes)
                .copied()
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn provision_compact_rows_keep_formatting_controls_together() {
        let mut state = AppState::new();
        state.provision_mut().kind = ProvisionKind::Mode0;

        let fields = state.provision_visible_fields();
        let rows = state.provision_field_rows_for_width(68);
        for region in ["启动区", "交换区", "保密区"] {
            let row = rows
                .iter()
                .find(|(section, indexes)| {
                    *section == ProvisionFieldSection::Formatting
                        && indexes
                            .iter()
                            .any(|index| fields[*index].0 == format!("{region}格式化"))
                })
                .expect("formatting row");
            let labels = row
                .1
                .iter()
                .map(|index| fields[*index].0.as_str())
                .collect::<Vec<_>>();
            assert_eq!(labels.len(), 3);
            assert_eq!(labels[0], format!("{region}格式化"));
            assert_eq!(labels[1], format!("{region}文件系统"));
            assert!(
                labels[2] == format!("{region}格式化后卷标")
                    || labels[2] == format!("{region}卷标（原样保留）")
            );
        }
    }

    #[test]
    fn mode1_formatting_uses_combined_region_name() {
        let mut state = AppState::new();
        state.provision_mut().kind = ProvisionKind::Mode1;

        let fields = state.provision_visible_fields();
        let rows = state.provision_field_rows_for_width(68);
        let combined = rows
            .iter()
            .find(|(section, indexes)| {
                *section == ProvisionFieldSection::Formatting
                    && indexes
                        .iter()
                        .any(|index| fields[*index].0 == "二合一区格式化")
            })
            .expect("mode1 combined formatting row");
        assert_eq!(combined.1.len(), 3);
        assert!(!fields
            .iter()
            .any(|(label, _, _)| label.starts_with("启动/交换区")));
    }

    #[test]
    fn mode1_partition_layout_uses_combined_region_pair() {
        let mut state = AppState::new();
        state.provision_mut().kind = ProvisionKind::Mode1;

        let fields = state.provision_visible_fields();
        let rows = state.provision_field_rows_for_width(68);
        let combined_row = rows
            .iter()
            .find(|(section, indexes)| {
                *section == ProvisionFieldSection::PartitionLayout
                    && indexes
                        .iter()
                        .any(|index| fields[*index].0.starts_with("二合一区起点"))
            })
            .expect("combined partition row");
        let labels = combined_row
            .1
            .iter()
            .map(|index| fields[*index].0.as_str())
            .collect::<Vec<_>>();

        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0], "二合一区起点 LBA");
        assert!(labels[1].starts_with("二合一区容量"));
        assert!(!fields.iter().any(|(label, _, _)| {
            label.starts_with("交换区起点") || label.starts_with("交换区容量")
        }));
    }
    #[test]
    fn form_targets_and_policy_domains_are_applicable_and_indices_stay_aligned() {
        use crate::provision::{KeyDomainRole, PartitionRole};
        for kind in [
            ProvisionKind::Plain,
            ProvisionKind::Mode0,
            ProvisionKind::Mode1,
            ProvisionKind::Mode2,
            ProvisionKind::Mode3,
        ] {
            for advanced in [false, true] {
                let mut state = AppState::new();
                state.provision_mut().kind = kind;
                state.provision_mut().advanced_identity_open = advanced;
                let fields = state.provision_visible_fields();
                let descriptors = state.provision_field_descriptors();
                assert_eq!(fields.len(), descriptors.len(), "{kind:?}");
                for (index, descriptor) in descriptors.iter().enumerate() {
                    match descriptor.id {
                        ProvisionFieldId::FormatEnabled(role)
                        | ProvisionFieldId::Filesystem(role)
                        | ProvisionFieldId::VolumeLabel(role) => {
                            assert_ne!(role, PartitionRole::CompatibilityReserve);
                            let region =
                                crate::disk_layout::DiskRegionKind::from_partition_role(role)
                                    .label();
                            assert!(fields[index].0.starts_with(region));
                        }
                        ProvisionFieldId::MaxPasswordErrors(domain) => {
                            assert!(kind.disk_kind().has_key_domain(domain));
                            assert!(fields[index].0.ends_with("密码最大错误次数"));
                        }
                        _ => {}
                    }
                }
                for domain in [KeyDomainRole::Share, KeyDomainRole::Encrypt] {
                    assert_eq!(
                        descriptors
                            .iter()
                            .any(|field| field.id == ProvisionFieldId::MaxPasswordErrors(domain)),
                        kind.disk_kind().has_key_domain(domain)
                    );
                }
                for width in [40, 67, 68, 120] {
                    let rows = state.provision_field_rows_for_width(width);
                    assert_eq!(
                        rows.iter()
                            .flat_map(|(_, indices)| indices.iter().copied())
                            .collect::<Vec<_>>(),
                        (0..fields.len()).collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}
