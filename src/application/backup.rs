//! 备份删除/清理的唯一 application 命令：保留规则、确认快照与执行。
//!
//! CLI delete/prune 与 TUI delete 都经 `DeleteSession` 进入：open 固定一次扫描，
//! plan_* 解析目标并执行统一的“至少保留 1 份”底线后返回固定快照 DeletePlan，
//! execute 只信这份快照逐条 `delete_entry_verified`。渲染、交互与退出码是前端
//! 契约，不在此层。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::backup_catalog::{self, canonical_entry_path};
use crate::diskio::{self, BackupEntry};
use crate::selectors::BackupSelector;

/// 一次扫描会话。目录不存在时沿用 BackupCatalog 的空目录语义；
/// “备份目录不存在”的呈现与退出码属 CLI 前端契约，不在此判断。
pub struct DeleteSession {
    selector: BackupSelector,
}

/// prune 预览统计；delete 路径不填。
pub struct PruneStats {
    pub originals: usize,
    pub retained_snapshots: usize,
}

/// 固定快照的删除计划。targets 为确认前克隆的条目(含 content_sha256)，
/// execute 阶段只按这份快照复核，不重新解释目标。
pub struct DeletePlan {
    pub targets: Vec<BackupEntry>,
    pub prune_stats: Option<PruneStats>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletePlanError {
    /// 目标解析失败(编号/文件名/范围)，携带原始消息。
    Resolve(String),
    /// 固定路径在新鲜扫描中已不存在。
    Vanished { path: PathBuf },
    /// 固定内容摘要与新鲜扫描不符。
    Changed { path: PathBuf },
    /// 保留底线：本次删除会使某同盘组清零。
    RetentionFloor,
}

impl DeletePlanError {
    pub fn message(&self) -> String {
        match self {
            DeletePlanError::Resolve(message) => message.clone(),
            DeletePlanError::Vanished { path } => {
                format!("备份已不存在或不再属于当前备份目录: {}", path.display())
            }
            DeletePlanError::Changed { path } => {
                format!("备份在选择/确认期间已变化，拒绝删除: {}", path.display())
            }
            DeletePlanError::RetentionFloor => {
                "安全保护拒绝删除——该盘将被清到零份备份；至少保留 1 份。".into()
            }
        }
    }
}

impl DeleteSession {
    pub fn open(root: &Path) -> Self {
        DeleteSession {
            selector: BackupSelector::load(root),
        }
    }

    /// CLI 编号视图与交互选择共用同一份扫描。
    pub fn selector(&self) -> &BackupSelector {
        &self.selector
    }

    fn entries(&self) -> &[BackupEntry] {
        self.selector.catalog().entries()
    }

    /// 显式目标(编号/文件名/范围) → 解析 + 保留底线 → 计划。
    pub fn plan_targets(&self, targets: &[String]) -> Result<DeletePlan, DeletePlanError> {
        let selected = self
            .selector
            .resolve_many(targets)
            .map_err(DeletePlanError::Resolve)?;
        self.plan_resolved(selected.into_iter().cloned().collect())
    }

    /// TUI 固定的 (path, content_sha256)：新鲜扫描内解析 + 摘要复核 + 保留底线 → 计划。
    pub fn plan_exact(
        &self,
        path: &Path,
        expected_sha256: &str,
    ) -> Result<DeletePlan, DeletePlanError> {
        let entry = scanned_backup_by_path(&self.selector, path).map_err(|_| {
            DeletePlanError::Vanished {
                path: path.to_path_buf(),
            }
        })?;
        if entry.content_sha256.as_deref() != Some(expected_sha256) {
            return Err(DeletePlanError::Changed {
                path: path.to_path_buf(),
            });
        }
        self.plan_resolved(vec![entry.clone()])
    }

