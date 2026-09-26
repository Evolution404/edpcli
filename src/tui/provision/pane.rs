use super::*;

impl AppState {
    pub fn provision_focused_pane(&self) -> crate::tui::pane::PaneId {
        self.provision.pane_focus.focused()
    }

    pub fn provision_focus_pane(&mut self, pane: crate::tui::pane::PaneId) {
        if pane.is_provision() {
            self.provision.pane_focus.focus(pane);
        }
    }

    pub fn provision_shift_pane(&mut self, reverse: bool) {
        let order = match self.provision.stage {
            ProvisionStage::Review => crate::tui::pane::PaneId::PROVISION_REVIEW_ORDER.as_slice(),
            _ => crate::tui::pane::PaneId::PROVISION_FORM_ORDER.as_slice(),
        };
        self.provision.pane_focus.cycle(order, reverse);
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
                let details = self.provision_layout_editor_lines();
                model.pane_line_count("summary", &details)
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
            self.provision.pane_focus.viewport_mut(pane).scroll_y.top();
        }
    }

    pub fn provision_focused_bottom(&mut self, visible_len: usize) {
        let pane = self.provision.pane_focus.focused();
        if pane == crate::tui::pane::PaneId::ProvisionParameters {
            let count = self.provision_field_count();
            self.provision_move_field(count as isize);
        } else {
            let content_len = self.provision_focused_content_len();
            self.provision
                .pane_focus
                .viewport_mut(pane)
                .scroll_y
                .bottom(content_len, visible_len);
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
        } else {
            self.provision
                .pane_focus
                .viewport_mut(pane)
                .scroll_y
                .move_lines(delta, content_len, visible_len);
        }
    }
}
