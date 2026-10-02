use super::*;

impl AppState {
    pub fn provision_push_progress(&mut self, event: crate::application::progress::ProgressEvent) {
        if let Some(run) = self.provision.run.as_mut() {
            run.push(event);
        }
    }

    pub fn provision_scroll_run_log(&mut self, delta: isize, visible: usize) {
        let Some(run) = self.provision.run.as_ref() else {
            return;
        };
        let viewport = self
            .provision
            .pane_focus
            .viewport_mut(crate::tui::pane::PaneId::ProvisionRunLog);
        if viewport.selected.is_none() {
            viewport.scroll_y.bottom(run.log.len(), visible);
        }
        viewport.scroll_y.move_lines(delta, run.log.len(), visible);
        viewport.selected = Some(viewport.scroll_y.offset);
    }

    pub fn provision_follow_run_log(&mut self, visible: usize) {
        let Some(run) = self.provision.run.as_ref() else {
            return;
        };
        let viewport = self
            .provision
            .pane_focus
            .viewport_mut(crate::tui::pane::PaneId::ProvisionRunLog);
        viewport.selected = None;
        viewport.scroll_y.bottom(run.log.len(), visible);
    }

    pub fn provision_run_log_start(&self) -> Option<usize> {
        let viewport = self
            .provision
            .pane_focus
            .viewport(crate::tui::pane::PaneId::ProvisionRunLog);
        viewport.selected.map(|_| viewport.scroll_y.offset)
    }
}
