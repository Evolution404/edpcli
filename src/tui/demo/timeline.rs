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
    pub const BACKUP_LAST_TICK: usize = 3;

    pub fn at_tick(tick: usize, base: Instant) -> ProvisionRunState {
        let steps = [
            (
                Phase::Backup,
                Step::MandatoryBackup,
                None,
                "DEMO 制盘前备份已准备",
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