    /// TUI 批量固定的 (path, content_sha256) → 新鲜扫描逐项复核 → 统一保留底线。
    pub fn plan_exact_many(
        &self,
        targets: &[(PathBuf, String)],
    ) -> Result<DeletePlan, DeletePlanError> {
        let mut selected = Vec::with_capacity(targets.len());
        let mut seen = BTreeSet::new();
        for (path, expected_sha256) in targets {
            let canonical = canonical_entry_path(path);
            if !seen.insert(canonical.clone()) {
                continue;
            }
            let entry = scanned_backup_by_path(&self.selector, &canonical).map_err(|_| {
                DeletePlanError::Vanished {
                    path: path.to_path_buf(),
                }
            })?;
            if entry.content_sha256.as_deref() != Some(expected_sha256.as_str()) {
                return Err(DeletePlanError::Changed {
                    path: path.to_path_buf(),
                });
            }
            selected.push(entry.clone());
        }
        self.plan_resolved(selected)
    }

    /// keep-N 清理：prune_candidates 纯策略 + 组装条目 + 统计。
    /// prune_candidates 自带“无原盘组至少留 1”规则，底线复核仅作纵深防御。
    pub fn plan_prune(&self, keep: usize) -> Result<DeletePlan, DeletePlanError> {
        let all: Vec<BackupEntry> = self.entries().to_vec();
        let candidate_paths = diskio::prune_candidates(&all, keep);
        // 按 prune_candidates 给出的候选顺序(最旧→较新)组装，与旧 CLI 预览顺序一致。
        let targets: Vec<BackupEntry> = candidate_paths
            .iter()
            .map(|path| {
                let canonical = canonical_entry_path(path);
                all.iter()
                    .find(|entry| canonical_entry_path(&entry.path) == canonical)
                    .cloned()
            })
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default();
        let originals = all
            .iter()
            .filter(|entry| entry.meta.is_some() && !entry.is_nopwd)
            .count();
        let snapshots = all
            .iter()
            .filter(|entry| entry.meta.is_some() && entry.is_nopwd)
            .count();
        let plan = DeletePlan {
            targets,
            prune_stats: Some(PruneStats {
                originals,
                retained_snapshots: snapshots.saturating_sub(candidate_paths.len()),
            }),
        };
        enforce_retention_floor(self.entries(), &plan.targets)?;
        Ok(plan)
    }

    /// 已解析条目(前端交互选择的结果) → 统一保留底线 → 计划。
    pub fn plan_resolved(&self, selected: Vec<BackupEntry>) -> Result<DeletePlan, DeletePlanError> {
        enforce_retention_floor(self.entries(), &selected)?;
        Ok(DeletePlan {
            targets: selected,
            prune_stats: None,
        })
    }

    /// 逐条 delete_entry_verified；单条失败不阻断后续，呈现与退出码由前端决定。
    pub fn execute(&self, plan: &DeletePlan) -> Vec<(PathBuf, Result<(), String>)> {
        plan.targets
            .iter()
            .map(|entry| {
                let path = canonical_entry_path(&entry.path);
                let result = backup_catalog::delete_entry_verified(entry);
                (path, result)
            })
            .collect()
    }
}

fn scanned_backup_by_path<'a>(
    selector: &'a BackupSelector,
    path: &Path,
) -> Result<&'a BackupEntry, String> {
    let target = canonical_entry_path(path);
    selector
        .catalog()
        .entries()
        .iter()
        .find(|entry| canonical_entry_path(&entry.path) == target)
        .ok_or_else(|| format!("备份已不存在或不再属于当前备份目录: {}", path.display()))
}

/// 单一保留底线：任一同盘组“删除数 ≥ 组内总数且总数 > 0”即拒绝。
/// 与旧 CLI 批视角(backup_cli)及旧 TUI 单删视角语义等价：单删时两式相同，
/// 均为“组内仅剩 1 份时拒绝”。
fn enforce_retention_floor(
    all: &[BackupEntry],
    targets: &[BackupEntry],
) -> Result<(), DeletePlanError> {
    let deleting: BTreeSet<PathBuf> = targets
        .iter()
        .map(|entry| canonical_entry_path(&entry.path))
        .collect();
    let mut total_per_group: BTreeMap<String, usize> = BTreeMap::new();
    let mut deleting_per_group: BTreeMap<String, usize> = BTreeMap::new();
    for entry in all {
        if let Some(key) = diskio::backup_group_key(entry) {
            *total_per_group.entry(key.clone()).or_default() += 1;
            if deleting.contains(&canonical_entry_path(&entry.path)) {
                *deleting_per_group.entry(key).or_default() += 1;
            }
        }
    }
    for (key, deleting_count) in &deleting_per_group {
        let total = total_per_group.get(key).copied().unwrap_or(0);
        if *deleting_count >= total && total > 0 {
            return Err(DeletePlanError::RetentionFloor);
        }
    }
    Ok(())
}

