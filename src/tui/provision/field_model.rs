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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ProvisionFieldSection {
    Identity,
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
            Self::PartitionLayout => "分区布局",
            Self::PasswordDomain => "密码域",
            Self::Formatting => "格式化（可选）",
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
    pub verify_source_password: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProvisionFieldDescriptor {
    pub id: ProvisionFieldId,
    pub section: ProvisionFieldSection,
    pub capabilities: ProvisionFieldCapabilities,
}
