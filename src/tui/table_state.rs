use super::*;

impl AppState {
    pub fn active_table_kind(&self) -> Option<crate::tui::table_layout::TableKind> {
        use crate::tui::pane::PaneId;
        use crate::tui::table_layout::TableKind;

        if self.shell.wizard.as_ref().is_some_and(|wizard| {
            wizard.stage == WizardStage::PostRestore
                && wizard.post_restore_workbench.focused_pane() == PaneId::ResultPartitions
        }) {
            return Some(TableKind::ResultPartitions);
        }

        match self.shell.workspace {
            Workspace::Devices if self.devices_focused_pane() == PaneId::DevicesList => {
                Some(TableKind::Devices)
            }
            Workspace::Devices
                if self.devices_focused_pane() == PaneId::DevicesDetail
                    && matches!(
                        self.device_info_selected_key(),
                        DeviceInfoNodeKey::Status | DeviceInfoNodeKey::Backups
                    )
                    && !self.device_related_backups().is_empty() =>
            {
                Some(TableKind::RelatedBackups)
            }
            Workspace::Backups if self.backups_focused_pane() == PaneId::BackupsList => {
                Some(TableKind::Backups)
            }
            Workspace::Provision
                if self.provision.stage == ProvisionStage::Result
                    && self.provision.result_workbench.focused_pane()
                        == PaneId::ResultPartitions =>
            {
                Some(TableKind::ResultPartitions)
            }
            Workspace::Provision => None,
            Workspace::Inspect
                if self.advanced_inspect_focused_pane() == Some(PaneId::InspectDetail)
                    && !self.advanced_inspect_detail_rows().is_empty() =>
            {
                Some(TableKind::InspectFields)
            }
            _ => None,
        }
    }

    pub fn table_interaction(
        &self,
        kind: crate::tui::table_layout::TableKind,
    ) -> crate::tui::table_layout::TableInteractionState {
        self.shell
            .horizontal_scroll
            .get(&kind)
            .copied()
            .unwrap_or_default()
    }

    pub fn table_active_column(&self, kind: crate::tui::table_layout::TableKind) -> usize {
        self.table_interaction(kind).active_column()
    }

    pub fn table_column_order(&self, kind: crate::tui::table_layout::TableKind) -> Vec<usize> {
        let count = crate::tui::table_layout::layout_for(kind).specs().len();
        self.shell
            .table_column_order
            .get(&kind)
            .filter(|order| {
                order.len() == count && {
                    let mut sorted = (*order).clone();
                    sorted.sort_unstable();
                    sorted == (0..count).collect::<Vec<_>>()
                }
            })
            .cloned()
            .unwrap_or_else(|| (0..count).collect())
    }

    pub fn table_logical_column(
        &self,
        kind: crate::tui::table_layout::TableKind,
        visual_column: usize,
    ) -> usize {
        let order = self.table_column_order(kind);
        order
            .get(visual_column)
            .copied()
            .unwrap_or_else(|| visual_column.min(order.len().saturating_sub(1)))
    }

    fn table_selected_row_values(
        &self,
        kind: crate::tui::table_layout::TableKind,
    ) -> Option<Vec<String>> {
        use crate::tui::table_layout::TableKind;

        let sanitize = |value: String| crate::ui::sanitize_terminal_text(&value);
        match kind {
            TableKind::Devices => {
                let source = self.device_source_index_at_visible(self.shell.selected)?;
                self.devices.table_view.rows.get(source).cloned()
            }
            TableKind::Backups => {
                let source = self.backup_source_index_at_visible(self.shell.selected)?;
                self.backups.table_view.rows.get(source).cloned()
            }
            TableKind::RelatedBackups => self
                .device_related_backup_table_view()
                .rows
                .get(self.device_related_backup_selected_index()?)
                .cloned(),
            TableKind::InspectFields => {
                let row = self.advanced_inspect_detail_selected_row()?;
                Some(row.cells.iter().cloned().map(sanitize).collect::<Vec<_>>())
            }
            TableKind::ResultPartitions => {
                let selected = self.result_partition_selected_source_index()?;
                self.result_partition_table_view()
                    .and_then(|view| view.rows.get(selected).cloned())
            }
        }
    }

