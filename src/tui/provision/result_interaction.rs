use super::*;

impl AppState {
    pub fn provision_result_focused_pane(&self) -> crate::tui::pane::PaneId {
        self.provision.result_workbench.focused_pane()
    }

    pub fn provision_result_shift_pane(&mut self, reverse: bool) {
        self.provision.result_workbench.cycle_pane(reverse);
    }

    pub fn provision_result_shift_partition_column(&mut self, reverse: bool) -> bool {
        if self.provision.result_workbench.focused_pane()
            != crate::tui::pane::PaneId::ResultPartitions
        {
            return false;
        }
        self.provision
            .result_workbench
            .move_partition_active_column(
                reverse,
                crate::tui::result_workbench::RESULT_PARTITION_COLUMN_COUNT,
            )
    }

    pub fn provision_initialize_result_workbench(&mut self) {
        let Some(plan) = self.provision.result_plan.clone() else {
            self.provision.result_workbench =
                crate::tui::result_workbench::ResultWorkbenchState::default();
            return;
        };
        let Ok(model) = plan.disk_layout_model() else {
            self.provision.result_workbench =
                crate::tui::result_workbench::ResultWorkbenchState::default();
            return;
        };

        let mut workbench = crate::tui::result_workbench::ResultWorkbenchState::default();
        if let Some(selection) = plan.partition_selection(0) {
            workbench.selected_partition = Some(0);
            let _ = workbench.select_region_geometry(&model, &selection, 8);
        } else {
            workbench.reconcile_regions(&model, 8);
        }
        self.provision.result_workbench = workbench;
    }

    pub fn provision_result_move(&mut self, delta: isize, visible_rows: usize) {
        let Some(plan) = self.provision.result_plan.clone() else {
            return;
        };
        let Ok(model) = plan.disk_layout_model() else {
            return;
        };

        match self.provision.result_workbench.focused_pane() {
            crate::tui::pane::PaneId::ResultPartitions => {
                if plan.partitions.is_empty() {
                    self.provision.result_workbench.selected_partition = None;
                    return;
                }
                let current = self
                    .provision
                    .result_workbench
                    .selected_partition
                    .unwrap_or(0)
                    .min(plan.partitions.len().saturating_sub(1));
                let next = if delta < 0 {
                    current.saturating_sub(delta.unsigned_abs())
                } else {
                    current.saturating_add(delta as usize)
                }
                .min(plan.partitions.len().saturating_sub(1));
                self.provision.result_workbench.selected_partition = Some(next);
                if let Some(selection) = plan.partition_selection(next) {
                    let _ = self.provision.result_workbench.select_region_geometry(
                        &model,
                        &selection,
                        visible_rows,
                    );
                }
            }
            crate::tui::pane::PaneId::ResultDiskLayout => {
                if self.provision.result_workbench.move_region_selection(
                    &model,
                    delta,
                    visible_rows,
                ) {
                    self.provision.result_workbench.selected_partition = self
                        .provision
                        .result_workbench
                        .region_selection()
                        .as_ref()
                        .and_then(|selection| plan.partition_index_for_selection(selection));
                }
            }
            crate::tui::pane::PaneId::ResultVerification => {
                let content_len = self.provision_result_verification_line_count();
                self.provision
                    .result_workbench
                    .viewport_mut(crate::tui::pane::PaneId::ResultVerification)
                    .scroll_y
                    .move_lines(delta, content_len, visible_rows);
            }
            _ => {}
        }
    }

    pub fn provision_result_top(&mut self, visible_rows: usize) {
        let delta = isize::MIN / 2;
        self.provision_result_move(delta, visible_rows);
        if self.provision.result_workbench.focused_pane()
            == crate::tui::pane::PaneId::ResultVerification
        {
            self.provision
                .result_workbench
                .viewport_mut(crate::tui::pane::PaneId::ResultVerification)
                .scroll_y
                .top();
        }
    }

    pub fn provision_result_bottom(&mut self, visible_rows: usize) {
        let delta = isize::MAX;
        self.provision_result_move(delta, visible_rows);
        if self.provision.result_workbench.focused_pane()
            == crate::tui::pane::PaneId::ResultVerification
        {
            let content_len = self.provision_result_verification_line_count();
            self.provision
                .result_workbench
                .viewport_mut(crate::tui::pane::PaneId::ResultVerification)
                .scroll_y
                .bottom(content_len, visible_rows);
        }
    }

    pub fn provision_result_verification_line_count(&self) -> usize {
        let warnings = self
            .provision
            .result_outcome
            .as_ref()
            .map(|outcome| outcome.warnings.len())
            .unwrap_or(0);
        let formats = self
            .provision
            .result_outcome
            .as_ref()
            .and_then(|outcome| match &outcome.commit {
                crate::application::provision::ProvisionCommitOutcome::Official(report) => {
                    Some(report.formats.len())
                }
                crate::application::provision::ProvisionCommitOutcome::Plain { .. } => None,
            })
            .unwrap_or(0);
        8usize.saturating_add(warnings).saturating_add(formats)
    }
}
