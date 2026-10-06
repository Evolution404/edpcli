use super::*;

impl AppState {
    pub fn active_table_kind(&self) -> Option<crate::tui::table_layout::TableKind> {
        use crate::tui::pane::PaneId;
        use crate::tui::table_layout::TableKind;

        if self.restore.wizard.as_ref().is_some_and(|wizard| {
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
            TableKind::Backups => self.backup_view_snapshot().content_widths.clone(),
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
            TableKind::Devices => terminal_width.saturating_sub(4),
            TableKind::Backups => {
                let class = crate::tui::ui::ViewportClass::for_width(terminal_width);
                terminal_width
                    .saturating_sub(class.backup_device_sidebar_width().unwrap_or(0))
                    .saturating_sub(4)
            }
            TableKind::RelatedBackups => terminal_width
                .saturating_mul(7)
                .saturating_div(10)
                .saturating_sub(6),
            TableKind::InspectFields => {
                let class = crate::tui::ui::ViewportClass::for_width(terminal_width);
                if class == crate::tui::ui::ViewportClass::Compact {
                    terminal_width.saturating_sub(3)
                } else {
                    terminal_width
                        .saturating_mul(71)
                        .saturating_div(100)
                        .saturating_sub(3)
                }
            }
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
        interaction.ensure_active_visible_for_layout(&layout, &widths, viewport_width);
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
        let interaction = self.shell.horizontal_scroll.entry(kind).or_default();
        if kind == crate::tui::table_layout::TableKind::InspectFields {
            interaction.move_active_bounded(&layout, &widths, viewport_width, reverse)
        } else {
            interaction.move_active(&layout, &widths, viewport_width, reverse)
        }
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
        let interaction = self.shell.horizontal_scroll.entry(kind).or_default();
        if kind == crate::tui::table_layout::TableKind::InspectFields {
            interaction.move_active_edge_bounded(&layout, &widths, viewport_width, last)
        } else {
            interaction.move_active_edge(&layout, &widths, viewport_width, last)
        }
    }

    pub fn table_sort(
        &self,
        kind: crate::tui::table_layout::TableKind,
    ) -> Option<crate::tui::table_layout::TableSort> {
        self.table_interaction(kind).sort()
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

    pub fn table_scroll_offset(&self, kind: crate::tui::table_layout::TableKind) -> usize {
        self.shell
            .horizontal_scroll
            .get(&kind)
            .copied()
            .unwrap_or_default()
            .offset()
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
        let interaction = self.shell.horizontal_scroll.entry(kind).or_default();
        if kind == crate::tui::table_layout::TableKind::InspectFields {
            interaction.scroll_viewport_bounded(&layout, &widths, viewport_width, reverse)
        } else {
            interaction.scroll_viewport(&layout, &widths, viewport_width, reverse)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_table_interaction_width_matches_rendered_table_after_device_sidebar() {
        let state = AppState::new();
        use crate::tui::table_layout::TableKind;

        assert_eq!(state.table_viewport_width(TableKind::Backups, 100, 40), 66);
        assert_eq!(state.table_viewport_width(TableKind::Backups, 140, 40), 100);
        assert_eq!(state.table_viewport_width(TableKind::Backups, 200, 40), 156);
        assert_eq!(
            state.table_viewport_width(TableKind::Backups, 79, 40),
            75,
            "compact mode has no side-by-side device tree"
        );
    }
}