    pub fn table_copy_payload(
        &self,
        kind: crate::tui::table_layout::TableKind,
        whole_row: bool,
    ) -> Option<String> {
        let values = self.table_selected_row_values(kind)?;
        let order = self.table_column_order(kind);
        if whole_row {
            Some(crate::tui::table_layout::copy_row_values(
                kind, &order, &values,
            ))
        } else {
            let logical = self.table_logical_column(kind, self.table_active_column(kind));
            crate::tui::table_layout::copy_cell_value(kind, logical, &values)
        }
    }

    pub fn table_visual_layout(
        &self,
        kind: crate::tui::table_layout::TableKind,
    ) -> crate::tui::table_layout::AdaptiveTableLayout {
        let base = crate::tui::table_layout::layout_for(kind);
        let order = self.table_column_order(kind);
        crate::tui::table_layout::AdaptiveTableLayout::new(
            order
                .into_iter()
                .filter_map(|logical| base.specs().get(logical).copied())
                .collect(),
        )
    }

    pub fn table_visual_widths(
        &self,
        kind: crate::tui::table_layout::TableKind,
        logical_widths: &[usize],
    ) -> Vec<usize> {
        self.table_column_order(kind)
            .into_iter()
            .map(|logical| logical_widths.get(logical).copied().unwrap_or(0))
            .collect()
    }

    fn ensure_table_column_order(
        &mut self,
        kind: crate::tui::table_layout::TableKind,
    ) -> &mut Vec<usize> {
        let count = crate::tui::table_layout::layout_for(kind).specs().len();
        let order = self
            .shell
            .table_column_order
            .entry(kind)
            .or_insert_with(|| (0..count).collect());
        let valid = order.len() == count && {
            let mut sorted = order.clone();
            sorted.sort_unstable();
            sorted == (0..count).collect::<Vec<_>>()
        };
        if !valid {
            *order = (0..count).collect();
        }
        order
    }

    fn table_content_widths(&self, kind: crate::tui::table_layout::TableKind) -> Vec<usize> {
        use crate::tui::table_layout::{display_width, TableKind};

        match kind {
            TableKind::Devices => self.devices.table_view.content_widths.clone(),
            TableKind::Backups => self.backups.table_view.content_widths.clone(),
            TableKind::RelatedBackups => self.device_related_backup_table_view().content_widths,
            TableKind::InspectFields => {
                let mut widths = INSPECT_DETAIL_HEADINGS
                    .iter()
                    .map(|value| display_width(value))
                    .collect::<Vec<_>>();
                for row in self.advanced_inspect_detail_rows() {
                    for (index, value) in row.cells.iter().enumerate() {
                        widths[index] = widths[index].max(display_width(value));
                    }
                }
                widths
            }
            TableKind::ResultPartitions => self
                .result_partition_table_view()
                .map(|view| view.content_widths)
                .unwrap_or_else(|| {
                    crate::tui::table_layout::table_column_schema(TableKind::ResultPartitions)
                        .expect("result partition schema")
                        .iter()
                        .map(|column| display_width(column.heading))
                        .collect()
                }),
        }
    }

    fn table_viewport_width(
        &self,
        kind: crate::tui::table_layout::TableKind,
        terminal_width: u16,
        _terminal_height: usize,
    ) -> u16 {
        use crate::tui::table_layout::TableKind;
        match kind {
            TableKind::Devices | TableKind::Backups => terminal_width.saturating_sub(4),
            TableKind::RelatedBackups => terminal_width
                .saturating_mul(7)
                .saturating_div(10)
                .saturating_sub(6),
            TableKind::InspectFields => terminal_width.saturating_sub(3),
            TableKind::ResultPartitions => {
                let class = crate::tui::ui::ViewportClass::for_width(terminal_width);
                if matches!(
                    class,
                    crate::tui::ui::ViewportClass::Wide | crate::tui::ui::ViewportClass::UltraWide
                ) {
                    terminal_width
                        .saturating_mul(46)
                        .saturating_div(100)
                        .saturating_sub(4)
                } else {
                    terminal_width.saturating_sub(4)
                }
            }
        }
        .max(1)
    }

    fn table_visual_geometry(
        &self,
        kind: crate::tui::table_layout::TableKind,
    ) -> (crate::tui::table_layout::AdaptiveTableLayout, Vec<usize>) {
        let logical_widths = self.table_content_widths(kind);
        (
            self.table_visual_layout(kind),
            self.table_visual_widths(kind, &logical_widths),
        )
    }

