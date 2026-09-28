//! Fixed progress events projected into the production ProvisionRunState.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::application::progress::{
    OperationKind, Phase, ProgressEvent, Severity, Step, TransactionActivity,
    TransactionActivityPhase, Unit,
};
use crate::tui::state::{BackupVerifyRunState, ProvisionRunState};

pub struct DemoTimeline;

impl DemoTimeline {
    pub const LAST_TICK: usize = 5;
    pub const LONG_INITIAL_TICK: usize = 18;
    pub const LONG_LAST_TICK: usize = 44;
    pub const BACKUP_LAST_TICK: usize = 3;

    pub fn at_tick(tick: usize, base: Instant) -> ProvisionRunState {
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
        let mut log = VecDeque::new();
        for (index, (phase, step, activity, detail)) in steps
            .into_iter()
            .enumerate()
            .take(tick.min(Self::LAST_TICK) + 1)
        {
            log.push_back(ProgressEvent {
                operation: OperationKind::Provision,
                phase,
                step,
                current: (index + 1) as u64,
                total,
                unit: Unit::Steps,
                work: activity.map(|phase| TransactionActivity {
                    phase,
                    current: 7,
                    total: 13,
                }),
                detail: Some(detail.into()),
                severity: Severity::Info,
                emitted_at: base + Duration::from_secs(index as u64 * 2),
            });
        }
        let latest = log.back().cloned();
        ProvisionRunState {
            started_at: base,
            last_activity_at: latest.as_ref().map_or(base, |event| event.emitted_at),
            latest,
            log,
        }
    }

    pub fn long_at_tick(tick: usize, base: Instant) -> ProvisionRunState {
        use crate::provision::PartitionRole;

        let total = 8;
        let mut events = Vec::with_capacity(Self::LONG_LAST_TICK + 1);
        let mut push = |phase,
                        step,
                        current,
                        work: Option<(TransactionActivityPhase, u64, u64)>,
                        detail: String| {
            let index = events.len();
            events.push(ProgressEvent {
                operation: OperationKind::Provision,
                phase,
                step,
                current,
                total,
                unit: Unit::Steps,
                work: work.map(|(phase, current, total)| TransactionActivity {
                    phase,
                    current,
                    total,
                }),
                detail: Some(detail),
                severity: Severity::Info,
                emitted_at: base + Duration::from_secs(index as u64),
            });
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
        let log: VecDeque<_> = events
            .into_iter()
            .take(tick.min(Self::LONG_LAST_TICK) + 1)
            .collect();
        let latest = log.back().cloned();
        ProvisionRunState {
            started_at: base,
            last_activity_at: latest.as_ref().map_or(base, |event| event.emitted_at),
            latest,
            log,
        }
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
            .map(|(index, (phase, step, detail))| ProgressEvent {
                operation: OperationKind::Backup,
                phase,
                step,
                current: (index + 1) as u64,
                total,
                unit: Unit::Steps,
                work: None,
                detail: Some(detail.into()),
                severity: Severity::Info,
                emitted_at: base + Duration::from_secs(index as u64 * 2),
            })
            .collect();
        BackupVerifyRunState {
            path: std::path::PathBuf::from("DEMO/confirmed.edpb"),
            latest: log.back().cloned().expect("at least one demo event"),
            log,
        }
    }
}
