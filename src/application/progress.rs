//! Application-owned progress contract shared by provisioning, backup and restore.
//! Counts describe completed work only; frontends must not manufacture percentages.

pub use crate::diskio::{TransactionActivity, TransactionActivityPhase};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationKind {
    Provision,
    Backup,
    Restore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Phase {
    Backup,
    Identity,
    Metadata,
    Transaction,
    Format,
    Readback,
    Lineage,
    Complete,
}

impl Phase {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Backup => "备份",
            Self::Identity => "介质身份",
            Self::Metadata => "元数据",
            Self::Transaction => "事务写入",
            Self::Format => "格式化",
            Self::Readback => "读回校验",
            Self::Lineage => "写后记录",
            Self::Complete => "完成",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    MandatoryBackup,
    BackupVerification,
    LockAndReopen,
    ProtocolWrite,
    ProtocolReadback,
    PartitionFormat(crate::provision::PartitionRole),
    PostWriteIdentity,
    Completed,
}

impl Step {
    pub const fn label(self) -> &'static str {
        match self {
            Self::MandatoryBackup => "制盘前备份",
            Self::BackupVerification => "备份身份校验",
            Self::LockAndReopen => "锁定并重开设备",
            Self::ProtocolWrite => "协议事务写盘",
            Self::ProtocolReadback => "协议读回校验",
            Self::PartitionFormat(_) => "分区格式化与读回",
            Self::PostWriteIdentity => "写后身份与历史记录",
            Self::Completed => "操作完成",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unit {
    Steps,
    Sectors,
    Bytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressEvent {
    pub operation: OperationKind,
    pub phase: Phase,
    pub step: Step,
    pub current: u64,
    pub total: u64,
    pub unit: Unit,
    pub work: Option<TransactionActivity>,
    pub detail: Option<String>,
    pub severity: Severity,
    pub emitted_at: std::time::Instant,
}

impl ProgressEvent {
    pub fn new(phase: Phase, step: Step, current: u64, total: u64) -> Self {
        assert!(current <= total && total > 0);
        Self {
            operation: OperationKind::Provision,
            phase,
            step,
            current,
            total,
            unit: Unit::Steps,
            work: None,
            detail: None,
            severity: Severity::Info,
            emitted_at: std::time::Instant::now(),
        }
    }
}

/// A sink panic must never stop a device transaction or prevent rollback.
pub fn emit_isolated(sink: &mut dyn FnMut(ProgressEvent), event: ProgressEvent) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink(event)));
}
