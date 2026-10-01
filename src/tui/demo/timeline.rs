//! Fixed progress events projected into the production OperationRunState.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::application::progress::{
    LogPolicy, OperationKind, OperationRunState, Phase, ProgressEvent, Severity, Step,
    TransactionActivity, TransactionActivityPhase, WorkProgress,
};
use crate::tui::state::BackupVerifyRunState;

fn demo_progress(
    operation: OperationKind,
    phase: Phase,
    step: Step,
    progress: (u64, u64),
    work: Option<TransactionActivity>,
    detail: impl Into<String>,
    emitted_at: Instant,
) -> ProgressEvent {
    let (current, total) = progress;
    let mut event = ProgressEvent::new(phase, step, current, total);
    event.operation = operation;
    event.work = work.map(WorkProgress::from_activity);
    event.detail = Some(detail.into());
    event.severity = Severity::Info;
    event.log_policy = if event.work.is_some() {
        LogPolicy::SnapshotOnly
    } else {
        LogPolicy::AppendOnChange
    };
    event.emitted_at = emitted_at;
    event
}

pub struct DemoTimeline;

impl DemoTimeline {
    pub const LAST_TICK: usize = 5;
    pub const LONG_INITIAL_TICK: usize = 18;
    pub const LONG_LAST_TICK: usize = 44;
    pub const BACKUP_LAST_TICK: usize = 3;

    pub fn at_tick(tick: usize, base: Instant) -> OperationRunState {
        let steps = [
            (
                Phase::Backup,
                Step::MandatoryBackup,
                None,
                "DEMO 制盘前元数据备份已准备",
            ),
            (
                Phase::Identity,
                Step::BackupVerification,
                None,
                "DEMO 介质身份已确认",
            ),
            (
                Phase::Transaction,
                Step::LockAndReopen,
                None,
                "DEMO 锁定与重开已模拟",
            ),
            (
                Phase::Transaction,
                Step::ProtocolWrite,
                Some(TransactionActivityPhase::Write),
                "DEMO 协议扇区活动 · 演示模式不会执行真实操作",
            ),
            (
                Phase::Readback,
                Step::ProtocolReadback,
                Some(TransactionActivityPhase::Readback),
                "DEMO 读回比对",
            ),
            (
                Phase::Complete,
                Step::Completed,
                None,
                "演示模式不会执行真实操作",
            ),
        ];
        let total = steps.len() as u64;
        let mut run = OperationRunState::new(OperationKind::Provision, "DEMO disk");
        run.started_at = base;
        run.last_activity_at = base;
        for (index, (phase, step, activity, detail)) in steps
            .into_iter()
            .enumerate()
            .take(tick.min(Self::LAST_TICK) + 1)
        {
            run.push(demo_progress(
                OperationKind::Provision,
                phase,
                step,
                ((index + 1) as u64, total),
                activity.map(|phase| TransactionActivity {
                    phase,
                    current: 7,
                    total: 13,
                }),
                detail,
                base + Duration::from_secs(index as u64 * 2),
            ));
        }
        run
    }