/// TUI 单删入口的兼容薄封装：一次新鲜扫描 → plan_exact → execute。
pub fn delete_backup_exact(root: &Path, path: &Path, expected_sha256: &str) -> Result<(), String> {
    let session = DeleteSession::open(root);
    let plan = session
        .plan_exact(path, expected_sha256)
        .map_err(|error| error.message())?;
    match session.execute(&plan).into_iter().next() {
        Some((_, Ok(()))) => Ok(()),
        Some((_, Err(message))) => Err(message),
        None => Err("删除计划为空".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diskio::{BackupMeta, Sha256Status};

    fn entry(name: &str, onlyid: &str, is_nopwd: bool) -> BackupEntry {
        BackupEntry {
            meta: Some(BackupMeta {
                disk: 6,
                secs: Some(122880000),
                vid: "0dd8".into(),
                pid: "2005".into(),
                device_id: "disk&ven_netac&prod_onlydisk".into(),
                onlyid: Some(onlyid.into()),
                tagged_nopwd: is_nopwd,
            }),
            path: PathBuf::from(name),
            mtime: 1,
            is_nopwd,
            provision_kind: crate::provision::DiskProvisionKind::Plain,
            sha256_ok: Sha256Status::Ok,
            size_ok: true,
            lba8: None,
            content_sha256: None,
        }
    }

    fn floor_err(all: &[BackupEntry], targets: &[BackupEntry]) -> bool {
        matches!(
            enforce_retention_floor(all, targets),
            Err(DeletePlanError::RetentionFloor)
        )
    }

    #[test]
    fn retention_floor_refuses_emptying_any_group() {
        // A 组: 加密原盘 + 2 快照; B 组: 2 快照; C 组: 仅 1 份。
        let all = vec![
            entry("a-orig.bin", "A", false),
            entry("a-n1.bin", "A", true),
            entry("a-n2.bin", "A", true),
            entry("b-n1.bin", "B", true),
            entry("b-n2.bin", "B", true),
            entry("c-1.bin", "C", true),
        ];
        // 组内仅剩 1 份: 单删拒绝(旧 TUI 单删视角)。
        assert!(floor_err(&all, &[all[5].clone()]));
        // 组内 2 份: 删 1 允许。
        assert!(!floor_err(&all, &[all[4].clone()]));
        // 批删整组拒绝(旧 CLI 批视角)。
        assert!(floor_err(&all, &[all[3].clone(), all[4].clone()]));
        assert!(floor_err(
            &all,
            &[all[0].clone(), all[1].clone(), all[2].clone()]
        ));
        // 跨组子集: 每组都不清零则允许。
        assert!(!floor_err(&all, &[all[1].clone(), all[3].clone()]));
        // 目标不在扫描集合(理论上不发生)不误伤其他组。
        assert!(!floor_err(&all, &[entry("ghost.bin", "Z", true)]));
    }

    #[test]
    fn floor_error_message_keeps_frontend_contract() {
        assert_eq!(
            DeletePlanError::RetentionFloor.message(),
            "安全保护拒绝删除——该盘将被清到零份备份；至少保留 1 份。"
        );
        assert_eq!(
            DeletePlanError::Vanished {
                path: PathBuf::from("/b/x.bin")
            }
            .message(),
            "备份已不存在或不再属于当前备份目录: /b/x.bin"
        );
        assert_eq!(
            DeletePlanError::Changed {
                path: PathBuf::from("/b/x.bin")
            }
            .message(),
            "备份在选择/确认期间已变化，拒绝删除: /b/x.bin"
        );
    }
}
