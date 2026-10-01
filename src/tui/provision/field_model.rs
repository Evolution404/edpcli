use super::Lba8IdentityField;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlainProvisionFieldKind {
    StartLba,
    Capacity,
    Filesystem,
    VolumeLabel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProvisionFieldId {
    Plain {
        partition: usize,
        kind: PlainProvisionFieldKind,
    },
    LabelId,
    User,
    Department,
    Safe6Label,
    AdvancedSection,
    Lba8Identity(Lba8IdentityField),
    SourcePassword(crate::provision::KeyDomainRole),
    TargetPassword(crate::provision::KeyDomainRole),
    Capacity(crate::provision::PartitionRole),
    StartLba(crate::provision::PartitionRole),
    FormatEnabled(crate::provision::PartitionRole),
    Filesystem(crate::provision::PartitionRole),
    VolumeLabel(crate::provision::PartitionRole),
    ForceChangePassword,
    CancelPasswordComplexityCheck,
    MaxPasswordErrors(crate::provision::KeyDomainRole),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProvisionRegionFocus {
    None,
    Boot,
    Share,
    Encrypt,
}

impl ProvisionFieldId {
    pub(crate) const fn region_focus(self) -> ProvisionRegionFocus {
        use crate::provision::{KeyDomainRole, PartitionRole};
        match self {
            Self::Capacity(PartitionRole::Boot)
            | Self::StartLba(PartitionRole::Boot)
            | Self::FormatEnabled(PartitionRole::Boot)
            | Self::Filesystem(PartitionRole::Boot)
            | Self::VolumeLabel(PartitionRole::Boot) => ProvisionRegionFocus::Boot,
            Self::Capacity(PartitionRole::Share | PartitionRole::BootShareCombined)
            | Self::StartLba(PartitionRole::Share | PartitionRole::BootShareCombined)
            | Self::FormatEnabled(PartitionRole::Share | PartitionRole::BootShareCombined)
            | Self::Filesystem(PartitionRole::Share | PartitionRole::BootShareCombined)
            | Self::VolumeLabel(PartitionRole::Share | PartitionRole::BootShareCombined)
            | Self::SourcePassword(KeyDomainRole::Share)
            | Self::TargetPassword(KeyDomainRole::Share)
            | Self::MaxPasswordErrors(KeyDomainRole::Share) => ProvisionRegionFocus::Share,
            Self::Capacity(PartitionRole::Encrypt)
            | Self::StartLba(PartitionRole::Encrypt)
            | Self::FormatEnabled(PartitionRole::Encrypt)
            | Self::Filesystem(PartitionRole::Encrypt)
            | Self::VolumeLabel(PartitionRole::Encrypt)
            | Self::SourcePassword(KeyDomainRole::Encrypt)
            | Self::TargetPassword(KeyDomainRole::Encrypt)
            | Self::MaxPasswordErrors(KeyDomainRole::Encrypt) => ProvisionRegionFocus::Encrypt,
            _ => ProvisionRegionFocus::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ProvisionFieldSection {
    Identity,
    AdvancedIdentity,
    PartitionLayout,
    PasswordDomain,
    Formatting,
    PasswordPolicy,
    PlainPartition(usize),
}

impl ProvisionFieldSection {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Identity => "身份信息",
            Self::AdvancedIdentity => "",
            Self::PartitionLayout => "分区布局",
            Self::PasswordDomain => "密码域",
            Self::Formatting => "格式化",
            Self::PasswordPolicy => "密码策略",
            Self::PlainPartition(0) => "普通分区 P1",
            Self::PlainPartition(1) => "普通分区 P2",
            Self::PlainPartition(2) => "普通分区 P3",
            Self::PlainPartition(3) => "普通分区 P4",
            Self::PlainPartition(_) => "普通分区",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProvisionFieldCapabilities {
    pub editable: bool,
    pub secret: bool,
    pub toggle: bool,
    pub fill_capacity: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProvisionFieldDescriptor {
    pub id: ProvisionFieldId,
    pub section: ProvisionFieldSection,
    pub capabilities: ProvisionFieldCapabilities,
}
