//! Application-owned progress contract shared by provisioning, backup and restore.
//! Counts describe completed work only; frontends must not manufacture percentages.

mod retention;
pub(crate) use retention::ProgressRetention;

pub use crate::diskio::{TransactionActivity, TransactionActivityPhase};

pub const OVERALL_BASIS_POINTS: u16 = 10_000;
const LOG_CAPACITY: usize = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationKind {
    Provision,
    Backup,
    Restore,
    PostRestoreFormat,
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
    BackupCreate,
    MandatoryBackup,
    BackupVerification,
    RestoreVerification,
    RestoreWrite,
    RestoreAssessment,
    LockAndReopen,
    ProtocolWrite,
    ProtocolReadback,
    PartitionFormat(crate::provision::PartitionRole),
    PostRestoreFormat(FormatStep),
    PostWriteIdentity,
    Completed,
}

impl Step {
    pub const fn label(self) -> &'static str {
        match self {
            Self::BackupCreate => "读取并创建元数据备份",
            Self::MandatoryBackup => "制盘前元数据备份",
            Self::BackupVerification => "备份身份校验",
            Self::RestoreVerification => "校验备份与目标介质",
            Self::RestoreWrite => "恢复元数据事务",
            Self::RestoreAssessment => "恢复后只读检查",
            Self::LockAndReopen => "锁定并重开设备",
            Self::ProtocolWrite => "协议事务写盘",
            Self::ProtocolReadback => "协议读回校验",
            Self::PartitionFormat(_) => "分区格式化与读回",
            Self::PostRestoreFormat(step) => step.label(),
            Self::PostWriteIdentity => "写后身份与历史记录",
            Self::Completed => "操作完成",
        }
    }
}

/// Observable stages of an independently authorized post-restore format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatStep {
    VerifyTarget,
    LockAndReopen,
    BuildImage,
    EncryptImage,
    SyncPreflight,
    Mirror,
    Write,
    Sync,
    Readback,
    VerifyFilesystem,
    Reassess,
    RollbackWrite,
    RollbackSync,
    RollbackReadback,
}

