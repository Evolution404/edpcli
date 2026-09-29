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
