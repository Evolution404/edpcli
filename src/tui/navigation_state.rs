use super::*;

impl AppState {
    pub(super) fn switch_workspace(&mut self, workspace: Workspace) {
        if self.shell.workspace == workspace {
            return;
        }
        if self.shell.workspace == Workspace::Devices
            && matches!(workspace, Workspace::Backups | Workspace::Inspect)
        {
            self.shell.pinned_disk = self.selected_device().map(|row| row.disk);
        }
        if workspace == Workspace::Provision {
            if let Some(disk) = self.provision.target_disk {
                self.shell.pinned_disk = Some(disk);
            } else {
                self.shell.pinned_disk = None;
                self.provision.stage = ProvisionStage::SelectDisk;
                self.provision.message = None;
            }
        }
        self.clear_search_matches();
        self.shell.search_query.clear();
        self.shell.input_buffer.clear();
        if self.shell.input_mode == InputMode::Search {
            self.shell.input_mode = InputMode::Normal;
        }
        self.shell.workspace = workspace;
        self.shell.selected = 0;
        if workspace == Workspace::Devices {
            if let Some(disk) = self.provision.target_disk {
                self.shell.selected = self
                    .devices
                    .rows
                    .iter()
                    .position(|row| row.disk == disk)
                    .unwrap_or(0);
            }
        }
        let count = match workspace {
            Workspace::Devices => self.devices.rows.len(),
            Workspace::Backups => self.backups.rows.len(),
            Workspace::Inspect => 0,
            Workspace::Provision => match self.provision.stage {
                ProvisionStage::SelectDisk => self.provision_selectable_devices().count(),
                ProvisionStage::Menu => ProvisionKind::ALL.len(),
                ProvisionStage::Form
                | ProvisionStage::Planning
                | ProvisionStage::Review
                | ProvisionStage::ExportPath
                | ProvisionStage::Exporting
                | ProvisionStage::Confirm
                | ProvisionStage::Running
                | ProvisionStage::Result => 0,
            },
        };
        self.set_item_count(count);
    }

    pub const fn selected(&self) -> usize {
        self.shell.selected
    }

    pub fn navigation(&self) -> &NavigationStack {
        &self.shell.navigation
    }

    pub fn pane_viewport(&self, pane: crate::tui::pane::PaneId) -> &crate::tui::pane::PaneViewport {
        if pane.is_inspect() {
            self.inspect
                .advanced
                .as_ref()
                .expect("Inspect pane requested without Inspect state")
                .pane_focus
                .viewport(pane)
        } else if pane.is_devices() {
            self.devices.pane_focus.viewport(pane)
        } else if pane.is_backups() {
            self.backups.pane_focus.viewport(pane)
        } else {
            self.provision.pane_focus.viewport(pane)
        }
    }

    pub fn pane_viewport_mut(
        &mut self,
        pane: crate::tui::pane::PaneId,
    ) -> &mut crate::tui::pane::PaneViewport {
        if pane.is_inspect() {
            self.inspect
                .advanced
                .as_mut()
                .expect("Inspect pane requested without Inspect state")
                .pane_focus
                .viewport_mut(pane)
        } else if pane.is_devices() {
            self.devices.pane_focus.viewport_mut(pane)
        } else if pane.is_backups() {
            self.backups.pane_focus.viewport_mut(pane)
        } else {
            self.provision.pane_focus.viewport_mut(pane)
        }
    }

    pub const fn backups_focused_pane(&self) -> crate::tui::pane::PaneId {
        self.backups.pane_focus.focused()
    }

    pub fn shift_workspace_pane(&mut self, reverse: bool) {
        match self.shell.workspace {
            Workspace::Devices => self
                .devices
                .pane_focus
                .cycle(&crate::tui::pane::PaneId::DEVICES_ORDER, reverse),
            Workspace::Backups => self
                .backups
                .pane_focus
                .cycle(&crate::tui::pane::PaneId::BACKUPS_ORDER, reverse),
            Workspace::Inspect | Workspace::Provision => {}
        }
    }

    pub fn focus_backups_pane(&mut self, pane: crate::tui::pane::PaneId) {
        if pane.is_backups() {
            self.backups.pane_focus.focus(pane);
        }
    }

