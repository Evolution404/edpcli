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
            ProvisionFieldId::AdvancedSection => None,
            ProvisionFieldId::Lba8Identity(_) => Some("高级身份字段".into()),
            ProvisionFieldId::Capacity(_) => Some("Space 切换 MiB / GiB / sector · f 填满".into()),
            ProvisionFieldId::SourcePassword(_) => {
                Some("修改原密码后，Enter / Esc 结束输入会自动只读验证".into())
            }
            ProvisionFieldId::TargetPassword(domain) => Some(match domain {
                crate::provision::KeyDomainRole::Share => {
                    "新密码只作用于交换密钥域；旧密码未知时修改会自动启用交换区重新格式化".into()
                }
                crate::provision::KeyDomainRole::Encrypt => {
                    "新密码只作用于保密密钥域；旧密码未知时修改会自动启用保密区重新格式化".into()
                }
            }),
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