    pub fn reorder_table_column_for_viewport(
        &mut self,
        kind: crate::tui::table_layout::TableKind,
        reverse: bool,
        terminal_width: u16,
        terminal_height: usize,
    ) -> bool {
        let active = self.table_interaction(kind).active_column();
        let count = crate::tui::table_layout::layout_for(kind).specs().len();
        if count == 0 {
            return false;
        }
        let target = if reverse {
            active.saturating_sub(1)
        } else {
            active.saturating_add(1).min(count - 1)
        };
        if target == active {
            return false;
        }

        {
            let order = self.ensure_table_column_order(kind);
            order.swap(active, target);
        }

        let (layout, widths) = self.table_visual_geometry(kind);
        let viewport_width = self.table_viewport_width(kind, terminal_width, terminal_height);
        let interaction = self.shell.horizontal_scroll.entry(kind).or_default();
        interaction.set_active_column(target);
        interaction.ensure_active_visible_for_layout(&layout, &widths, viewport_width, reverse);
        true
    }

    pub fn move_table_column_for_viewport(
        &mut self,
        kind: crate::tui::table_layout::TableKind,
        reverse: bool,
        terminal_width: u16,
        terminal_height: usize,
    ) -> bool {
        let (layout, widths) = self.table_visual_geometry(kind);
        let viewport_width = self.table_viewport_width(kind, terminal_width, terminal_height);
        self.shell
            .horizontal_scroll
            .entry(kind)
            .or_default()
            .move_active(&layout, &widths, viewport_width, reverse)
    }

    pub fn move_table_column_edge_for_viewport(
        &mut self,
        kind: crate::tui::table_layout::TableKind,
        last: bool,
        terminal_width: u16,
        terminal_height: usize,
    ) -> bool {
        let (layout, widths) = self.table_visual_geometry(kind);
        let viewport_width = self.table_viewport_width(kind, terminal_width, terminal_height);
        self.shell
            .horizontal_scroll
            .entry(kind)
            .or_default()
            .move_active_edge(&layout, &widths, viewport_width, last)
    }

    pub fn table_sort(
        &self,
        kind: crate::tui::table_layout::TableKind,
    ) -> Option<crate::tui::table_layout::TableSort> {
        self.table_interaction(kind).sort()
    }

    pub fn move_table_column(
        &mut self,
        kind: crate::tui::table_layout::TableKind,
        reverse: bool,
    ) -> bool {
        let (layout, widths) = self.table_visual_geometry(kind);
        self.shell
            .horizontal_scroll
            .entry(kind)
            .or_default()
            .move_active(&layout, &widths, u16::MAX, reverse)
    }

    pub fn toggle_table_sort(&mut self, kind: crate::tui::table_layout::TableKind) {
        self.change_table_sort(kind, false);
    }

    pub fn clear_table_sort(&mut self, kind: crate::tui::table_layout::TableKind) -> bool {
        self.change_table_sort(kind, true)
    }

    fn change_table_sort(
        &mut self,
        kind: crate::tui::table_layout::TableKind,
        clear: bool,
    ) -> bool {
        let device_disk = (kind == crate::tui::table_layout::TableKind::Devices)
            .then(|| self.selected_device().map(|row| row.disk))
            .flatten();
        let backup_path = (kind == crate::tui::table_layout::TableKind::Backups)
            .then(|| self.selected_backup().map(|row| row.path.clone()))
            .flatten();
        let related_backup_path = (kind == crate::tui::table_layout::TableKind::RelatedBackups)
            .then(|| {
                self.selected_device_related_backup()
                    .map(|row| row.path.clone())
            })
            .flatten();
        let inspect_key = (kind == crate::tui::table_layout::TableKind::InspectFields)
            .then(|| {
                self.advanced_inspect_detail_selected_row()
                    .map(|row| (row.field_index, row.child_index, row.range))
            })
            .flatten();

        let logical_column = self.table_logical_column(kind, self.table_active_column(kind));
        let interaction = self.shell.horizontal_scroll.entry(kind).or_default();
        let changed = if clear {
            interaction.clear_sort()
        } else {
            interaction.toggle_sort_for(logical_column);
            true
        };

        if let Some(disk) = device_disk {
            if let Some(position) = self
                .visible_device_indices()
                .iter()
                .position(|index| self.devices.rows[*index].disk == disk)
            {
                self.shell.selected = position;
            }
        }
        if let Some(path) = backup_path {
            if let Some(position) = self
                .visible_backup_indices()
                .iter()
                .position(|index| self.backups.rows[*index].path == path)
            {
                self.shell.selected = position;
            }
        }
        if let Some(path) = related_backup_path {
            if let Some(position) = self
                .device_related_backups()
                .iter()
                .position(|(source, _)| {
                    self.backups()
                        .get(*source)
                        .is_some_and(|backup| backup.path == path)
                })
            {
                self.devices.related_backup_selected = position;
            }
        }
        if let Some(key) = inspect_key {
            let rows = self.advanced_inspect_detail_rows();
            if let Some(position) = rows
                .iter()
                .position(|row| (row.field_index, row.child_index, row.range) == key)
            {
                if let Some(advanced) = self.inspect.advanced.as_mut() {
                    advanced
                        .pane_focus
                        .viewport_mut(crate::tui::pane::PaneId::InspectDetail)
                        .selected = Some(position);
                }
            }
        }
        changed
    }

