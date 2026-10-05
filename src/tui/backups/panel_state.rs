use super::*;
use crate::tui::disk_layout::{DiskCapacitySelection, DiskLayoutModel};
use crate::tui::disk_region_list::DiskRegionListState;
use crate::tui::pane::PaneId;

impl AppState {
    /// Supply the same terminal dimensions used by rendering before navigation.
    pub fn set_viewport_size(&mut self, size: ratatui::layout::Size) {
        if self.shell.viewport_size == size {
            return;
        }
        self.shell.viewport_size = size;
        if self.workspace() == Workspace::Backups {
            let body = crate::tui::backup_layout::shell_areas(
                ratatui::layout::Rect::new(0, 0, size.width, size.height),
                1,
            )[2];
            let (_, summary, _) =
                crate::tui::backup_layout::pane_areas(body, self.backups_focused_pane());
            if let Some(area) = summary {
                let inner = crate::tui::ui::card("", false).inner(area);
                let count =
                    crate::tui::backup_metadata::backup_metadata_lines(self, inner.width).len();
                let max = count.saturating_sub(usize::from(inner.height));
                let scroll = &mut self.pane_viewport_mut(PaneId::BackupSummary).scroll_y;
                scroll.offset = scroll.offset.min(max);
            }
        }
    }

    pub fn navigate_in_viewport(
        &mut self,
        command: NavCommand,
        size: ratatui::layout::Size,
    ) -> StateEffect {
        self.set_viewport_size(size);
        self.navigate(command, self.workspace_navigation_rows(size))
    }

    pub fn workspace_navigation_rows(&self, size: ratatui::layout::Size) -> usize {
        let body = crate::tui::backup_layout::shell_areas(
            ratatui::layout::Rect::new(0, 0, size.width, size.height),
            1,
        )[2];
        if crate::tui::ui::ViewportClass::for_width(size.width)
            == crate::tui::ui::ViewportClass::Compact
        {
            return usize::from(body.height.saturating_sub(6)).max(1);
        }
        match self.workspace() {
            Workspace::Devices => usize::from(
                crate::tui::workspace_layout::device_list_height(
                    body.height,
                    self.visible_device_count(),
                )
                .saturating_sub(6),
            )
            .max(1),
            Workspace::Backups => {
                crate::tui::backup_layout::pane_areas(body, self.backups_focused_pane())
                    .0
                    .map(|area| usize::from(area.height.saturating_sub(6)).max(1))
                    .unwrap_or(1)
            }
            _ => usize::from(size.height.saturating_sub(9)).max(1),
        }
    }

    fn backup_capacity_model(&self) -> Option<&DiskLayoutModel> {
        self.selected_backup()?
            .restore_preview
            .as_ref()?
            .layout
            .as_ref()
    }

    fn backup_panel_target(&self) -> Option<(std::path::PathBuf, Option<String>)> {
        self.selected_backup()
            .map(|backup| (backup.path.clone(), backup.content_sha256.clone()))
    }

    pub(crate) fn backup_capacity_region_state(&self) -> DiskRegionListState {
        let mut state = if self.backups.layout_target == self.backup_panel_target() {
            self.backups.layout_regions.clone()
        } else {
            DiskRegionListState::default()
        };
        if let Some(model) = self.backup_capacity_model() {
            if state.selected_index(model).is_none() {
                state.reconcile(model, 1);
            }
        } else {
            state.clear_selection();
        }
        state
    }

    pub fn backup_capacity_selection(&self) -> Option<DiskCapacitySelection> {
        self.backup_capacity_region_state().selection()
    }

    pub(crate) fn navigate_backup_panel(&mut self, command: NavCommand) -> bool {
        if self.workspace() != Workspace::Backups || self.wizard().is_some() {
            return false;
        }
        let focused = self.backups_focused_pane();
        if !matches!(focused, PaneId::BackupSummary | PaneId::BackupCoverage)
            || !matches!(
                command,
                NavCommand::Up
                    | NavCommand::Down
                    | NavCommand::Top
                    | NavCommand::Bottom
                    | NavCommand::HalfPageUp
                    | NavCommand::HalfPageDown
                    | NavCommand::PageUp
                    | NavCommand::PageDown
            )
        {
            return false;
        }
        let size = self.shell.viewport_size;
        let area = crate::tui::backup_layout::shell_areas(
            ratatui::layout::Rect::new(0, 0, size.width, size.height),
            1,
        )[2];
        let (_, summary, capacity) = crate::tui::backup_layout::pane_areas(area, focused);
        if focused == PaneId::BackupSummary {
            let inner = crate::tui::ui::card("", true).inner(summary.unwrap_or(area));
            let count = crate::tui::backup_metadata::backup_metadata_lines(self, inner.width).len();
            let height = usize::from(inner.height).max(1);
            let scroll = &mut self.pane_viewport_mut(focused).scroll_y;
            match command {
                NavCommand::Up => scroll.line_up(),
                NavCommand::Down => scroll.line_down(count, height),
                NavCommand::Top => scroll.top(),
                NavCommand::Bottom => scroll.bottom(count, height),
                NavCommand::PageUp => scroll.page_up(height),
                NavCommand::PageDown => scroll.page_down(count, height),
                NavCommand::HalfPageUp => scroll.half_page_up(height),
                NavCommand::HalfPageDown => scroll.half_page_down(count, height),
                _ => unreachable!(),
            }
        } else if let Some(model) = self.backup_capacity_model().cloned() {
            let inner = crate::tui::ui::card("", true).inner(capacity.unwrap_or(area));
            let list = crate::tui::backup_layout::capacity_sections(inner)[1];
            let visible = (usize::from(list.height.saturating_sub(1))
                / crate::tui::disk_region_list::region_row_height(list.width))
            .max(1);
            let mut regions = self.backup_capacity_region_state();
            match command {
                NavCommand::Top => {
                    regions.select_index(&model, 0, visible);
                }
                NavCommand::Bottom => {
                    regions.select_index(
                        &model,
                        model
                            .collapsed_tail_model()
                            .segments
                            .len()
                            .saturating_sub(1),
                        visible,
                    );
                }
                _ => {
                    let count =
                        if matches!(command, NavCommand::HalfPageUp | NavCommand::HalfPageDown) {
                            (visible / 2).max(1) as isize
                        } else if matches!(command, NavCommand::PageUp | NavCommand::PageDown) {
                            visible as isize
                        } else {
                            1
                        };
                    let delta = if matches!(
                        command,
                        NavCommand::Up | NavCommand::HalfPageUp | NavCommand::PageUp
                    ) {
                        -count
                    } else {
                        count
                    };
                    regions.move_selection(&model, delta, visible);
                }
            }
            self.backups.layout_regions = regions;
            self.backups.layout_target = self.backup_panel_target();
        }
        true
    }
}