    pub fn long_at_tick(tick: usize, base: Instant) -> OperationRunState {
        use crate::provision::PartitionRole;

        let total = 8;
        let mut events = Vec::with_capacity(Self::LONG_LAST_TICK + 1);
        let mut push = |phase,
                        step,
                        current,
                        work: Option<(TransactionActivityPhase, u64, u64)>,
                        detail: String| {
            let index = events.len();
            events.push(demo_progress(
                OperationKind::Provision,
                phase,
                step,
                (current, total),
                work.map(|(phase, current, total)| TransactionActivity {
                    phase,
                    current,
                    total,
                }),
                detail,
                base + Duration::from_secs(index as u64),
            ));
        };

        push(
            Phase::Backup,
            Step::MandatoryBackup,
            0,
            None,
            "DEMO 慢盘：正在创建制盘前元数据备份".into(),
        );
        push(
            Phase::Backup,
            Step::MandatoryBackup,
            1,
            None,
            "DEMO 慢盘：制盘前元数据备份已落盘并 fsync".into(),
        );
        push(
            Phase::Identity,
            Step::BackupVerification,
            2,
            None,
            "DEMO 慢盘：备份身份与目标介质已确认".into(),
        );
        push(
            Phase::Identity,
            Step::LockAndReopen,
            2,
            None,
            "DEMO 慢盘：设备已锁定并完成 reopen 身份复核".into(),
        );
        for current in [1, 3, 5, 7, 9, 11, 13] {
            push(
                Phase::Transaction,
                Step::ProtocolWrite,
                2,
                Some((TransactionActivityPhase::Write, current, 13)),
                format!("DEMO 慢盘：协议事务写入 {current}/13 sectors"),
            );
        }
        push(
            Phase::Transaction,
            Step::ProtocolWrite,
            3,
            None,
            "DEMO 慢盘：协议事务写入完成并同步".into(),
        );
        for current in [1, 4, 7, 10, 13] {
            push(
                Phase::Readback,
                Step::ProtocolReadback,
                3,
                Some((TransactionActivityPhase::Readback, current, 13)),
                format!("DEMO 慢盘：协议读回比对 {current}/13 sectors"),
            );
        }
        push(
            Phase::Readback,
            Step::ProtocolReadback,
            4,
            None,
            "DEMO 慢盘：协议读回校验完成".into(),
        );

        for current in [64, 160, 288, 416, 512] {
            push(
                Phase::Format,
                Step::PartitionFormat(PartitionRole::Boot),
                4,
                Some((TransactionActivityPhase::FormatWrite, current, 512)),
                format!("DEMO 慢盘：启动区格式化写入 {current}/512 sectors"),
            );
        }
        for current in [128, 320, 512] {
            push(
                Phase::Format,
                Step::PartitionFormat(PartitionRole::Boot),
                4,
                Some((TransactionActivityPhase::FormatReadback, current, 512)),
                format!("DEMO 慢盘：启动区格式化读回 {current}/512 sectors"),
            );
        }
        push(
            Phase::Format,
            Step::PartitionFormat(PartitionRole::Boot),
            5,
            None,
            "DEMO 慢盘：启动区格式化与读回完成".into(),
        );

        for current in [4_096, 12_288, 24_576, 32_768] {
            push(
                Phase::Format,
                Step::PartitionFormat(PartitionRole::Share),
                5,
                Some((TransactionActivityPhase::FormatWrite, current, 32_768)),
                format!("DEMO 慢盘：交换区格式化写入 {current}/32768 sectors"),
            );
        }
        for current in [8_192, 24_576, 32_768] {
            push(
                Phase::Format,
                Step::PartitionFormat(PartitionRole::Share),
                5,
                Some((TransactionActivityPhase::FormatReadback, current, 32_768)),
                format!("DEMO 慢盘：交换区格式化读回 {current}/32768 sectors"),
            );
        }
        push(
            Phase::Format,
            Step::PartitionFormat(PartitionRole::Share),
            6,
            None,
            "DEMO 慢盘：交换区格式化与读回完成".into(),
        );

        for current in [2_048, 8_192, 14_336, 16_384] {
            push(
                Phase::Format,
                Step::PartitionFormat(PartitionRole::Encrypt),
                6,
                Some((TransactionActivityPhase::FormatWrite, current, 16_384)),
                format!("DEMO 慢盘：保密区格式化写入 {current}/16384 sectors"),
            );
        }
        for current in [4_096, 12_288, 16_384] {
            push(
                Phase::Format,
                Step::PartitionFormat(PartitionRole::Encrypt),
                6,
                Some((TransactionActivityPhase::FormatReadback, current, 16_384)),
                format!("DEMO 慢盘：保密区格式化读回 {current}/16384 sectors"),
            );
        }
        push(
            Phase::Format,
            Step::PartitionFormat(PartitionRole::Encrypt),
            7,
            None,
            "DEMO 慢盘：保密区格式化与读回完成".into(),
        );
        push(
            Phase::Lineage,
            Step::PostWriteIdentity,
            8,
            None,
            "DEMO 慢盘：写后身份与历史记录已完成".into(),
        );
        push(
            Phase::Complete,
            Step::Completed,
            8,
            None,
            "DEMO 慢盘：全部步骤完成；演示模式未访问真实介质".into(),
        );

        debug_assert_eq!(events.len(), Self::LONG_LAST_TICK + 1);
        let mut run = OperationRunState::new(OperationKind::Provision, "DEMO slow disk");
        run.started_at = base;
        run.last_activity_at = base;
        for event in events.into_iter().take(tick.min(Self::LONG_LAST_TICK) + 1) {
            run.push(event);
        }
        run
    }

    pub fn backup_verify_at_tick(tick: usize, base: Instant) -> BackupVerifyRunState {
        let steps = [
            (Phase::Backup, Step::BackupVerification, "DEMO 打开备份清单"),
            (
                Phase::Readback,
                Step::BackupVerification,
                "DEMO 检查大小与摘要",
            ),
            (
                Phase::Readback,
                Step::BackupVerification,
                "DEMO 校验 SHA-256",
            ),
            (
                Phase::Complete,
                Step::Completed,
                "DEMO 校验结束；未访问真实文件",
            ),
        ];
        let total = steps.len() as u64;
        let log: VecDeque<_> = steps
            .into_iter()
            .enumerate()
            .take(tick.min(Self::BACKUP_LAST_TICK) + 1)
            .map(|(index, (phase, step, detail))| {
                demo_progress(
                    OperationKind::Backup,
                    phase,
                    step,
                    ((index + 1) as u64, total),
                    None,
                    detail,
                    base + Duration::from_secs(index as u64 * 2),
                )
            })
            .collect();
        BackupVerifyRunState {
            path: std::path::PathBuf::from("DEMO/confirmed.edpb"),
            latest: log.back().cloned().expect("at least one demo event"),
            log,
        }
    }
}
