use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BackupDeviceFilter {
    #[default]
    All,
    Confirmed(String),
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupDeviceTreeNode {
    pub filter: BackupDeviceFilter,
    pub label: String,
    pub count: usize,
    pub depth: usize,
}

#[derive(Debug, Clone)]
pub struct BackupDeleteState {
    pub stage: WizardStage,
    pub path: std::path::PathBuf,
    pub expected_sha256: String,
    pub message: Option<crate::tui::ui::UiMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupBatchDeleteStage {
    Planning,
    Confirm,
    Running,
}

pub struct BackupBatchDeleteState {
    pub stage: BackupBatchDeleteStage,
    pub prepared: Option<crate::application::backup::DeletePlan>,
    pub message: Option<crate::tui::ui::UiMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupPruneStage {
    Input,
    Planning,
    Confirm,
    Running,
}

pub struct BackupPrunePrepared {
    pub plan: crate::application::backup::DeletePlan,
    pub keep: usize,
    pub managed_backups: usize,
    pub retained_backups: usize,
}

pub struct BackupPruneState {
    pub stage: BackupPruneStage,
    pub keep_input: String,
    pub prepared: Option<BackupPrunePrepared>,
    pub message: Option<crate::tui::ui::UiMessage>,
}

#[derive(Debug, Clone)]
pub struct BackupVerifyRunState {
    pub path: std::path::PathBuf,
    pub latest: crate::application::progress::ProgressEvent,
    pub log: std::collections::VecDeque<crate::application::progress::ProgressEvent>,
}

pub struct BackupsState {
    pub(super) rows: Vec<crate::application::BackupWorkspaceItem>,
    pub(super) verify_run: Option<BackupVerifyRunState>,
    pub(super) table_view: super::super::table_layout::TableViewData,
    pub(super) scan_pending: bool,
    pub(super) delete: Option<BackupDeleteState>,
    pub(super) batch_delete: Option<BackupBatchDeleteState>,
    pub(super) selection: std::collections::BTreeSet<std::path::PathBuf>,
    pub(super) prune: Option<BackupPruneState>,
    pub(super) device_filter: BackupDeviceFilter,
    pub(super) device_tree_selected: usize,
    pub(super) device_tree_scroll_x: usize,
    pub(super) device_tree_expanded: bool,
    pub(super) pane_focus: crate::tui::pane::PaneFocus,
}

impl Default for BackupsState {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            verify_run: None,
            table_view: super::super::table_layout::TableViewData::default(),
            scan_pending: false,
            delete: None,
            batch_delete: None,
            selection: std::collections::BTreeSet::new(),
            prune: None,
            device_filter: BackupDeviceFilter::All,
            device_tree_selected: 0,
            device_tree_scroll_x: 0,
            device_tree_expanded: true,
            pane_focus: crate::tui::pane::PaneFocus::backups(),
        }
    }
}

impl AppState {
    fn backup_strong_group_key(backup: &crate::application::BackupWorkspaceItem) -> Option<String> {
        backup.identity.as_ref()?.strong_backup_group_key()
    }

    fn backup_group_display_label(backup: &crate::application::BackupWorkspaceItem) -> String {
        use crate::media_identity::SerialQuality;

        let view = crate::application::identity::WorkspaceIdentity::from_backup(backup);
        let model = view.model();
        let identity = backup.identity.as_ref();
        let suffix = identity
            .filter(|identity| identity.hardware.serial_quality == SerialQuality::Usable)
            .and_then(|identity| identity.hardware.serial.as_deref())
            .map(|serial| {
                let trimmed = serial.trim();
                if trimmed.chars().count() > 12 {
                    trimmed.chars().take(12).collect::<String>()
                } else {
                    trimmed.to_string()
                }
            })
            .or_else(|| backup.onlyid.clone())
            .or_else(|| {
                let vid_pid = view.vid_pid();
                (vid_pid != "—").then_some(vid_pid)
            });
        let base = match (model.as_str(), suffix) {
            ("—", Some(suffix)) => format!("设备 · {suffix}"),
            ("—", None) => "已确认设备".into(),
            (_, Some(suffix)) => format!("{model} · {suffix}"),
            (_, None) => model,
        };
        backup
            .size_bytes
            .map(crate::common::fmt_capacity)
            .map(|capacity| format!("{base} · {capacity}"))
            .unwrap_or(base)
    }

    fn disambiguate_backup_group_labels(groups: &mut [(String, String, usize)]) {
        let mut label_counts = std::collections::BTreeMap::<String, usize>::new();
        for (_, label, _) in groups.iter() {
            *label_counts.entry(label.clone()).or_default() += 1;
        }
        for (key, label, _) in groups.iter_mut() {
            if label_counts.get(label).copied().unwrap_or_default() > 1 {
                let digest = crate::sha256::sha256_hex(key.as_bytes());
                *label = format!("#{} · {label}", &digest[..6]);
            }
        }
    }

    pub fn backup_device_tree_nodes(&self) -> Vec<BackupDeviceTreeNode> {
        let mut nodes = vec![BackupDeviceTreeNode {
            filter: BackupDeviceFilter::All,
            label: "全部备份".into(),
            count: self.backups.rows.len(),
            depth: 0,
        }];
        if !self.backups.device_tree_expanded {
            return nodes;
        }

        let mut groups: Vec<(String, String, usize)> = Vec::new();
        let mut positions = std::collections::BTreeMap::<String, usize>::new();
        let mut unresolved = 0usize;
        for backup in &self.backups.rows {
            let Some(key) = Self::backup_strong_group_key(backup) else {
                unresolved += 1;
                continue;
            };
            if let Some(index) = positions.get(&key).copied() {
                groups[index].2 += 1;
            } else {
                positions.insert(key.clone(), groups.len());
                groups.push((key, Self::backup_group_display_label(backup), 1));
            }
        }
        Self::disambiguate_backup_group_labels(&mut groups);
        nodes.extend(
            groups
                .into_iter()
                .map(|(key, label, count)| BackupDeviceTreeNode {
                    filter: BackupDeviceFilter::Confirmed(key),
                    label,
                    count,
                    depth: 1,
                }),
        );
        if unresolved > 0 {
            nodes.push(BackupDeviceTreeNode {
                filter: BackupDeviceFilter::Unresolved,
                label: "身份未确认".into(),
                count: unresolved,
                depth: 1,
            });
        }
        nodes
    }

    pub fn backup_device_filter(&self) -> &BackupDeviceFilter {
        &self.backups.device_filter
    }

    pub fn backup_device_filter_active(&self) -> bool {
        !matches!(self.backups.device_filter, BackupDeviceFilter::All)
    }

    pub fn backup_matches_device_filter(
        &self,
        backup: &crate::application::BackupWorkspaceItem,
    ) -> bool {
        match &self.backups.device_filter {
            BackupDeviceFilter::All => true,
            BackupDeviceFilter::Confirmed(expected) => {
                Self::backup_strong_group_key(backup).as_ref() == Some(expected)
            }
            BackupDeviceFilter::Unresolved => Self::backup_strong_group_key(backup).is_none(),
        }
    }

    pub fn backup_device_filtered_count(&self) -> usize {
        self.backups
            .rows
            .iter()
            .filter(|backup| self.backup_matches_device_filter(backup))
            .count()
    }

    pub fn backup_device_filter_label(&self) -> String {
        match &self.backups.device_filter {
            BackupDeviceFilter::All => "全部备份".into(),
            BackupDeviceFilter::Unresolved => "身份未确认".into(),
            BackupDeviceFilter::Confirmed(key) => self
                .backups
                .rows
                .iter()
                .find(|backup| Self::backup_strong_group_key(backup).as_ref() == Some(key))
                .map(Self::backup_group_display_label)
                .unwrap_or_else(|| "已确认设备".into()),
        }
    }

    pub fn backup_device_tree_selected(&self) -> usize {
        self.backups.device_tree_selected
    }

    pub fn backup_device_tree_expanded(&self) -> bool {
        self.backups.device_tree_expanded
    }

    pub(crate) fn backup_device_tree_row_parts(
        &self,
        index: usize,
    ) -> Option<(&'static str, String, usize)> {
        let nodes = self.backup_device_tree_nodes();
        let node = nodes.get(index)?;
        if node.depth == 0 {
            if self.backups.device_tree_expanded {
                Some(("▾ ", node.label.clone(), node.count))
            } else if self.backup_device_filter_active() {
                Some((
                    "▸ ",
                    self.backup_device_filter_label(),
                    self.backup_device_filtered_count(),
                ))
            } else {
                Some(("▸ ", node.label.clone(), node.count))
            }
        } else {
            Some(("  ", node.label.clone(), node.count))
        }
    }

    fn backup_device_tree_active_width(&self) -> usize {
        let selected = self.backup_device_tree_selected();
        let Some((prefix, label, _count)) = self.backup_device_tree_row_parts(selected) else {
            return 0;
        };
        crate::tui::table_layout::display_width(prefix)
            .saturating_add(crate::tui::table_layout::display_width(&label))
    }

    pub fn backup_device_tree_count_width(&self) -> usize {
        self.backup_device_tree_nodes()
            .iter()
            .map(|node| node.count.to_string().len())
            .max()
            .unwrap_or(1)
            .max(3)
    }

    pub fn backup_device_tree_scroll_offset(&self) -> usize {
        self.backups.device_tree_scroll_x
    }

    pub fn scroll_backup_device_tree(&mut self, reverse: bool, terminal_width: u16) -> bool {
        let inner_width = usize::from(
            crate::tui::ui::ViewportClass::for_width(terminal_width)
                .backup_device_tree_inner_width(terminal_width),
        );
        let marker_width = 2usize;
        let count_width = self.backup_device_tree_count_width();
        let gap_width = 1usize;
        let viewport_width = inner_width
            .saturating_sub(marker_width)
            .saturating_sub(count_width)
            .saturating_sub(gap_width)
            .max(1);
        let max_scroll = self
            .backup_device_tree_active_width()
            .saturating_sub(viewport_width);
        let current = self.backups.device_tree_scroll_x.min(max_scroll);
        let next = if reverse {
            current.saturating_sub(2)
        } else {
            current.saturating_add(2).min(max_scroll)
        };
        let changed = next != self.backups.device_tree_scroll_x;
        self.backups.device_tree_scroll_x = next;
        changed
    }

    fn apply_backup_device_tree_selection(&mut self) {
        let nodes = self.backup_device_tree_nodes();
        let Some(node) = nodes.get(self.backups.device_tree_selected) else {
            return;
        };
        self.backups.device_filter = node.filter.clone();
        self.backups.device_tree_scroll_x = 0;
        self.shell.selected = 0;
        self.set_item_count(self.visible_backup_indices().len());
    }

    pub(super) fn follow_backup_device_group(&mut self, group_key: Option<String>) {
        self.backups.device_filter = group_key
            .filter(|expected| {
                self.backups
                    .rows
                    .iter()
                    .any(|backup| Self::backup_strong_group_key(backup).as_ref() == Some(expected))
            })
            .map(BackupDeviceFilter::Confirmed)
            .unwrap_or(BackupDeviceFilter::All);
        self.backups.device_tree_scroll_x = 0;
        self.reconcile_backup_device_filter();
        self.shell.selected = 0;
        self.set_item_count(self.visible_backup_indices().len());
    }

    pub(super) fn reconcile_backup_device_filter(&mut self) {
        let valid = match &self.backups.device_filter {
            BackupDeviceFilter::All => true,
            BackupDeviceFilter::Confirmed(expected) => self
                .backups
                .rows
                .iter()
                .any(|backup| Self::backup_strong_group_key(backup).as_ref() == Some(expected)),
            BackupDeviceFilter::Unresolved => self
                .backups
                .rows
                .iter()
                .any(|backup| Self::backup_strong_group_key(backup).is_none()),
        };
        if !valid {
            self.backups.device_filter = BackupDeviceFilter::All;
        }
        if self.backups.device_tree_expanded {
            let nodes = self.backup_device_tree_nodes();
            self.backups.device_tree_selected = nodes
                .iter()
                .position(|node| node.filter == self.backups.device_filter)
                .unwrap_or(0);
        } else {
            self.backups.device_tree_selected = 0;
        }
    }

    pub fn backup_device_tree_move(&mut self, delta: isize) {
        if !self.backups.device_tree_expanded {
            self.backups.device_tree_selected = 0;
            return;
        }
        let nodes = self.backup_device_tree_nodes();
        if nodes.is_empty() {
            return;
        }
        let current = self.backups.device_tree_selected.min(nodes.len() - 1);
        self.backups.device_tree_selected = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize).min(nodes.len() - 1)
        };
        self.apply_backup_device_tree_selection();
    }

    pub fn backup_device_tree_jump(&mut self, to_end: bool) {
        if !self.backups.device_tree_expanded {
            self.backups.device_tree_selected = 0;
            return;
        }
        let nodes = self.backup_device_tree_nodes();
        if nodes.is_empty() {
            return;
        }
        self.backups.device_tree_selected = if to_end { nodes.len() - 1 } else { 0 };
        self.apply_backup_device_tree_selection();
    }

    pub fn backup_device_tree_toggle(&mut self) {
        self.backups.device_tree_expanded = !self.backups.device_tree_expanded;
        self.backups.device_tree_scroll_x = 0;
        if self.backups.device_tree_expanded {
            self.reconcile_backup_device_filter();
        } else {
            self.backups.device_tree_selected = 0;
        }
    }

    pub fn backup_device_tree_focus_list(&mut self) {
        self.backups
            .pane_focus
            .focus(crate::tui::pane::PaneId::BackupsList);
    }

    pub fn backup_delete(&self) -> Option<&BackupDeleteState> {
        self.backups.delete.as_ref()
    }

    pub fn backup_batch_delete(&self) -> Option<&BackupBatchDeleteState> {
        self.backups.batch_delete.as_ref()
    }

    pub fn backup_selection_count(&self) -> usize {
        self.backups.selection.len()
    }

    pub fn backup_is_selected(&self, path: &std::path::Path) -> bool {
        self.backups.selection.contains(path)
    }

    pub fn toggle_selected_backup(&mut self) {
        let Some((path, expected_sha256)) = self.selected_backup_delete_target() else {
            self.set_warning_notice("当前备份缺少固定 SHA-256，不能加入批量删除选择。");
            return;
        };
        debug_assert!(!expected_sha256.is_empty());
        if !self.backups.selection.remove(&path) {
            self.backups.selection.insert(path);
        }
        self.set_success_notice(format!(
            "批量删除已勾选 {} 份备份；空格继续选择，d 进入统一删除流程。",
            self.backups.selection.len()
        ));
    }

    pub fn selected_backup_batch_targets(&self) -> Vec<(std::path::PathBuf, String)> {
        self.backups
            .rows
            .iter()
            .filter(|row| self.backups.selection.contains(&row.path))
            .filter_map(|row| {
                row.content_sha256
                    .as_ref()
                    .map(|hash| (row.path.clone(), hash.clone()))
            })
            .collect()
    }

    pub fn begin_backup_batch_delete(&mut self) -> Option<Vec<(std::path::PathBuf, String)>> {
        if self.shell.critical_operation || self.backups.batch_delete.is_some() {
            self.set_warning_notice("已有关键操作或批量删除向导正在执行。");
            return None;
        }
        let targets = self.selected_backup_batch_targets();
        if targets.is_empty() {
            self.set_warning_notice("先在备份页按空格勾选至少一份备份。");
            return None;
        }
        self.shell.input_mode = InputMode::Normal;
        self.backups.batch_delete = Some(BackupBatchDeleteState {
            stage: BackupBatchDeleteStage::Planning,
            prepared: None,
            message: Some(crate::tui::ui::UiMessage::progress(
                "正在新鲜扫描并逐项复核 SHA-256，生成固定删除计划…",
            )),
        });
        Some(targets)
    }

    pub fn backup_batch_delete_finish_plan(
        &mut self,
        result: Result<crate::application::backup::DeletePlan, String>,
    ) {
        match result {
            Ok(plan) if plan.targets.is_empty() => {
                self.backups.batch_delete = None;
                self.set_notice("批量删除计划为空，没有可删除目标。");
            }
            Ok(plan) => {
                if let Some(batch) = self.backups.batch_delete.as_mut() {
                    batch.prepared = Some(plan);
                    batch.stage = BackupBatchDeleteStage::Confirm;
                    batch.message = None;
                    self.shell.input_mode = InputMode::Normal;
                }
            }
            Err(message) => {
                self.backups.batch_delete = None;
                self.set_error_notice(message);
            }
        }
    }

    pub fn backup_batch_delete_take_for_execute(
        &mut self,
    ) -> Option<crate::application::backup::DeletePlan> {
        let batch = self.backups.batch_delete.as_mut()?;
        if batch.stage != BackupBatchDeleteStage::Confirm {
            return None;
        }
        let plan = batch.prepared.take()?;
        batch.stage = BackupBatchDeleteStage::Running;
        batch.message = Some(crate::tui::ui::UiMessage::progress(
            "正在按固定计划逐条复核并删除…",
        ));
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = true;
        Some(plan)
    }

    pub fn backup_batch_delete_finish_execute(&mut self, result: Result<usize, String>) {
        self.shell.critical_operation = false;
        let success = result.is_ok();
        let message = match result {
            Ok(count) => format!("批量删除完成：已安全删除 {count} 份备份。"),
            Err(message) => message,
        };
        self.backups.batch_delete = None;
        if success {
            self.backups.selection.clear();
            self.set_success_notice(message);
        } else {
            self.set_error_notice(message);
        }
    }

    pub fn close_backup_batch_delete(&mut self) {
        if !self.shell.critical_operation {
            self.backups.batch_delete = None;
            self.shell.input_mode = InputMode::Normal;
        }
    }

    pub fn backup_prune(&self) -> Option<&BackupPruneState> {
        self.backups.prune.as_ref()
    }

    pub fn backup_prune_mut(&mut self) -> Option<&mut BackupPruneState> {
        self.backups.prune.as_mut()
    }

    pub fn begin_backup_prune(&mut self) -> bool {
        if self.shell.critical_operation || self.backups.prune.is_some() {
            return false;
        }
        self.shell.input_mode = InputMode::Insert;
        self.backups.prune = Some(BackupPruneState {
            stage: BackupPruneStage::Input,
            keep_input: "3".into(),
            prepared: None,
            message: None,
        });
        true
    }

    pub fn backup_prune_push_digit(&mut self, ch: char) {
        if let Some(prune) = self.backups.prune.as_mut() {
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
        if let Some(prune) = self.backups.prune.as_mut() {
            if prune.stage == BackupPruneStage::Input {
                prune.keep_input.pop();
                prune.message = None;
            }
        }
    }

    pub fn backup_prune_start_plan(&mut self) -> Result<usize, String> {
        let prune = self
            .backups
            .prune
            .as_mut()
            .ok_or_else(|| "清理向导未打开".to_string())?;
        let keep = prune
            .keep_input
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| "保留份数必须为大于 0 的整数".to_string())?;
        prune.stage = BackupPruneStage::Planning;
        prune.message = Some(crate::tui::ui::UiMessage::progress(
            "正在后台扫描备份并生成固定清理计划…",
        ));
        self.shell.input_mode = InputMode::Normal;
        Ok(keep)
    }

    pub fn backup_prune_finish_plan(&mut self, result: Result<BackupPrunePrepared, String>) {
        match result {
            Ok(prepared) if prepared.plan.targets.is_empty() => {
                self.backups.prune = None;
                self.shell.input_mode = InputMode::Normal;
                self.set_success_notice("无需清理：当前备份已经满足保留策略。");
            }
            Ok(prepared) => {
                if let Some(prune) = self.backups.prune.as_mut() {
                    prune.prepared = Some(prepared);
                    prune.stage = BackupPruneStage::Confirm;
                    prune.message = None;
                    self.shell.input_mode = InputMode::Normal;
                }
            }
            Err(message) => {
                if let Some(prune) = self.backups.prune.as_mut() {
                    prune.stage = BackupPruneStage::Input;
                    prune.message = Some(crate::tui::ui::UiMessage::error(message));
                    self.shell.input_mode = InputMode::Insert;
                }
            }
        }
    }

    pub fn backup_prune_take_for_execute(&mut self) -> Option<BackupPrunePrepared> {
        let prune = self.backups.prune.as_mut()?;
        if prune.stage != BackupPruneStage::Confirm {
            return None;
        }
        let prepared = prune.prepared.take()?;
        prune.stage = BackupPruneStage::Running;
        prune.message = Some(crate::tui::ui::UiMessage::progress(
            "正在逐条复核摘要并清理固定候选…",
        ));
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = true;
        Some(prepared)
    }

    pub fn backup_prune_finish_execute(&mut self, result: Result<usize, String>) {
        self.shell.critical_operation = false;
        let success = result.is_ok();
        let message = match result {
            Ok(count) => format!("清理完成：已安全删除 {count} 份旧备份。"),
            Err(message) => message,
        };
        self.backups.prune = None;
        self.shell.input_mode = InputMode::Normal;
        if success {
            self.set_success_notice(message);
        } else {
            self.set_error_notice(message);
        }
    }

    pub fn close_backup_prune(&mut self) {
        if !self.shell.critical_operation {
            self.backups.prune = None;
            self.shell.input_mode = InputMode::Normal;
        }
    }

    pub fn begin_backup_delete(
        &mut self,
        path: std::path::PathBuf,
        expected_sha256: String,
    ) -> bool {
        if self.shell.critical_operation {
            self.set_warning_notice("关键操作仍在执行，完成前不能启动其他任务。".to_string());
            return false;
        }
        self.shell.input_mode = InputMode::Normal;
        self.backups.delete = Some(BackupDeleteState {
            stage: WizardStage::Confirm,
            path,
            expected_sha256,
            message: None,
        });
        true
    }

    pub fn submit_backup_delete_confirmation(&mut self) -> Option<(std::path::PathBuf, String)> {
        let delete = self.backups.delete.as_mut()?;
        if delete.stage != WizardStage::Confirm {
            return None;
        }
        delete.stage = WizardStage::Running;
        delete.message = Some(crate::tui::ui::UiMessage::progress(
            "正在复核文件内容并删除备份…",
        ));
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = true;
        Some((delete.path.clone(), delete.expected_sha256.clone()))
    }

    pub fn finish_backup_delete(&mut self, result: Result<(), String>) {
        self.shell.critical_operation = false;
        self.backups.delete = None;
        let success = result.is_ok();
        let message = match result {
            Ok(()) => "备份已删除；列表已刷新".to_string(),
            Err(message) => message,
        };
        if success {
            self.set_success_notice(message);
        } else {
            self.set_error_notice(message);
        }
    }

    pub const fn workspace(&self) -> Workspace {
        self.shell.workspace
    }

    pub const fn provision(&self) -> &ProvisionState {
        &self.provision
    }
}
