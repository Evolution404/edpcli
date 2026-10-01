use super::*;

impl AppState {
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
                (ProvisionFieldSection::Identity, ProvisionFieldId::AdvancedSection) => 1,
                (ProvisionFieldSection::Identity, _) => 2,
                (ProvisionFieldSection::AdvancedIdentity, _) => 1,
                (ProvisionFieldSection::PartitionLayout, ProvisionFieldId::StartLba(_)) => 2,
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
                PlainProvisionFieldKind::Capacity => Some(format!(
                    "Space 切换 {} / {} / sector · f 填满",
                    ProvisionForm::quick_unit_label(crate::provision::QuickCapacityUnit::MiB),
                    ProvisionForm::quick_unit_label(crate::provision::QuickCapacityUnit::GiB)
                )),
                PlainProvisionFieldKind::Filesystem => Some("Space 切换 FAT16 / exFAT".into()),
                PlainProvisionFieldKind::VolumeLabel => Some("普通卷标".into()),
            };
        }
        match id {
            ProvisionFieldId::AdvancedSection => None,
            ProvisionFieldId::Lba8Identity(_) => Some("高级身份字段".into()),
            ProvisionFieldId::Capacity(_) => Some(format!(
                "Space 切换 {} / {} / sector · f 最大可用容量",
                ProvisionForm::quick_unit_label(crate::provision::QuickCapacityUnit::MiB),
                ProvisionForm::quick_unit_label(crate::provision::QuickCapacityUnit::GiB)
            )),
            ProvisionFieldId::SourcePassword(_) => {
                Some("修改原密码后，Enter / Esc 结束输入会自动只读验证".into())
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
            ProvisionFieldId::ForceChangePassword
            | ProvisionFieldId::CancelPasswordComplexityCheck
            | ProvisionFieldId::Filesystem(_) => Some("Space 切换".into()),
            ProvisionFieldId::StartLba(_) => Some("f 自动寻找最小可用起点".into()),
            ProvisionFieldId::MaxPasswordErrors(_) => Some("范围 0–255".into()),
            _ => None,
        }
    }
}