    pub fn device_related_backup_table_view(&self) -> crate::tui::table_layout::TableViewData {
        let related = self.device_related_backups();
        crate::tui::table_layout::related_backup_table_view(self.backups(), &related)
    }

    pub fn result_partition_table_view(&self) -> Option<crate::tui::table_layout::TableViewData> {
        use crate::tui::table_layout::{table_column_schema, TableKind, TableViewData};

        let rows = if self
            .shell
            .wizard
            .as_ref()
            .is_some_and(|wizard| wizard.stage == WizardStage::PostRestore)
        {
            let wizard = self.shell.wizard.as_ref()?;
            let outcome = wizard.restore_outcome.as_ref()?;
            outcome
                .assessment
                .partitions
                .iter()
                .map(|partition| {
                    let status = match partition.state {
                        crate::application::post_restore::PostRestorePartitionState::Usable => {
                            "可用"
                        }
                        crate::application::post_restore::PostRestorePartitionState::NeedsFormat => {
                            "需要格式化"
                        }
                        crate::application::post_restore::PostRestorePartitionState::PasswordRequired => {
                            "需要原密码"
                        }
                        crate::application::post_restore::PostRestorePartitionState::CryptoMetadataInvalid => {
                            "加密元数据异常"
                        }
                        crate::application::post_restore::PostRestorePartitionState::Unsupported => {
                            "暂不支持"
                        }
                    };
                    let filesystem = partition
                        .detected_filesystem
                        .map(|value| value.label().to_string())
                        .or_else(|| partition.filesystem_hint.clone())
                        .unwrap_or_else(|| "—".into());
                    let end = partition
                        .start_lba
                        .saturating_add(partition.sector_count)
                        .saturating_sub(1);
                    let key_state = if partition.requires_original_key {
                        "需要原密钥域"
                    } else {
                        "无需原密钥"
                    };
                    vec![
                        format!("P{}", partition.index),
                        status.into(),
                        filesystem,
                        format!("{}..={end}", partition.start_lba),
                        crate::common::fmt_capacity(
                            partition
                                .sector_count
                                .saturating_mul(crate::common::SECTOR as u64),
                        ),
                        key_state.into(),
                        crate::ui::sanitize_terminal_text(&partition.detail),
                    ]
                })
                .collect::<Vec<_>>()
        } else if self.shell.workspace == Workspace::Provision
            && self.provision.stage == ProvisionStage::Result
        {
            let plan = self.provision.result_plan.as_ref()?;
            plan.partitions
                .iter()
                .enumerate()
                .map(|(index, partition)| {
                    let filesystem = partition
                        .filesystem
                        .map(|kind| kind.config_token().to_string())
                        .unwrap_or_else(|| "—".into());
                    let disposition = |value: crate::provision::RegionDisposition| match value {
                        crate::provision::RegionDisposition::PreserveOpaque => "原样保留",
                        crate::provision::RegionDisposition::PreserveVerified => "验证保留",
                        crate::provision::RegionDisposition::RewrapVerified => "密钥已更新",
                        crate::provision::RegionDisposition::Migrate => "数据已迁移",
                        crate::provision::RegionDisposition::Rebuild => "已重建",
                        crate::provision::RegionDisposition::Drop => "已移除",
                    };
                    let action = if partition.selected_for_format {
                        "格式化"
                    } else {
                        partition.disposition.map(disposition).unwrap_or("写入")
                    };
                    let final_status = if plan.target == crate::provision::ProvisionTarget::Plain {
                        if self.provision.result_outcome.is_some() {
                            "已写入 · 读回通过".to_string()
                        } else {
                            "未确认".to_string()
                        }
                    } else if partition.selected_for_format {
                        let format_status = partition.role.and_then(|role| {
                            self.provision.result_outcome.as_ref().and_then(|outcome| {
                                match &outcome.commit {
                                    crate::application::provision::ProvisionCommitOutcome::Official(report) => report
                                        .formats
                                        .iter()
                                        .find(|item| item.role == role)
                                        .map(|format| {
                                            if format.result.is_ok() {
                                                "已格式化 · 读回通过"
                                            } else {
                                                "格式化失败"
                                            }
                                        }),
                                    crate::application::provision::ProvisionCommitOutcome::Plain { .. } => None,
                                }
                            })
                        });
                        format_status.unwrap_or("已写入").to_string()
                    } else {
                        partition
                            .disposition
                            .map(disposition)
                            .unwrap_or("未格式化")
                            .to_string()
                    };
                    vec![
                        format!("P{}", index + 1),
                        partition
                            .role
                            .map(crate::provision::PartitionRole::label)
                            .unwrap_or("普通分区")
                            .into(),
                        filesystem,
                        format!(
                            "{}..={}",
                            partition.start_lba,
                            partition.end_exclusive().saturating_sub(1)
                        ),
                        crate::common::fmt_capacity(partition.size_bytes),
                        action.into(),
                        final_status,
                    ]
                })
                .collect::<Vec<_>>()
        } else {
            return None;
        };

        Some(TableViewData::from_rows(
            0,
            &table_column_schema(TableKind::ResultPartitions)
                .expect("result partition table schema"),
            rows,
        ))
    }

