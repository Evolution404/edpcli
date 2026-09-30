use super::*;

impl AppState {
    pub fn provision_focused_pane(&self) -> crate::tui::pane::PaneId {
        if self.provision.stage == ProvisionStage::Result {
            self.provision.result_workbench.focused_pane()
        } else {
            self.provision.pane_focus.focused()
        }
    }

    pub fn provision_focus_pane(&mut self, pane: crate::tui::pane::PaneId) {
        if self.provision.stage == ProvisionStage::Result && pane.is_result() {
            self.provision.result_workbench.focus(pane);
        } else if pane.is_provision() {
            self.provision.pane_focus.focus(pane);
        }
    }

    pub fn provision_shift_pane(&mut self, reverse: bool) {
        if self.provision.stage == ProvisionStage::Result {
            self.provision_result_shift_pane(reverse);
            return;
        }
        let order = match self.provision.stage {
            ProvisionStage::Review => crate::tui::pane::PaneId::PROVISION_REVIEW_ORDER.as_slice(),
            _ => crate::tui::pane::PaneId::PROVISION_FORM_ORDER.as_slice(),
        };
        self.provision.pane_focus.cycle(order, reverse);
    }

    pub fn provision_tab_focus(&mut self, reverse: bool) {
        match self.provision.stage {
            ProvisionStage::Form | ProvisionStage::Review => self.provision_shift_pane(reverse),
            ProvisionStage::Result => self.provision_result_shift_pane(reverse),
            _ => {}
        }
    }

    pub fn provision_spatial_focus(&mut self, dx: i8, dy: i8) {
        if self.provision.stage == ProvisionStage::Review {
            self.provision.pane_focus.spatial_provision_review(dx, dy);
        } else {
            self.provision.pane_focus.spatial_provision_form(dx, dy);
        }
    }

    pub fn provision_focused_content_len(&self) -> usize {
        use crate::tui::pane::PaneId;

        match self.provision.pane_focus.focused() {
            PaneId::ProvisionParameters => self.provision_field_count(),
            PaneId::ProvisionDiskLayout => {
                let model = self.provision_layout_model();
                let details = self.provision_layout_editor_details();
                crate::tui::disk_layout::DiskLayoutPresentation::new(
                    &model,
                    crate::tui::disk_layout::DiskLayoutProfile::EditorExact,
                    self.disk_layout_tail_expansion(),
                )
                .pane_line_count("summary", &details)
            }
            PaneId::ProvisionSummary => self.provision_review_summary_lines().len(),
            PaneId::ProvisionChanges => self.provision_review_change_lines().len(),
            _ => 0,
        }
    }

    pub fn provision_focused_top(&mut self) {
        let pane = self.provision.pane_focus.focused();
        if pane == crate::tui::pane::PaneId::ProvisionParameters {
            let count = self.provision_field_count();
            self.provision_move_field(-(count as isize));
        } else {
            if pane == crate::tui::pane::PaneId::ProvisionDiskLayout {
                self.disk_layout_move_selection(-(self.disk_layout_selected() as isize), 1);
            }
            self.provision.pane_focus.viewport_mut(pane).scroll_y.top();
        }
    }

    pub fn provision_focused_bottom(&mut self, visible_len: usize) {
        let pane = self.provision.pane_focus.focused();
        if pane == crate::tui::pane::PaneId::ProvisionParameters {
            let count = self.provision_field_count();
            self.provision_move_field(count as isize);
        } else {
            if pane == crate::tui::pane::PaneId::ProvisionDiskLayout {
                let count = self.provision_layout_model().segments.len();
                self.disk_layout_move_selection(count as isize, count);
            }
            let content_len = self.provision_focused_content_len();
            self.provision
                .pane_focus
                .viewport_mut(pane)
                .scroll_y
                .bottom(content_len, visible_len.max(1));
        }
    }

    pub fn provision_move_focused_vertical(
        &mut self,
        delta: isize,
        visible_len: usize,
        content_len: usize,
    ) {
        let pane = self.provision.pane_focus.focused();
        if pane == crate::tui::pane::PaneId::ProvisionParameters {
            self.provision_move_field(delta);
        } else if pane == crate::tui::pane::PaneId::ProvisionDiskLayout
            && self.provision.stage == ProvisionStage::Form
        {
            let model = self.provision_layout_model();
            let count = crate::tui::disk_layout::DiskLayoutPresentation::new(
                &model,
                crate::tui::disk_layout::DiskLayoutProfile::EditorExact,
                self.disk_layout_tail_expansion(),
            )
            .visible_model()
            .segments
            .len();
            self.disk_layout_move_selection(delta, count);

            let details = self.provision_layout_editor_details();
            let selected_detail = details
                .iter()
                .position(|detail| detail.selected && detail.columns.is_some())
                .unwrap_or(0);
            // Summary + compact capacity map + separating blank precede the detail rows.
            let selected_line = 5usize.saturating_add(selected_detail);
            let visible = visible_len.max(1);
            let viewport = &mut self.provision.pane_focus.viewport_mut(pane).scroll_y;
            if selected_line < viewport.offset {
                viewport.offset = selected_line;
            } else if selected_line >= viewport.offset.saturating_add(visible) {
                viewport.offset = selected_line.saturating_add(1).saturating_sub(visible);
            }
            viewport.clamp(content_len, visible);
        } else {
            self.provision
                .pane_focus
                .viewport_mut(pane)
                .scroll_y
                .move_lines(delta, content_len, visible_len.max(1));
        }
    }
}
