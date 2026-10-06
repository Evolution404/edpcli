use super::*;

impl AppState {
    pub fn disk_layout_tail_expansion(&self) -> crate::tui::disk_layout::TailExpansion {
        self.shell.disk_layout_tail
    }

    pub fn toggle_disk_layout_tail(&mut self) {
        self.shell.disk_layout_tail.toggle();
        self.shell.disk_layout_selected = 0;
    }

    pub fn disk_layout_selected(&self) -> usize {
        self.shell.disk_layout_selected
    }

    pub fn disk_layout_move_selection(&mut self, delta: isize, count: usize) {
        self.shell.disk_layout_selected = if delta < 0 {
            self.shell
                .disk_layout_selected
                .saturating_sub(delta.unsigned_abs())
        } else {
            self.shell
                .disk_layout_selected
                .saturating_add(delta as usize)
        }
        .min(count.saturating_sub(1));
    }
}
