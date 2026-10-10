use super::*;

#[derive(Debug, Clone)]
pub(super) struct ProvisionFormViewSnapshot {
    pane_focus: crate::tui::pane::PaneFocus,
    field_selected: usize,
    field_cursor: usize,
}

impl ProvisionFormViewSnapshot {
    fn capture(state: &ProvisionState) -> Self {
        Self {
            pane_focus: state.pane_focus.clone(),
            field_selected: state.field_selected,
            field_cursor: state.field_cursor,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ProvisionReviewViewSnapshot {
    pane_focus: crate::tui::pane::PaneFocus,
    region_selected: usize,
    details_expanded: bool,
}

impl ProvisionReviewViewSnapshot {
    fn capture(state: &ProvisionState) -> Self {
        Self {
            pane_focus: state.pane_focus.clone(),
            region_selected: state.review_region_selected,
            details_expanded: state.review_details_expanded,
        }
    }
}

impl AppState {
    pub(crate) fn provision_transition_enter_form(&mut self) {
        self.provision.stage = ProvisionStage::Form;
        self.shell.input_mode = InputMode::Normal;
    }

    pub(super) fn provision_transition_begin_planning(&mut self) {
        if self.provision.stage == ProvisionStage::Form {
            self.provision.form_view_snapshot =
                Some(ProvisionFormViewSnapshot::capture(&self.provision));
        }
        self.provision.stage = ProvisionStage::Planning;
        self.shell.input_mode = InputMode::Normal;
        self.provision.message = None;
    }

    fn provision_restore_form_snapshot(&mut self) {
        if let Some(snapshot) = self.provision.form_view_snapshot.take() {
            self.provision.pane_focus = snapshot.pane_focus;
            self.provision.field_selected = snapshot.field_selected;
            self.provision.field_cursor = snapshot.field_cursor;
        } else {
            self.provision.pane_focus = crate::tui::pane::PaneFocus::provision_form();
            self.provision_sync_cursor_to_end();
        }
    }

    pub(super) fn provision_transition_plan_succeeded(&mut self) {
        self.provision.stage = ProvisionStage::Review;
        self.shell.input_mode = InputMode::Normal;
        self.provision.pane_focus = crate::tui::pane::PaneFocus::provision_review();
        self.provision.review_region_selected = 0;
        self.provision.review_details_expanded = false;
        self.provision.review_view_snapshot = None;
        self.provision.message = None;
    }

    pub(super) fn provision_transition_plan_failed(&mut self, message: String) {
        self.provision.stage = ProvisionStage::Form;
        self.shell.input_mode = InputMode::Normal;
        self.provision_restore_form_snapshot();
        self.provision.review_view_snapshot = None;
        self.provision.message = None;
        self.set_error_notice(message);
    }

    pub fn provision_return_review_to_form(&mut self) {
        self.provision.stage = ProvisionStage::Form;
        self.shell.input_mode = InputMode::Normal;
        self.provision_restore_form_snapshot();
        self.provision.review_view_snapshot = None;
        self.provision.prepared = None;
        self.provision.native_readonly_review = None;
        self.provision.review_region_selected = 0;
        self.provision.review_details_expanded = false;
        self.provision.confirmation.clear();
        self.provision.message = None;
    }

    fn provision_capture_review_snapshot(&mut self) {
        self.provision.review_view_snapshot =
            Some(ProvisionReviewViewSnapshot::capture(&self.provision));
    }

    fn provision_restore_review_snapshot(&mut self) {
        if let Some(snapshot) = self.provision.review_view_snapshot.take() {
            self.provision.pane_focus = snapshot.pane_focus;
            self.provision.review_region_selected = snapshot.region_selected;
            self.provision.review_details_expanded = snapshot.details_expanded;
        } else if !matches!(
            self.provision.pane_focus.focused(),
            crate::tui::pane::PaneId::ProvisionDiskLayout
                | crate::tui::pane::PaneId::ProvisionPartitionPlan
                | crate::tui::pane::PaneId::ProvisionExecutionSummary
        ) {
            self.provision.pane_focus = crate::tui::pane::PaneFocus::provision_review();
        }
    }

    pub(super) fn provision_transition_begin_export_path(&mut self) {
        self.provision_capture_review_snapshot();
        self.provision.stage = ProvisionStage::ExportPath;
        self.shell.input_mode = InputMode::Insert;
        self.provision.message = None;
    }

    pub(super) fn provision_transition_begin_exporting(&mut self) {
        self.provision.stage = ProvisionStage::Exporting;
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = true;
    }

    pub(crate) fn provision_transition_return_to_review(&mut self) {
        self.provision.stage = ProvisionStage::Review;
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = false;
        self.provision_restore_review_snapshot();
    }

    pub(super) fn provision_transition_begin_confirm(&mut self) {
        self.shell.confirmation_offset = 0;
        self.provision_capture_review_snapshot();
        self.provision.stage = ProvisionStage::Confirm;
        self.shell.input_mode = InputMode::Confirm;
        self.provision.confirmation.clear();
        self.provision.message = None;
    }

    pub(super) fn provision_transition_begin_running(&mut self) {
        self.provision.stage = ProvisionStage::Running;
        self.shell.input_mode = InputMode::Normal;
        self.provision.form_view_snapshot = None;
        self.provision.review_view_snapshot = None;
    }

    pub(super) fn provision_transition_finish_running(&mut self) {
        self.provision.stage = ProvisionStage::Result;
        self.shell.input_mode = InputMode::Normal;
    }
}
