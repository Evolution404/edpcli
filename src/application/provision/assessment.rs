/// Read-only geometry assessment for the editor. A matching candidate still
/// needs the prepare/commit key and readback checks before it can be preserved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreserveAssessment {
    pub candidate: bool,
    pub failure: Option<crate::provision::CompatibilityFailure>,
}

impl PreserveAssessment {
    pub fn for_partition(
        source: Option<&crate::provision::ExistingPartition>,
        target: &crate::provision::TargetPartitionGeometry,
    ) -> Self {
        use crate::provision::CompatibilityFailure as Failure;
        let failure = match source {
            None => Some(Failure::SemanticRole),
            Some(_) if target.role == crate::provision::PartitionRole::CompatibilityReserve => {
                Some(Failure::CompatibilityReserveIsCanonicalRebuild)
            }
            Some(source) if source.partition_type != target.partition_type => {
                Some(Failure::PartitionType)
            }
            Some(source) if source.start_lba != target.start_lba => Some(Failure::StartLba),
            Some(source) if source.sector_count != target.sector_count => {
                Some(Failure::SectorCount)
            }
            Some(source) if source.physically_encrypted != target.physically_encrypted => {
                Some(Failure::PhysicalCrypto)
            }
            Some(_) => None,
        };
        Self {
            candidate: failure.is_none(),
            failure,
        }
    }

    pub const fn reason(self) -> &'static str {
        use crate::provision::CompatibilityFailure as Failure;
        match self.failure {
            None => "几何和物理加密属性匹配；仍需计划与密钥校验",
            Some(Failure::CompatibilityReserveIsCanonicalRebuild) => "兼容保留区必须重建",
            Some(Failure::SemanticRole) => "来源无相同语义分区",
            Some(Failure::PartitionType) => "分区类型不同",
            Some(Failure::StartLba) => "起点 LBA 已改变",
            Some(Failure::SectorCount) => "扇区数已改变",
            Some(Failure::PhysicalCrypto) => "物理加密状态已改变",
            Some(Failure::Filesystem) => "文件系统不同",
            Some(Failure::KeyDomain) => "密码域不同",
            Some(Failure::WrapMode) => "密钥封装模式不同",
        }
    }
}
