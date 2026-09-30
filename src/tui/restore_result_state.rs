use super::*;

const RESTORE_RESULT_COLUMN_COUNT: usize = 7;

fn partition_selection(
    outcome: &crate::application::post_restore::MetadataRestoreOutcome,
    index: usize,
) -> Option<crate::tui::disk_layout::DiskCapacitySelection> {
    let partition = outcome.assessment.partitions.get(index)?;
    let model = outcome.layout.as_ref().ok()?;
    model
        .segments
        .iter()
        .find(|segment| {
            segment.start_lba == partition.start_lba
                && segment.sector_count == partition.sector_count
        })
        .and_then(crate::tui::disk_layout::DiskCapacitySelection::from_segment)
}

fn partition_index_for_selection(
    outcome: &crate::application::post_restore::MetadataRestoreOutcome,
    selection: &crate::tui::disk_layout::DiskCapacitySelection,
) -> Option<usize> {
    (0..outcome.assessment.partitions.len()).find(|index| {
        partition_selection(outcome, *index).as_ref() == Some(selection)
    })
}

impl WizardState {
    pub(super) fn active_post_restore_partition_index(&self) -> Option<usize> {
        self.post_restore_workbench.selected_partition
    }
}

impl AppState {
    pub fn initialize_post_restore_result_workbench(&mut self) {
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        let Some(outcome) = wizard.restore_outcome.as_ref() else {
            wizard.post_restore_workbench =
                crate::tui::result_workbench::ResultWorkbenchState::default();
            return;
        };

        let mut workbench = crate::tui::result_workbench::ResultWorkbenchState::default();
        if !outcome.assessment.partitions.is_empty() {
            workbench.selected_partition = Some(0);
            if let (Ok(model), Some(selection)) =
                (outcome.layout.as_ref(), partition_selection(outcome, 0))
            {
                let _ = workbench.select_region_geometry(model, &selection, 8);
            }
        }
        wizard.post_restore_workbench = workbench;
    }

    pub fn post_restore_result_focused_pane(&self) -> crate::tui::pane::PaneId {
        self.shell
            .wizard
            .as_ref()
            .map(|wizard| wizard.post_restore_workbench.focused_pane())
            .unwrap_or(crate::tui::pane::PaneId::ResultPartitions)
    }

    pub fn post_restore_result_shift_pane(&mut self, reverse: bool) {
        if let Some(wizard) = self.shell.wizard.as_mut() {
            if wizard.stage == WizardStage::PostRestore {
                wizard.post_restore_workbench.cycle_pane(reverse);
            }
        }
    }

