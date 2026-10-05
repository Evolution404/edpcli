use super::*;

impl AppState {
    pub const fn provision_scheme_picker_open(&self) -> bool {
        self.provision.scheme_picker_open
    }

    pub const fn provision_scheme_selected(&self) -> usize {
        self.provision.scheme_selected
    }

    pub fn provision_close_scheme_picker(&mut self) {
        self.provision.scheme_picker_open = false;
    }

    pub fn provision_move_scheme_picker(&mut self, delta: isize) {
        let len = ProvisionKind::ALL.len();
        if len == 0 {
            self.provision.scheme_selected = 0;
            return;
        }
        self.provision.scheme_selected = if delta < 0 {
            self.provision
                .scheme_selected
                .saturating_sub(delta.unsigned_abs())
        } else {
            self.provision
                .scheme_selected
                .saturating_add(delta as usize)
                .min(len - 1)
        };
    }

    pub fn provision_select_scheme_index(&mut self, index: usize) -> bool {
        if index >= ProvisionKind::ALL.len() {
            return false;
        }
        self.provision.scheme_selected = index;
        true
    }
}