    pub fn visible_result_partition_indices(&self) -> Vec<usize> {
        let Some(view) = self.result_partition_table_view() else {
            return Vec::new();
        };
        view.sorted_indices(
            (0..view.rows.len()).collect(),
            self.table_interaction(crate::tui::table_layout::TableKind::ResultPartitions),
        )
    }

    pub fn result_partition_selected_source_index(&self) -> Option<usize> {
        if self
            .shell
            .wizard
            .as_ref()
            .is_some_and(|wizard| wizard.stage == WizardStage::PostRestore)
        {
            self.shell
                .wizard
                .as_ref()
                .and_then(|wizard| wizard.post_restore_workbench.selected_partition)
        } else if self.shell.workspace == Workspace::Provision
            && self.provision.stage == ProvisionStage::Result
        {
            self.provision.result_workbench.selected_partition
        } else {
            None
        }
    }

    pub fn result_partition_moved_source(
        &self,
        current_source: usize,
        delta: isize,
    ) -> Option<usize> {
        let order = self.visible_result_partition_indices();
        let current = order
            .iter()
            .position(|source| *source == current_source)
            .unwrap_or(0);
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize)
        }
        .min(order.len().saturating_sub(1));
        order.get(next).copied()
    }

    pub fn table_scroll_offset(&self, kind: crate::tui::table_layout::TableKind) -> usize {
        self.shell
            .horizontal_scroll
            .get(&kind)
            .copied()
            .unwrap_or_default()
            .offset()
    }

    pub fn scroll_table(
        &mut self,
        kind: crate::tui::table_layout::TableKind,
        reverse: bool,
    ) -> bool {
        self.scroll_table_for_viewport(kind, reverse, 80, 24)
    }

    pub fn scroll_table_for_viewport(
        &mut self,
        kind: crate::tui::table_layout::TableKind,
        reverse: bool,
        terminal_width: u16,
        terminal_height: usize,
    ) -> bool {
        let (layout, widths) = self.table_visual_geometry(kind);
        let viewport_width = self.table_viewport_width(kind, terminal_width, terminal_height);
        self.shell
            .horizontal_scroll
            .entry(kind)
            .or_default()
            .scroll_viewport(&layout, &widths, viewport_width, reverse)
    }
}