    pub fn spatial_workspace_focus(&mut self, dx: i8, dy: i8) {
        use crate::tui::pane::PaneId;
        let (focus, next) = match self.shell.workspace {
            Workspace::Devices => {
                let focus = self.devices.pane_focus.focused();
                let next = match (focus, dx.signum(), dy.signum()) {
                    (PaneId::DevicesList, _, 1) => Some(PaneId::DevicesTree),
                    (PaneId::DevicesTree | PaneId::DevicesDetail, _, -1) => {
                        Some(PaneId::DevicesList)
                    }
                    (PaneId::DevicesTree, 1, _) => Some(PaneId::DevicesDetail),
                    (PaneId::DevicesDetail, -1, _) => Some(PaneId::DevicesTree),
                    _ => None,
                };
                (focus, next)
            }
            Workspace::Backups => {
                let focus = self.backups.pane_focus.focused();
                let next = match (focus, dx.signum(), dy.signum()) {
                    (PaneId::BackupsList, _, 1) => Some(PaneId::BackupSummary),
                    (PaneId::BackupSummary | PaneId::BackupCoverage, _, -1) => {
                        Some(PaneId::BackupsList)
                    }
                    (PaneId::BackupSummary, 1, _) => Some(PaneId::BackupCoverage),
                    (PaneId::BackupCoverage, -1, _) => Some(PaneId::BackupSummary),
                    _ => None,
                };
                (focus, next)
            }
            _ => return,
        };
        if let Some(next) = next {
            if focus.is_devices() {
                self.devices.pane_focus.focus(next);
            } else {
                self.backups.pane_focus.focus(next);
            }
        }
    }

    pub fn push_navigation_frame(&mut self, location: NavigationLocation) {
        let table_kind = match location {
            NavigationLocation::Devices => Some(crate::tui::table_layout::TableKind::Devices),
            NavigationLocation::Backups => Some(crate::tui::table_layout::TableKind::Backups),
            NavigationLocation::Provision => {
                Some(crate::tui::table_layout::TableKind::ProvisionDevices)
            }
            NavigationLocation::Inspect | NavigationLocation::SectorInspector => None,
        };
        let table_scroll = table_kind.map(|kind| (kind, self.table_scroll_offset(kind)));
        let pane_focus = match location {
            NavigationLocation::Provision => Some(self.provision.pane_focus.clone()),
            NavigationLocation::Inspect | NavigationLocation::SectorInspector => self
                .inspect
                .advanced
                .as_ref()
                .map(|state| state.pane_focus.clone()),
            NavigationLocation::Devices => Some(self.devices.pane_focus.clone()),
            NavigationLocation::Backups => Some(self.backups.pane_focus.clone()),
        };
        self.shell.navigation.push(NavigationFrame {
            location,
            selection: self.shell.selected,
            item_count: self.shell.item_count,
            panel: None,
            tree_selection: 0,
            pane_focus,
            table_scroll,
        });
    }

    pub fn pop_navigation_frame(&mut self) -> Option<NavigationFrame> {
        self.shell.navigation.pop()
    }

    pub(super) fn restore_workspace_frame(&mut self) {
        if let Some(frame) = self.pop_navigation_frame() {
            let workspace = match frame.location {
                NavigationLocation::Devices => Workspace::Devices,
                NavigationLocation::Backups => Workspace::Backups,
                NavigationLocation::Provision => Workspace::Provision,
                NavigationLocation::Inspect => Workspace::Inspect,
                NavigationLocation::SectorInspector => return,
            };
            self.switch_workspace(workspace);
            self.shell.selected = frame.selection.min(self.shell.item_count.saturating_sub(1));
            if let Some(pane_focus) = frame.pane_focus {
                match workspace {
                    Workspace::Devices => self.devices.pane_focus = pane_focus,
                    Workspace::Backups => self.backups.pane_focus = pane_focus,
                    Workspace::Provision => self.provision.pane_focus = pane_focus,
                    Workspace::Inspect => {}
                }
            }
            if let Some((kind, offset)) = frame.table_scroll {
                self.shell
                    .horizontal_scroll
                    .entry(kind)
                    .or_default()
                    .set_offset(offset, &crate::tui::table_layout::layout_for(kind));
            }
        } else {
            self.switch_workspace(Workspace::Devices);
        }
    }
}
