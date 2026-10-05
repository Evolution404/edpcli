use crate::filesystem::FilesystemError;
use crate::provision::PartitionRole;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProvisionPlanningError {
    Filesystem {
        partition: Option<usize>,
        source: FilesystemError,
    },
    SizeOverflow {
        unit: &'static str,
    },
    TooManyPartitions {
        count: usize,
        max: usize,
    },
    FillWithoutSpace {
        partition: usize,
    },
    Geometry(String),
    FormatTargetCountMismatch,
    MissingFormatRole,
    MissingFilesystem {
        role: PartitionRole,
    },
    FormatImage {
        role: PartitionRole,
        message: String,
    },
    ImageMismatch {
        role: PartitionRole,
    },
}

impl std::fmt::Display for ProvisionPlanningError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Filesystem { partition, source } => match partition {
                Some(index) => write!(formatter, "P{}: {source}", index + 1),
                None => std::fmt::Display::fmt(source, formatter),
            },
            Self::SizeOverflow { unit } => write!(formatter, "普通分区 {unit} 容量溢出"),
            Self::TooManyPartitions { count, max } => {
                write!(
                    formatter,
                    "普通盘包含 {count} 个分区，最多支持 {max} 个 MBR 主分区"
                )
            }
            Self::FillWithoutSpace { partition } => {
                write!(formatter, "P{} fill 后没有可用空间", partition + 1)
            }
            Self::Geometry(message) => formatter.write_str(message),
            Self::FormatTargetCountMismatch => {
                formatter.write_str("format serial/key count does not match partition count")
            }
            Self::MissingFormatRole => formatter.write_str("当前模式不包含所选的可格式化分区"),
            Self::MissingFilesystem { role } => {
                write!(formatter, "{} 缺少文件系统定义", role.label())
            }
            Self::FormatImage { role, message } => {
                write!(formatter, "{} 格式化计划无效: {message}", role.label())
            }
            Self::ImageMismatch { role } => {
                write!(formatter, "{} 明文格式化镜像与验证镜像不一致", role.label())
            }
        }
    }
}

impl std::error::Error for ProvisionPlanningError {}
