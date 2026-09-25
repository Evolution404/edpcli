use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupCreateChoice {
    Metadata,
    Deep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupCreateChoiceState {
    pub selected: usize,
}

impl BackupCreateChoiceState {
    pub const fn choice(self) -> BackupCreateChoice {
        if self.selected == 0 {
            BackupCreateChoice::Metadata
        } else {
            BackupCreateChoice::Deep
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackupDeleteState {
    pub stage: WizardStage,
    pub path: std::path::PathBuf,
    pub expected_sha256: String,
    pub confirmation: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupBatchDeleteStage {
    Planning,
    Review,
    Confirm,
    Running,
    Result,
}

pub struct BackupBatchDeleteState {
    pub stage: BackupBatchDeleteStage,
    pub prepared: Option<crate::application::backup::DeletePlan>,
    pub confirmation: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupPruneStage {
    Input,
    Planning,
    Review,
    Confirm,
    Running,
    Result,
}

pub struct BackupPrunePrepared {
    pub plan: crate::application::backup::DeletePlan,
    pub keep: usize,
    pub originals: usize,
    pub retained_snapshots: usize,
}

pub struct BackupPruneState {
    pub stage: BackupPruneStage,
    pub keep_input: String,
    pub prepared: Option<BackupPrunePrepared>,
    pub confirmation: String,
    pub message: Option<String>,
}

impl AppState {
    pub fn backup_create_choice(&self) -> Option<BackupCreateChoiceState> {
        self.backup_create_choice
    }

    pub fn begin_backup_create_choice(&mut self) -> bool {
        if self.critical_operation || self.wizard.is_some() {
            self.set_notice("已有关键操作或向导正在执行。");
            return false;
        }
        if self.selected_device().is_none() {
            self.set_notice("创建备份需要先在设备页选定目标 U 盘，再进入备份页。");
            return false;
        }
        self.backup_create_choice = Some(BackupCreateChoiceState { selected: 0 });
        self.input_mode = InputMode::Normal;
        true
    }

    pub fn move_backup_create_choice(&mut self, delta: isize) {
        let Some(choice) = self.backup_create_choice.as_mut() else {
            return;
        };
        choice.selected = if delta < 0 {
            choice.selected.saturating_sub(delta.unsigned_abs())
        } else {
            choice.selected.saturating_add(delta as usize).min(1)
        };
    }

    pub fn take_backup_create_choice(&mut self) -> Option<BackupCreateChoice> {
        self.backup_create_choice
            .take()
            .map(BackupCreateChoiceState::choice)
    }

    pub fn cancel_backup_create_choice(&mut self) {
        self.backup_create_choice = None;
    }

    pub fn backup_delete(&self) -> Option<&BackupDeleteState> {
        self.backup_delete.as_ref()
    }

    pub fn backup_batch_delete(&self) -> Option<&BackupBatchDeleteState> {
        self.backup_batch_delete.as_ref()
    }

    pub fn backup_selection_count(&self) -> usize {
        self.backup_selection.len()
    }

    pub fn backup_is_selected(&self, path: &std::path::Path) -> bool {
        self.backup_selection.contains(path)
    }

    pub fn toggle_selected_backup(&mut self) {
        let Some((path, expected_sha256)) = self.selected_backup_delete_target() else {
            self.set_notice("当前备份缺少固定 SHA-256，不能加入批量删除选择。");
            return;
        };
        debug_assert!(!expected_sha256.is_empty());
        if !self.backup_selection.remove(&path) {
            self.backup_selection.insert(path);
        }
        self.set_notice(format!(
            "批量删除已勾选 {} 份备份；空格继续选择，d 进入统一删除流程。",
            self.backup_selection.len()
        ));
    }

    pub fn selected_backup_batch_targets(&self) -> Vec<(std::path::PathBuf, String)> {
        self.backups
            .iter()
            .filter(|row| self.backup_selection.contains(&row.path))
            .filter_map(|row| {
                row.content_sha256
                    .as_ref()
                    .map(|hash| (row.path.clone(), hash.clone()))
            })
            .collect()
    }

    pub fn begin_backup_batch_delete(&mut self) -> Option<Vec<(std::path::PathBuf, String)>> {
        if self.critical_operation || self.backup_batch_delete.is_some() {
            self.set_notice("已有关键操作或批量删除向导正在执行。");
            return None;
        }
        let targets = self.selected_backup_batch_targets();
        if targets.is_empty() {
            self.set_notice("先在备份页按空格勾选至少一份备份。");
            return None;
        }
        self.input_mode = InputMode::Normal;
        self.backup_batch_delete = Some(BackupBatchDeleteState {
            stage: BackupBatchDeleteStage::Planning,
            prepared: None,
            confirmation: String::new(),
            message: Some("正在新鲜扫描并逐项复核 SHA-256，生成固定删除计划…".into()),
        });
        Some(targets)
    }

    pub fn backup_batch_delete_finish_plan(
        &mut self,
        result: Result<crate::application::backup::DeletePlan, String>,
    ) {
        let Some(batch) = self.backup_batch_delete.as_mut() else {
            return;
        };
        match result {
            Ok(plan) if plan.targets.is_empty() => {
                batch.stage = BackupBatchDeleteStage::Result;
                batch.message = Some("批量删除计划为空，没有可删除目标。".into());
            }
            Ok(plan) => {
                batch.prepared = Some(plan);
                batch.stage = BackupBatchDeleteStage::Review;
                batch.message = None;
            }
            Err(message) => {
                batch.stage = BackupBatchDeleteStage::Result;
                batch.message = Some(message);
            }
        }
    }

    pub fn backup_batch_delete_begin_confirm(&mut self) {
        if let Some(batch) = self.backup_batch_delete.as_mut() {
            if batch.stage == BackupBatchDeleteStage::Review {
                batch.stage = BackupBatchDeleteStage::Confirm;
                batch.confirmation.clear();
                batch.message = None;
                self.input_mode = InputMode::Confirm;
            }
        }
    }

    pub fn backup_batch_delete_push_confirmation(&mut self, ch: char) {
        if let Some(batch) = self.backup_batch_delete.as_mut() {
            if batch.stage == BackupBatchDeleteStage::Confirm && batch.confirmation.len() < 16 {
                batch.confirmation.push(ch);
                batch.message = None;
            }
        }
    }

    pub fn backup_batch_delete_backspace(&mut self) {
        if let Some(batch) = self.backup_batch_delete.as_mut() {
            if batch.stage == BackupBatchDeleteStage::Confirm {
                batch.confirmation.pop();
                batch.message = None;
            }
        }
    }

    pub fn backup_batch_delete_take_for_execute(
        &mut self,
    ) -> Option<crate::application::backup::DeletePlan> {
        let batch = self.backup_batch_delete.as_mut()?;
        if batch.stage != BackupBatchDeleteStage::Confirm {
            return None;
        }
        if batch.confirmation != "YES" {
            batch.message = Some("必须精确输入 YES 才会批量删除备份。".into());
            return None;
        }
        let plan = batch.prepared.take()?;
        batch.stage = BackupBatchDeleteStage::Running;
        batch.message = Some("正在按固定计划逐条复核并删除…".into());
        self.input_mode = InputMode::Normal;
        self.critical_operation = true;
        Some(plan)
    }

    pub fn backup_batch_delete_finish_execute(&mut self, result: Result<usize, String>) {
        self.critical_operation = false;
        let success = result.is_ok();
        if let Some(batch) = self.backup_batch_delete.as_mut() {
            batch.stage = BackupBatchDeleteStage::Result;
            batch.message = Some(match result {
                Ok(count) => format!("批量删除完成：已安全删除 {count} 份备份。"),
                Err(message) => message,
            });
        }
        if success {
            self.backup_selection.clear();
        }
    }

    pub fn close_backup_batch_delete(&mut self) {
        if !self.critical_operation {
            self.backup_batch_delete = None;
            self.input_mode = InputMode::Normal;
        }
    }

    pub fn backup_prune(&self) -> Option<&BackupPruneState> {
        self.backup_prune.as_ref()
    }

    pub fn backup_prune_mut(&mut self) -> Option<&mut BackupPruneState> {
        self.backup_prune.as_mut()
    }

    pub fn begin_backup_prune(&mut self) -> bool {
        if self.critical_operation || self.backup_prune.is_some() {
            return false;
        }
        self.input_mode = InputMode::Insert;
        self.backup_prune = Some(BackupPruneState {
            stage: BackupPruneStage::Input,
            keep_input: "3".into(),
            prepared: None,
            confirmation: String::new(),
            message: None,
        });
        true
    }

    pub fn backup_prune_push_digit(&mut self, ch: char) {
        if let Some(prune) = self.backup_prune.as_mut() {
            if prune.stage == BackupPruneStage::Input
                && ch.is_ascii_digit()
                && prune.keep_input.len() < 6
            {
                prune.keep_input.push(ch);
                prune.message = None;
            }
        }
    }

    pub fn backup_prune_backspace(&mut self) {
        if let Some(prune) = self.backup_prune.as_mut() {
            match prune.stage {
                BackupPruneStage::Input => {
                    prune.keep_input.pop();
                    prune.message = None;
                }
                BackupPruneStage::Confirm => {
                    prune.confirmation.pop();
                    prune.message = None;
                }
                _ => {}
            }
        }
    }

    pub fn backup_prune_start_plan(&mut self) -> Result<usize, String> {
        let prune = self
            .backup_prune
            .as_mut()
            .ok_or_else(|| "清理向导未打开".to_string())?;
        let keep = prune
            .keep_input
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| "保留份数必须为大于 0 的整数".to_string())?;
        prune.stage = BackupPruneStage::Planning;
        prune.message = Some("正在后台扫描备份并生成固定清理计划…".into());
        self.input_mode = InputMode::Normal;
        Ok(keep)
    }

    pub fn backup_prune_finish_plan(&mut self, result: Result<BackupPrunePrepared, String>) {
        let Some(prune) = self.backup_prune.as_mut() else {
            return;
        };
        match result {
            Ok(prepared) if prepared.plan.targets.is_empty() => {
                prune.prepared = Some(prepared);
                prune.stage = BackupPruneStage::Result;
                prune.message = Some("无需清理：当前备份已经满足保留策略。".into());
                self.input_mode = InputMode::Normal;
            }
            Ok(prepared) => {
                prune.prepared = Some(prepared);
                prune.stage = BackupPruneStage::Review;
                prune.message = None;
                self.input_mode = InputMode::Normal;
            }
            Err(message) => {
                prune.stage = BackupPruneStage::Input;
                prune.message = Some(message);
                self.input_mode = InputMode::Insert;
            }
        }
    }

    pub fn backup_prune_begin_confirm(&mut self) {
        if let Some(prune) = self.backup_prune.as_mut() {
            if prune.stage == BackupPruneStage::Review {
                prune.stage = BackupPruneStage::Confirm;
                prune.confirmation.clear();
                prune.message = None;
                self.input_mode = InputMode::Confirm;
            }
        }
    }

    pub fn backup_prune_push_confirmation(&mut self, ch: char) {
        if let Some(prune) = self.backup_prune.as_mut() {
            if prune.stage == BackupPruneStage::Confirm && prune.confirmation.len() < 16 {
                prune.confirmation.push(ch);
                prune.message = None;
            }
        }
    }

    pub fn backup_prune_take_for_execute(&mut self) -> Option<BackupPrunePrepared> {
        let prune = self.backup_prune.as_mut()?;
        if prune.stage != BackupPruneStage::Confirm {
            return None;
        }
        if prune.confirmation != "YES" {
            prune.message = Some("必须精确输入 YES 才会删除备份".into());
            return None;
        }
        let prepared = prune.prepared.take()?;
        prune.stage = BackupPruneStage::Running;
        prune.message = Some("正在逐条复核摘要并清理固定候选…".into());
        self.input_mode = InputMode::Normal;
        self.critical_operation = true;
        Some(prepared)
    }

    pub fn backup_prune_finish_execute(&mut self, result: Result<usize, String>) {
        self.critical_operation = false;
        if let Some(prune) = self.backup_prune.as_mut() {
            prune.stage = BackupPruneStage::Result;
            prune.message = Some(match result {
                Ok(count) => format!("清理完成：已安全删除 {count} 份旧备份。"),
                Err(message) => message,
            });
        }
    }

    pub fn close_backup_prune(&mut self) {
        if !self.critical_operation {
            self.backup_prune = None;
            self.input_mode = InputMode::Normal;
        }
    }

    pub fn begin_backup_delete(
        &mut self,
        path: std::path::PathBuf,
        expected_sha256: String,
    ) -> bool {
        if self.critical_operation {
            self.set_notice("关键操作仍在执行，完成前不能启动其他任务。".to_string());
            return false;
        }
        self.input_mode = InputMode::Confirm;
        self.backup_delete = Some(BackupDeleteState {
            stage: WizardStage::Confirm,
            path,
            expected_sha256,
            confirmation: String::new(),
            message: None,
        });
        true
    }

    pub fn push_backup_delete_confirmation(&mut self, ch: char) {
        if let Some(delete) = self.backup_delete.as_mut() {
            if delete.stage == WizardStage::Confirm && delete.confirmation.len() < 16 {
                delete.confirmation.push(ch);
                delete.message = None;
            }
        }
    }

    pub fn backspace_backup_delete_confirmation(&mut self) {
        if let Some(delete) = self.backup_delete.as_mut() {
            if delete.stage == WizardStage::Confirm {
                delete.confirmation.pop();
                delete.message = None;
            }
        }
    }

    pub fn submit_backup_delete_confirmation(&mut self) -> Option<(std::path::PathBuf, String)> {
        let delete = self.backup_delete.as_mut()?;
        if delete.stage != WizardStage::Confirm {
            return None;
        }
        if delete.confirmation != "YES" {
            delete.message = Some("必须精确输入 YES 才会删除备份".to_string());
            return None;
        }
        delete.stage = WizardStage::Running;
        delete.message = Some("正在复核文件内容并删除备份…".to_string());
        self.input_mode = InputMode::Normal;
        self.critical_operation = true;
        Some((delete.path.clone(), delete.expected_sha256.clone()))
    }

    pub fn finish_backup_delete(&mut self, result: Result<(), String>) {
        self.critical_operation = false;
        if let Some(delete) = self.backup_delete.as_mut() {
            delete.stage = WizardStage::Result;
            delete.message = Some(match result {
                Ok(()) => "备份已删除；列表已刷新".to_string(),
                Err(message) => message,
            });
        }
    }

    pub const fn workspace(&self) -> Workspace {
        self.workspace
    }

    pub const fn provision(&self) -> &ProvisionState {
        &self.provision
    }
}