    pub fn post_restore_result_shift_partition_column(&mut self, reverse: bool) -> bool {
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return false;
        };
        if wizard.stage != WizardStage::PostRestore
            || wizard.post_restore_workbench.focused_pane()
                != crate::tui::pane::PaneId::ResultPartitions
        {
            return false;
        }
        wizard
            .post_restore_workbench
            .move_partition_active_column(reverse, RESTORE_RESULT_COLUMN_COUNT)
    }

    pub fn post_restore_result_active_column(&self) -> usize {
        self.shell
            .wizard
            .as_ref()
            .map(|wizard| {
                wizard
                    .post_restore_workbench
                    .partition_active_column(RESTORE_RESULT_COLUMN_COUNT)
            })
            .unwrap_or(0)
    }

    pub fn move_post_restore_result_selection(&mut self, delta: isize, visible_rows: usize) {
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        if wizard.stage != WizardStage::PostRestore {
            return;
        }
        let Some(outcome) = wizard.restore_outcome.as_ref() else {
            return;
        };

        match wizard.post_restore_workbench.focused_pane() {
            crate::tui::pane::PaneId::ResultPartitions => {
                let len = outcome.assessment.partitions.len();
                if len == 0 {
                    wizard.post_restore_workbench.selected_partition = None;
                    return;
                }
                let current = wizard
                    .post_restore_workbench
                    .selected_partition
                    .unwrap_or(0)
                    .min(len - 1);
                let next = if delta < 0 {
                    current.saturating_sub(delta.unsigned_abs())
                } else {
                    current.saturating_add(delta as usize)
                }
                .min(len - 1);
                wizard.post_restore_workbench.selected_partition = Some(next);
                if let (Ok(model), Some(selection)) =
                    (outcome.layout.as_ref(), partition_selection(outcome, next))
                {
                    let _ = wizard.post_restore_workbench.select_region_geometry(
                        model,
                        &selection,
                        visible_rows,
                    );
                }
            }
            crate::tui::pane::PaneId::ResultDiskLayout => {
                let Ok(model) = outcome.layout.as_ref() else {
                    return;
                };
                if wizard.post_restore_workbench.move_region_selection(
                    model,
                    delta,
                    visible_rows,
                ) {
                    let selected = wizard
                        .post_restore_workbench
                        .region_selection()
                        .as_ref()
                        .and_then(|selection| partition_index_for_selection(outcome, selection));
                    wizard.post_restore_workbench.selected_partition = selected;
                }
            }
            crate::tui::pane::PaneId::ResultVerification => {
                let content_len = 7usize
                    .saturating_add(outcome.assessment.issues.len())
                    .saturating_add(usize::from(outcome.layout.is_err()));
                wizard
                    .post_restore_workbench
                    .viewport_mut(crate::tui::pane::PaneId::ResultVerification)
                    .scroll_y
                    .move_lines(delta, content_len, visible_rows);
            }
            _ => {}
        }
    }

    pub fn post_restore_result_top(&mut self, visible_rows: usize) {
        self.move_post_restore_result_selection(isize::MIN / 2, visible_rows);
        if let Some(wizard) = self.shell.wizard.as_mut() {
            if wizard.stage == WizardStage::PostRestore
                && wizard.post_restore_workbench.focused_pane()
                    == crate::tui::pane::PaneId::ResultVerification
            {
                wizard
                    .post_restore_workbench
                    .viewport_mut(crate::tui::pane::PaneId::ResultVerification)
                    .scroll_y
                    .top();
            }
        }
    }

    pub fn post_restore_result_bottom(&mut self, visible_rows: usize) {
        self.move_post_restore_result_selection(isize::MAX, visible_rows);
        let Some(wizard) = self.shell.wizard.as_mut() else {
            return;
        };
        if wizard.stage != WizardStage::PostRestore
            || wizard.post_restore_workbench.focused_pane()
                != crate::tui::pane::PaneId::ResultVerification
        {
            return;
        }
        let Some(outcome) = wizard.restore_outcome.as_ref() else {
            return;
        };
        let content_len = 7usize
            .saturating_add(outcome.assessment.issues.len())
            .saturating_add(usize::from(outcome.layout.is_err()));
        wizard
            .post_restore_workbench
            .viewport_mut(crate::tui::pane::PaneId::ResultVerification)
            .scroll_y
            .bottom(content_len, visible_rows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_geometry_lookup_does_not_map_free_space_to_a_partition() {
        use crate::application::disk_layout::{
            DiskLayoutModel, DiskLayoutSegment, DiskRegionKind,
        };
        use crate::application::post_restore::{
            MetadataRestoreOutcome, MetadataRestoreReport, PostRestoreAssessment,
            PostRestorePartition, PostRestorePartitionState,
        };

        let layout = DiskLayoutModel::canonical_plain_plan(
            20_000,
            vec![DiskLayoutSegment {
                label: "P1".into(),
                start_lba: 2_048,
                sector_count: 4_096,
                kind: DiskRegionKind::Plain,
            }],
        )
        .expect("layout");
        let outcome = MetadataRestoreOutcome {
            report: MetadataRestoreReport {
                metadata_restored: true,
                readback_verified: true,
                restored_artifact_ids: vec![],
            },
            assessment: PostRestoreAssessment {
                partitions: vec![PostRestorePartition {
                    index: 1,
                    role: Some("plain".into()),
                    start_lba: 2_048,
                    sector_count: 4_096,
                    filesystem_hint: Some("exfat".into()),
                    detected_filesystem: None,
                    requires_original_key: false,
                    state: PostRestorePartitionState::NeedsFormat,
                    detail: "fixture".into(),
                }],
                issues: vec![],
            },
            partitions: vec![],
            device_state: "plain".into(),
            device_id: String::new(),
            total_sectors: 20_000,
            layout: Ok(layout.clone()),
            format_target_pin: None,
        };
        assert_eq!(partition_selection(&outcome, 0).unwrap().start_lba, 2_048);

        let free = layout
            .segments
            .iter()
            .find(|segment| segment.kind == DiskRegionKind::Free)
            .and_then(crate::tui::disk_layout::DiskCapacitySelection::from_segment)
            .expect("free selection");
        assert_eq!(partition_index_for_selection(&outcome, &free), None);
    }
}
