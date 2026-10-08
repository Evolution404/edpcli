use super::*;

fn write_and_verify<'a>(
    dev: &mut dyn SectorDev,
    sector: impl Fn(u32) -> Option<&'a [u8]>,
    order: &[u32],
    readback_order: &[u32],
    rollback: bool,
    observer: &mut dyn FnMut(TransactionActivity),
) -> io::Result<()> {
    notify_activity(
        observer,
        TransactionActivity {
            phase: if rollback {
                TransactionActivityPhase::RollbackWrite
            } else {
                TransactionActivityPhase::Write
            },
            current: 0,
            total: order.len() as u64,
        },
    );
    for (index, &lba) in order.iter().enumerate() {
        let bytes =
            sector(lba).ok_or_else(|| io::Error::other(format!("写入计划缺少 LBA{lba}")))?;
        dev.write_sector(lba, bytes).map_err(|error| {
            io::Error::new(error.kind(), format!("写入 LBA{lba} 失败: {error}"))
        })?;
        notify_activity(
            observer,
            TransactionActivity {
                phase: if rollback {
                    TransactionActivityPhase::RollbackWrite
                } else {
                    TransactionActivityPhase::Write
                },
                current: index as u64 + 1,
                total: order.len() as u64,
            },
        );
    }
    // 读回前先把写缓存提交到介质；否则紧随其后的 pread 可能只验证到内核缓存。
    notify_activity(
        observer,
        TransactionActivity {
            phase: if rollback {
                TransactionActivityPhase::RollbackSync
            } else {
                TransactionActivityPhase::Sync
            },
            current: 0,
            total: 0,
        },
    );
    dev.sync()
        .map_err(|error| io::Error::new(error.kind(), format!("写后缓存同步失败: {error}")))?;
    notify_activity(
        observer,
        TransactionActivity {
            phase: if rollback {
                TransactionActivityPhase::RollbackReadback
            } else {
                TransactionActivityPhase::Readback
            },
            current: 0,
            total: readback_order.len() as u64,
        },
    );
    let mut checked = [0_u8; SECTOR];
    for (index, &lba) in readback_order.iter().enumerate() {
        dev.read_sector_into(lba, &mut checked).map_err(|error| {
            io::Error::new(error.kind(), format!("读回 LBA{lba} 失败: {error}"))
        })?;
        let expected =
            sector(lba).ok_or_else(|| io::Error::other(format!("读回计划缺少 LBA{lba}")))?;
        if checked.as_slice() != expected {
            return Err(io::Error::other(format!("LBA{} 读回校验不符", lba)));
        }
        notify_activity(
            observer,
            TransactionActivity {
                phase: if rollback {
                    TransactionActivityPhase::RollbackReadback
                } else {
                    TransactionActivityPhase::Readback
                },
                current: index as u64 + 1,
                total: readback_order.len() as u64,
            },
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransactionActivityPhase {
    SyncPreflight,
    Mirror,
    Write,
    Sync,
    Readback,
    RollbackWrite,
    RollbackSync,
    RollbackReadback,
    FormatWrite,
    FormatReadback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransactionActivity {
    pub phase: TransactionActivityPhase,
    pub current: u64,
    pub total: u64,
}

fn notify_activity(observer: &mut dyn FnMut(TransactionActivity), event: TransactionActivity) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer(event)));
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SectorWriteStage {
    Data,
    Metadata,
    Commit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionSectorWrite {
    pub bytes: Vec<u8>,
    pub stage: SectorWriteStage,
    pub owner: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteTransactionPlan {
    total_sectors: u64,
    writes: BTreeMap<u32, TransactionSectorWrite>,
}

impl WriteTransactionPlan {
    pub fn new(total_sectors: u64) -> Self {
        Self {
            total_sectors,
            writes: BTreeMap::new(),
        }
    }

    pub fn total_sectors(&self) -> u64 {
        self.total_sectors
    }

    pub fn writes(&self) -> &BTreeMap<u32, TransactionSectorWrite> {
        &self.writes
    }

    pub fn insert(
        &mut self,
        lba: u32,
        bytes: Vec<u8>,
        stage: SectorWriteStage,
        owner: impl Into<String>,
    ) -> Result<(), String> {
        if self.total_sectors == 0 || u64::from(lba) >= self.total_sectors {
            return Err(format!(
                "事务写入 LBA{lba} 超过目标末端{}",
                self.total_sectors.saturating_sub(1)
            ));
        }
        if bytes.len() != SECTOR {
            return Err(format!(
                "事务写入 LBA{lba} 数据长度 {}B，必须为 {SECTOR}B",
                bytes.len()
            ));
        }
        if self.writes.contains_key(&lba) {
            return Err(format!("事务写计划重复声明 LBA{lba}"));
        }
        self.writes.insert(
            lba,
            TransactionSectorWrite {
                bytes,
                stage,
                owner: owner.into(),
            },
        );
        Ok(())
    }

    pub fn from_plain_provision(
        plain: &crate::provision::PlainProvisionWritePlan,
    ) -> Result<Self, String> {
        let mut transaction = Self::new(plain.total_sectors);
        for (&lba, write) in &plain.writes {
            let (stage, owner) = match write.owner {
                crate::provision::PlainSectorOwner::Mbr => {
                    (SectorWriteStage::Commit, "plain mbr".to_string())
                }
                crate::provision::PlainSectorOwner::EdpMetadataCleanup => (
                    SectorWriteStage::Metadata,
                    "plain edp metadata cleanup".to_string(),
                ),
                crate::provision::PlainSectorOwner::LceCleanup => {
                    (SectorWriteStage::Data, "plain lce cleanup".to_string())
                }
                crate::provision::PlainSectorOwner::Filesystem { partition_index } => (
                    SectorWriteStage::Data,
                    format!("plain filesystem P{}", partition_index + 1),
                ),
            };
            transaction.insert(lba, write.bytes.to_vec(), stage, owner)?;
        }
        Ok(transaction)
    }

    fn ordered_lbas(&self) -> Vec<u32> {
        // BTreeMap is already LBA sorted. Three passes preserve stage then LBA
        // ordering while avoiding a temporary tuple vector and comparison sort.
        let mut order = Vec::with_capacity(self.writes.len());
        for stage in [
            SectorWriteStage::Data,
            SectorWriteStage::Metadata,
            SectorWriteStage::Commit,
        ] {
            order.extend(
                self.writes
                    .iter()
                    .filter_map(|(&lba, write)| (write.stage == stage).then_some(lba)),
            );
        }
        order
    }
}

/// Execute an already normalized sector transaction.
///
/// The planner owns business semantics and final sector ownership. This executor
/// owns only the safety mechanics: preflight sync, snapshot of every touched
/// sector, stage-ordered writes, exact readback, and exact rollback on failure.
pub fn execute_write_transaction(
    dev: &mut dyn SectorDev,
    plan: &WriteTransactionPlan,
) -> EdpCliResult<()> {
    execute_write_transaction_observed(dev, plan, &mut |_| {})
}

pub fn execute_write_transaction_observed(
    dev: &mut dyn SectorDev,
    plan: &WriteTransactionPlan,
    observer: &mut dyn FnMut(TransactionActivity),
) -> EdpCliResult<()> {
    if plan.writes.is_empty() {
        return Ok(());
    }
    if plan.total_sectors == 0 || plan.total_sectors > u32::MAX as u64 {
        return Err(EdpCliError::new(
            EXIT_IO,
            format!("错误: 事务目标扇区数不受支持: {}", plan.total_sectors),
        ));
    }

    notify_activity(
        observer,
        TransactionActivity {
            phase: TransactionActivityPhase::SyncPreflight,
            current: 0,
            total: 0,
        },
    );
    dev.sync().map_err(|e| {
        EdpCliError::new(
            EXIT_IO,
            format!("错误: 写前介质缓存同步预检失败，拒绝开始事务: {e}"),
        )
    })?;

    // Borrow the immutable planned sectors; only the rollback mirror owns old bytes.
    let readback_order = plan.writes.keys().copied().collect::<Vec<_>>();
    let order = plan.ordered_lbas();
    // A sorted LBA index already exists in readback_order. Keep one contiguous
    // rollback image instead of one BTreeMap node + heap allocation per sector.
    // A failed reservation is handled before ANY data write.
    let mut mirror: Vec<[u8; SECTOR]> = Vec::new();
    mirror
        .try_reserve_exact(readback_order.len())
        .map_err(|error| {
            EdpCliError::new(EXIT_IO, format!("错误: 回滚镜像内存预留失败: {error}"))
        })?;
    notify_activity(
        observer,
        TransactionActivity {
            phase: TransactionActivityPhase::Mirror,
            current: 0,
            total: readback_order.len() as u64,
        },
    );
    for (index, &lba) in readback_order.iter().enumerate() {
        let mut old = [0_u8; SECTOR];
        dev.read_sector_into(lba, &mut old).map_err(|e| {
            EdpCliError::new(
                EXIT_IO,
                format!("错误: 回滚镜像读取 LBA{lba} 失败: {e}；尚未写入介质"),
            )
        })?;
        mirror.push(old);
        notify_activity(
            observer,
            TransactionActivity {
                phase: TransactionActivityPhase::Mirror,
                current: index as u64 + 1,
                total: readback_order.len() as u64,
            },
        );
    }

    match write_and_verify(
        dev,
        |lba| plan.writes.get(&lba).map(|write| write.bytes.as_slice()),
        &order,
        &readback_order,
        false,
        observer,
    ) {
        Ok(()) => Ok(()),
        Err(write_error) => {
            for attempt in 0..3 {
                match write_and_verify(
                    dev,
                    |lba| {
                        readback_order
                            .binary_search(&lba)
                            .ok()
                            .and_then(|index| mirror.get(index))
                            .map(|bytes| bytes.as_slice())
                    },
                    &order,
                    &readback_order,
                    true,
                    observer,
                ) {
                    Ok(()) => {
                        return Err(EdpCliError::new(
                            EXIT_ROLLED_BACK,
                            format!("错误: 事务写入失败({write_error})，已完整回滚全部 touched sectors；目标仍为写前状态。"),
                        ));
                    }
                    Err(error) if attempt == 2 => {
                        return Err(EdpCliError::new(
                            EXIT_INTERMEDIATE,
                            format!(
                                "错误: 事务写入失败({write_error})，事务回滚失败({error})，目标处于中间状态；禁止继续使用该盘。"
                            ),
                        ));
                    }
                    Err(_) => thread::sleep(Duration::from_millis(500)),
                }
            }
            Err(EdpCliError::new(
                EXIT_INTERMEDIATE,
                "错误: 事务回滚重试次数耗尽，目标可能处于中间状态；禁止继续使用该盘。",
            ))
        }
    }
}

/// 全有或全无写盘(patch={lba:512B 新内容})。
///
/// USB 盘硬件没有跨扇区事务, 严格原子不可得; 以四层逼近:
///   1) 单 fd 打开后全程持有 → 不存在中途重开撞 EBUSY/系统重扫的窗口;
///   2) LBA0(唯一改 MBR 的扇区)最后写 → 未到它之前系统视角的 MBR 仍是旧的;
///   3) 写完逐扇读回校验, 落盘与否以读回为准;
///   4) 任一失败 → 以写前内存镜像自动回滚全部扇区并再校验。
///
/// 回滚成功 → EXIT_ROLLED_BACK(盘仍为写前状态, 可安全重试);
/// 回滚失败 → EXIT_INTERMEDIATE(中间态, 指引重插后 edpcli backup restore 从备份还原)。
pub fn atomic_write_sectors(
    dev: &mut dyn SectorDev,
    patch: &BTreeMap<u32, Vec<u8>>,
) -> EdpCliResult<()> {
    let mut plan = WriteTransactionPlan::new((crate::common::METADATA_LAST_LBA + 1) as u64);
    for (&lba, data) in patch {
        if lba > crate::common::METADATA_LAST_LBA {
            return Err(EdpCliError::new(
                EXIT_IO,
                format!(
                    "错误: 原子写仅允许元数据 LBA0-{}，收到 LBA{}",
                    crate::common::METADATA_LAST_LBA,
                    lba
                ),
            ));
        }
        let stage = if lba == 0 {
            SectorWriteStage::Commit
        } else {
            SectorWriteStage::Metadata
        };
        plan.insert(lba, data.clone(), stage, "metadata")
            .map_err(|message| EdpCliError::new(EXIT_IO, format!("错误: {message}")))?;
    }
    execute_write_transaction(dev, &plan)
}

/// Transactional writer for a fully planned new-disk provisioning image.
///
/// The domain planner has already bounded every filesystem/LCE sector. This
/// layer independently enforces target bounds and complete LBA0-12 metadata,
/// mirrors every touched sector, writes data/LCE first, metadata LBA1-12 next,
/// and commits LBA0/MBR last. Any write/readback failure rolls back the exact
/// same touched set.
pub fn atomic_write_official_provision_sectors(
    dev: &mut dyn SectorDev,
    patch: &BTreeMap<u32, Vec<u8>>,
    total_sectors: u64,
) -> EdpCliResult<()> {
    atomic_write_official_provision_sectors_observed(dev, patch, total_sectors, &mut |_| {})
}

pub fn atomic_write_official_provision_sectors_observed(
    dev: &mut dyn SectorDev,
    patch: &BTreeMap<u32, Vec<u8>>,
    total_sectors: u64,
    observer: &mut dyn FnMut(TransactionActivity),
) -> EdpCliResult<()> {
    if total_sectors == 0 || total_sectors > u32::MAX as u64 {
        return Err(EdpCliError::new(
            EXIT_IO,
            format!("错误: 制盘目标扇区数不受支持: {total_sectors}"),
        ));
    }
    for required in 0..=crate::common::METADATA_LAST_LBA {
        if !patch.contains_key(&required) {
            return Err(EdpCliError::new(
                EXIT_IO,
                format!("错误: 制盘写入计划缺少协议元数据 LBA{required}"),
            ));
        }
    }
    let mut has_data = false;
    for (&lba, data) in patch {
        if u64::from(lba) >= total_sectors {
            return Err(EdpCliError::new(
                EXIT_IO,
                format!(
                    "错误: 制盘写入计划 LBA{lba} 超过目标末端 LBA{}",
                    total_sectors - 1
                ),
            ));
        }
        has_data |= lba > crate::common::METADATA_LAST_LBA;
        if data.len() != SECTOR {
            return Err(EdpCliError::new(
                EXIT_IO,
                format!(
                    "错误: 制盘 LBA{lba} 数据长度 {}B，必须为 {SECTOR}B",
                    data.len()
                ),
            ));
        }
    }
    if !has_data {
        return Err(EdpCliError::new(
            EXIT_IO,
            "错误: 制盘写入计划没有文件系统/LCE 数据扇区",
        ));
    }

    let mut plan = WriteTransactionPlan::new(total_sectors);
    for (&lba, data) in patch {
        let stage = if lba == 0 {
            SectorWriteStage::Commit
        } else if lba <= crate::common::METADATA_LAST_LBA {
            SectorWriteStage::Metadata
        } else {
            SectorWriteStage::Data
        };
        plan.insert(lba, data.clone(), stage, "official provision")
            .map_err(|message| EdpCliError::new(EXIT_IO, format!("错误: {message}")))?;
    }
    execute_write_transaction_observed(dev, &plan, observer)
}

// ══════════════════════════════════════════════════════════════════
// 2. 时钟(本地时间; 进程内换算, 测试注入 FixedClock)
// ══════════════════════════════════════════════════════════════════