impl FormatStep {
    pub const fn label(self) -> &'static str {
        match self {
            Self::VerifyTarget => "复核目标身份与分区状态",
            Self::LockAndReopen => "卸载/锁卷、重开与复核",
            Self::BuildImage => "生成空文件系统镜像",
            Self::EncryptImage => "加密文件系统镜像",
            Self::SyncPreflight => "写前缓存同步预检",
            Self::Mirror => "保存待修改扇区的原始内容",
            Self::Write => "写入文件系统结构",
            Self::Sync => "同步写入缓存到介质",
            Self::Readback => "读回校验文件系统结构",
            Self::VerifyFilesystem => "验证文件系统引导扇区",
            Self::Reassess => "重新评估分区可用性",
            Self::RollbackWrite => "回滚原始扇区",
            Self::RollbackSync => "同步回滚缓存到介质",
            Self::RollbackReadback => "读回校验回滚结果",
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct OverallProgress {
    basis_points: u16,
}

impl OverallProgress {
    pub const fn from_basis_points(basis_points: u16) -> Self {
        Self {
            basis_points: if basis_points > OVERALL_BASIS_POINTS {
                OVERALL_BASIS_POINTS
            } else {
                basis_points
            },
        }
    }

    pub const fn basis_points(self) -> u16 {
        self.basis_points
    }

    pub fn ratio(self) -> f64 {
        f64::from(self.basis_points) / f64::from(OVERALL_BASIS_POINTS)
    }
}

impl Default for OverallProgress {
    fn default() -> Self {
        Self::from_basis_points(0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkProgress {
    pub current: u64,
    pub total: u64,
    pub unit: Unit,
    pub activity: Option<TransactionActivityPhase>,
}

impl WorkProgress {
    pub const fn new(current: u64, total: u64, unit: Unit) -> Self {
        Self {
            current,
            total,
            unit,
            activity: None,
        }
    }

    pub const fn from_activity(activity: TransactionActivity) -> Self {
        Self {
            current: activity.current,
            total: activity.total,
            unit: Unit::Sectors,
            activity: Some(activity.phase),
        }
    }

    pub fn percent(self) -> u64 {
        self.current
            .min(self.total)
            .saturating_mul(100)
            .checked_div(self.total)
            .unwrap_or(0)
    }

    pub fn ratio(self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.current.min(self.total) as f64 / self.total as f64
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgressSpan {
    start_basis_points: u16,
    end_basis_points: u16,
}

impl ProgressSpan {
    pub const fn new(start_basis_points: u16, end_basis_points: u16) -> Self {
        assert!(start_basis_points <= end_basis_points);
        assert!(end_basis_points <= OVERALL_BASIS_POINTS);
        Self {
            start_basis_points,
            end_basis_points,
        }
    }

    pub fn for_index(index: u64, total: u64) -> Self {
        if total == 0 {
            return Self::new(0, 0);
        }
        let index = index.min(total);
        let next = index.saturating_add(1).min(total);
        let start = (u64::from(OVERALL_BASIS_POINTS).saturating_mul(index) / total) as u16;
        let end = (u64::from(OVERALL_BASIS_POINTS).saturating_mul(next) / total) as u16;
        Self::new(start, end)
    }

    pub const fn start(self) -> OverallProgress {
        OverallProgress::from_basis_points(self.start_basis_points)
    }

    pub const fn end(self) -> OverallProgress {
        OverallProgress::from_basis_points(self.end_basis_points)
    }

    pub fn interpolate(self, current: u64, total: u64) -> OverallProgress {
        if total == 0 {
            return self.start();
        }
        let current = current.min(total);
        let width = u64::from(self.end_basis_points - self.start_basis_points);
        let offset = width.saturating_mul(current) / total;
        OverallProgress::from_basis_points(self.start_basis_points + offset as u16)
    }

    pub fn subspan(self, start: u16, end: u16) -> Self {
        assert!(start <= end);
        assert!(end <= OVERALL_BASIS_POINTS);
        let width = u64::from(self.end_basis_points - self.start_basis_points);
        let absolute = |relative: u16| {
            self.start_basis_points
                + (width.saturating_mul(u64::from(relative)) / u64::from(OVERALL_BASIS_POINTS))
                    as u16
        };
        Self::new(absolute(start), absolute(end))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StageProgress {
    pub current: u64,
    pub total: u64,
}

impl StageProgress {
    pub fn new(current: u64, total: u64) -> Self {
        assert!(current <= total && total > 0);
        Self { current, total }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogPolicy {
    SnapshotOnly,
    Append,
    AppendOnChange,
}

/// Delivery meaning is independent of Warning severity (for example rollback work).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressDelivery {
    Reliable,
    WorkSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressEvent {
    pub delivery: ProgressDelivery,
    pub operation: OperationKind,
    pub phase: Phase,
    pub step: Step,
    pub overall: OverallProgress,
    pub stage: Option<StageProgress>,
    pub work: Option<WorkProgress>,
    pub detail: Option<String>,
    pub severity: Severity,
    pub log_policy: LogPolicy,
    pub emitted_at: std::time::Instant,
}

impl ProgressEvent {
    pub fn new(phase: Phase, step: Step, current: u64, total: u64) -> Self {
        assert!(current <= total && total > 0);
        Self {
            delivery: ProgressDelivery::Reliable,
            operation: OperationKind::Provision,
            phase,
            step,
            overall: ProgressSpan::new(0, OVERALL_BASIS_POINTS).interpolate(current, total),
            stage: Some(StageProgress::new(current, total)),
            work: None,
            detail: None,
            severity: Severity::Info,
            log_policy: LogPolicy::AppendOnChange,
            emitted_at: std::time::Instant::now(),
        }
    }

    pub fn with_overall(mut self, overall: OverallProgress) -> Self {
        self.overall = overall;
        self
    }

    pub fn with_stage(mut self, stage: StageProgress) -> Self {
        self.stage = Some(stage);
        self
    }

    pub fn with_work(mut self, work: WorkProgress) -> Self {
        self.delivery = ProgressDelivery::WorkSnapshot;
        self.work = Some(work);
        self
    }

    pub fn with_log_policy(mut self, log_policy: LogPolicy) -> Self {
        self.log_policy = log_policy;
        self
    }

    pub fn started(
        operation: OperationKind,
        phase: Phase,
        step: Step,
        detail: impl Into<String>,
    ) -> Self {
        let mut event = Self::new(phase, step, 0, 1);
        event.operation = operation;
        event.overall = OverallProgress::from_basis_points(0);
        event.stage = None;
        event.detail = Some(detail.into());
        event.log_policy = LogPolicy::Append;
        event
    }
}

pub fn project_write_event(
    operation: OperationKind,
    event: &super::write::WriteEvent,
) -> ProgressEvent {
    use super::write::WriteEvent;

    let (phase, step, basis_points, stage, detail, severity, log_policy) = match event {
        WriteEvent::BackupCreated { path } => (
            Phase::Complete,
            Step::Completed,
            OVERALL_BASIS_POINTS,
            Some(StageProgress::new(1, 1)),
            format!("备份已创建：{}", path.display()),
            Severity::Info,
            LogPolicy::Append,
        ),
        WriteEvent::RestoreMatchesHeader { count, .. } => (
            Phase::Backup,
            Step::RestoreVerification,
            500,
            Some(StageProgress::new(1, 6)),
            format!("找到 {count} 个匹配备份"),
            Severity::Info,
            LogPolicy::AppendOnChange,
        ),
        WriteEvent::RestoreMatchRow { index, time, .. } => (
            Phase::Backup,
            Step::RestoreVerification,
            500,
            Some(StageProgress::new(1, 6)),
            format!("候选 {index} · {time}"),
            Severity::Info,
            LogPolicy::AppendOnChange,
        ),
        WriteEvent::RestoreSelectionRetry { message } => (
            Phase::Backup,
            Step::RestoreVerification,
            500,
            Some(StageProgress::new(1, 6)),
            message.clone(),
            Severity::Warning,
            LogPolicy::Append,
        ),
        WriteEvent::BackupShaVerified { .. } => (
            Phase::Identity,
            Step::RestoreVerification,
            2_000,
            Some(StageProgress::new(2, 6)),
            "备份 SHA-256 与恢复元数据校验通过".into(),
            Severity::Info,
            LogPolicy::Append,
        ),
        WriteEvent::RestoreDryRunNotice { path, .. } => (
            Phase::Metadata,
            Step::RestoreVerification,
            3_500,
            Some(StageProgress::new(3, 6)),
            format!("恢复预览完成：{}", path.display()),
            Severity::Info,
            LogPolicy::Append,
        ),
        WriteEvent::RestoreTargetHeader { path } => (
            Phase::Identity,
            Step::LockAndReopen,
            4_000,
            Some(StageProgress::new(4, 6)),
            format!("目标介质已固定：{}", path.display()),
            Severity::Info,
            LogPolicy::Append,
        ),
        WriteEvent::RestoreWriteCompleted => (
            Phase::Readback,
            Step::RestoreWrite,
            9_000,
            Some(StageProgress::new(5, 6)),
            "元数据恢复成功；文件系统未恢复，部分分区可能需要格式化".into(),
            Severity::Info,
            LogPolicy::Append,
        ),
        WriteEvent::PostRestoreAssessment { assessment } => (
            Phase::Complete,
            Step::Completed,
            OVERALL_BASIS_POINTS,
            Some(StageProgress::new(6, 6)),
            format!("恢复后只读检查完成：{} 个分区", assessment.partitions.len()),
            Severity::Info,
            LogPolicy::Append,
        ),
    };
    ProgressEvent {
        delivery: ProgressDelivery::Reliable,
        operation,
        phase,
        step,
        overall: OverallProgress::from_basis_points(basis_points),
        stage,
        work: None,
        detail: Some(detail),
        severity,
        log_policy,
        emitted_at: std::time::Instant::now(),
    }
}

#[derive(Clone, Debug)]
pub struct OperationRunState {
    pub operation: OperationKind,
    pub target: String,
    pub started_at: std::time::Instant,
    pub last_activity_at: std::time::Instant,
    pub latest: Option<ProgressEvent>,
    pub log: std::collections::VecDeque<ProgressEvent>,
}

impl OperationRunState {
    pub fn new(operation: OperationKind, target: impl Into<String>) -> Self {
        let now = std::time::Instant::now();
        Self {
            operation,
            target: target.into(),
            started_at: now,
            last_activity_at: now,
            latest: None,
            log: std::collections::VecDeque::with_capacity(LOG_CAPACITY),
        }
    }

    pub fn push(&mut self, mut event: ProgressEvent) {
        debug_assert_eq!(event.operation, self.operation);
        self.last_activity_at = event.emitted_at;
        if let Some(previous) = self.latest.as_ref() {
            if event.overall < previous.overall {
                event.overall = previous.overall;
            }
            if self.operation == OperationKind::PostRestoreFormat
                && event.stage.is_none_or(|stage| {
                    previous
                        .stage
                        .is_some_and(|old| stage.current < old.current)
                })
            {
                event.stage = previous.stage;
            }
        }

        let safety_event = matches!(event.severity, Severity::Warning | Severity::Error);
        let rollback_event = event
            .work
            .and_then(|work| work.activity)
            .is_some_and(|phase| {
                matches!(
                    phase,
                    TransactionActivityPhase::RollbackWrite
                        | TransactionActivityPhase::RollbackSync
                        | TransactionActivityPhase::RollbackReadback
                )
            });
        let append = safety_event
            || match event.log_policy {
                LogPolicy::SnapshotOnly => {
                    rollback_event
                        && self.log.back().is_none_or(|previous| {
                            previous.work.and_then(|work| work.activity)
                                != event.work.and_then(|work| work.activity)
                        })
                }
                LogPolicy::Append => true,
                LogPolicy::AppendOnChange => self.log.back().is_none_or(|previous| {
                    previous.phase != event.phase
                        || previous.step != event.step
                        || previous.detail != event.detail
                        || previous.severity != event.severity
                        || previous.work.and_then(|work| work.activity)
                            != event.work.and_then(|work| work.activity)
                }),
            };

        if append {
            if self.log.len() == LOG_CAPACITY {
                self.log.pop_front();
            }
            self.log.push_back(event.clone());
        }
        self.latest = Some(event);
    }
}

/// A sink panic must never stop a device transaction or prevent rollback.
pub fn emit_isolated(sink: &mut dyn FnMut(ProgressEvent), event: ProgressEvent) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink(event)));
}
