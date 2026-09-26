use super::*;

#[derive(Debug, Clone)]
pub struct ProvisionRunState {
    pub started_at: std::time::Instant,
    pub last_activity_at: std::time::Instant,
    pub latest: Option<crate::application::progress::ProgressEvent>,
    pub log: std::collections::VecDeque<crate::application::progress::ProgressEvent>,
}

impl ProvisionRunState {
    const LOG_CAPACITY: usize = 200;

    pub(super) fn new() -> Self {
        let now = std::time::Instant::now();
        Self {
            started_at: now,
            last_activity_at: now,
            latest: None,
            log: std::collections::VecDeque::with_capacity(Self::LOG_CAPACITY),
        }
    }

    fn push(&mut self, event: crate::application::progress::ProgressEvent) {
        self.last_activity_at = std::time::Instant::now();
        self.latest = Some(event.clone());
        if self.log.len() == Self::LOG_CAPACITY {
            self.log.pop_front();
        }
        self.log.push_back(event);
    }
}

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
}
