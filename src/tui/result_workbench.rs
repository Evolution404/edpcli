//! Shared interactive shell for Provision and Restore result surfaces.
//! Business-specific rows and actions stay in their owning workflow modules.

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::disk_layout::{DiskCapacitySelection, DiskLayoutModel};
use super::disk_region_list::{DiskRegionListMode, DiskRegionListState};
use super::pane::{PaneFocus, PaneId};
use super::ui::{ResultTone, ViewportClass};

/// Shared partition-result column contract for both Provision and Restore workbenches.
pub const RESULT_PARTITION_COLUMN_COUNT: usize = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultHero {
    pub title: String,
    pub status: String,
    pub detail: String,
    pub tone: ResultTone,
}

impl ResultHero {
    pub fn new(
        title: impl Into<String>,
        status: impl Into<String>,
        detail: impl Into<String>,
        tone: ResultTone,
    ) -> Self {
        Self {
            title: title.into(),
            status: status.into(),
            detail: detail.into(),
            tone,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultWorkbenchState {
    pane_focus: PaneFocus,
    pub selected_partition: Option<usize>,
    partition_active_column: usize,
    region_list: DiskRegionListState,
}

impl Default for ResultWorkbenchState {
    fn default() -> Self {
        Self {
            pane_focus: PaneFocus::result_workbench(),
            selected_partition: None,
            partition_active_column: 0,
            region_list: DiskRegionListState::default(),
        }
    }
}

impl ResultWorkbenchState {
    pub fn focused_pane(&self) -> PaneId {
        self.pane_focus.focused()
    }

    pub fn focus(&mut self, pane: PaneId) {
        if pane.is_result() {
            self.pane_focus.focus(pane);
        }
    }

    pub fn cycle_pane(&mut self, reverse: bool) {
        self.pane_focus.cycle(&PaneId::RESULT_ORDER, reverse);
    }

    pub fn spatial_pane(&mut self, dx: i8, dy: i8) {
        use PaneId::*;
        let next = match (self.focused_pane(), dx.signum(), dy.signum()) {
            (ResultPartitions, 1, _) => Some(ResultDiskLayout),
            (ResultDiskLayout, -1, _) | (ResultVerification, -1, _) => Some(ResultPartitions),
            (ResultDiskLayout, _, 1) => Some(ResultVerification),
            (ResultVerification, _, -1) => Some(ResultDiskLayout),
            _ => None,
        };
        if let Some(next) = next {
            self.focus(next);
        }
    }

    pub fn partition_active_column(&self, column_count: usize) -> usize {
        self.partition_active_column
            .min(column_count.saturating_sub(1))
    }

    pub fn move_partition_active_column(&mut self, reverse: bool, column_count: usize) -> bool {
        if column_count == 0 {
            self.partition_active_column = 0;
            return false;
        }
        let current = self.partition_active_column(column_count);
        let next = if reverse {
            current.saturating_sub(1)
        } else {
            current.saturating_add(1).min(column_count - 1)
        };
        self.partition_active_column = next;
        next != current
    }

    pub fn viewport(&self, pane: PaneId) -> &super::pane::PaneViewport {
        self.pane_focus.viewport(pane)
    }

    pub fn viewport_mut(&mut self, pane: PaneId) -> &mut super::pane::PaneViewport {
        self.pane_focus.viewport_mut(pane)
    }

    pub fn region_selection(&self) -> Option<DiskCapacitySelection> {
        self.region_list.selection()
    }

    pub fn selected_region_index(&self, model: &DiskLayoutModel) -> Option<usize> {
        self.region_list.selected_index(model)
    }

    pub fn reconcile_regions(&mut self, model: &DiskLayoutModel, visible_rows: usize) {
        self.region_list.reconcile(model, visible_rows);
    }

    pub fn move_region_selection(
        &mut self,
        model: &DiskLayoutModel,
        delta: isize,
        visible_rows: usize,
    ) -> bool {
        self.region_list.move_selection(model, delta, visible_rows)
    }

    pub fn select_region_geometry(
        &mut self,
        model: &DiskLayoutModel,
        selection: &DiskCapacitySelection,
        visible_rows: usize,
    ) -> bool {
        self.region_list
            .select_geometry(model, selection, visible_rows)
    }

    pub(crate) fn region_list_state(&self) -> &DiskRegionListState {
        &self.region_list
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultPaneSlot {
    pub pane: PaneId,
    pub area: Rect,
    pub focused: bool,
}

fn tone_style(tone: ResultTone) -> Style {
    let theme = super::theme::current();
    match tone {
        ResultTone::Primary => theme.body_text(),
        ResultTone::Muted => theme.muted(),
        ResultTone::Accent => theme.accent(),
        ResultTone::Success => theme.success(),
        ResultTone::Warning => theme.warning(),
        ResultTone::Danger => theme.danger(),
    }
}

pub fn render_result_hero(frame: &mut Frame, area: Rect, hero: &ResultHero) {
    let tone = tone_style(hero.tone);
    let block = super::ui::card(hero.title.clone(), false).border_style(tone);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            hero.status.clone(),
            tone.add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            hero.detail.clone(),
            super::theme::current().muted(),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        inner,
    );
}

pub fn render_result_region_list(
    frame: &mut Frame,
    area: Rect,
    state: &ResultWorkbenchState,
    model: &DiskLayoutModel,
    focused: bool,
) {
    super::disk_region_list::render_disk_region_list_body(
        frame,
        area,
        model,
        state.region_list_state(),
        DiskRegionListMode::Interactive { focused },
    );
}

pub fn result_pane_slots(area: Rect, state: &ResultWorkbenchState) -> Vec<ResultPaneSlot> {
    let focused = state.focused_pane();
    let class = ViewportClass::for_width(area.width);
    if matches!(class, ViewportClass::Wide | ViewportClass::UltraWide) && area.height >= 14 {
        let columns = Layout::horizontal([Constraint::Percentage(46), Constraint::Percentage(54)])
            .split(area);
        let right = Layout::vertical([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(columns[1]);
        return vec![
            ResultPaneSlot {
                pane: PaneId::ResultPartitions,
                area: columns[0],
                focused: focused == PaneId::ResultPartitions,
            },
            ResultPaneSlot {
                pane: PaneId::ResultDiskLayout,
                area: right[0],
                focused: focused == PaneId::ResultDiskLayout,
            },
            ResultPaneSlot {
                pane: PaneId::ResultVerification,
                area: right[1],
                focused: focused == PaneId::ResultVerification,
            },
        ];
    }

    vec![ResultPaneSlot {
        pane: focused,
        area,
        focused: true,
    }]
}

/// Draws only the shared workbench shell and returns the pane rectangles.
/// Callers render business-owned pane contents into the returned slots.
pub fn render_result_workbench_shell(
    frame: &mut Frame,
    area: Rect,
    state: &ResultWorkbenchState,
    hero: &ResultHero,
) -> Vec<ResultPaneSlot> {
    let hero_height = area.height.clamp(4, 6);
    let root = Layout::vertical([Constraint::Length(hero_height), Constraint::Min(1)]).split(area);
    render_result_hero(frame, root[0], hero);

    result_pane_slots(root[1], state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workbench_cycles_only_result_panes() {
        let mut state = ResultWorkbenchState::default();
        assert_eq!(state.focused_pane(), PaneId::ResultPartitions);
        state.cycle_pane(false);
        assert_eq!(state.focused_pane(), PaneId::ResultDiskLayout);
        state.cycle_pane(false);
        assert_eq!(state.focused_pane(), PaneId::ResultVerification);
        state.cycle_pane(false);
        assert_eq!(state.focused_pane(), PaneId::ResultPartitions);
        state.focus(PaneId::DevicesList);
        assert_eq!(state.focused_pane(), PaneId::ResultPartitions);
    }

    #[test]
    fn workbench_spatial_navigation_matches_wide_layout() {
        let mut state = ResultWorkbenchState::default();
        state.spatial_pane(1, 0);
        assert_eq!(state.focused_pane(), PaneId::ResultDiskLayout);
        state.spatial_pane(0, 1);
        assert_eq!(state.focused_pane(), PaneId::ResultVerification);
        state.spatial_pane(0, -1);
        assert_eq!(state.focused_pane(), PaneId::ResultDiskLayout);
        state.spatial_pane(-1, 0);
        assert_eq!(state.focused_pane(), PaneId::ResultPartitions);
        state.spatial_pane(0, 1);
        assert_eq!(
            state.focused_pane(),
            PaneId::ResultPartitions,
            "down from a full-height left pane must not invent a new focus target"
        );
    }

    #[test]
    fn workbench_layout_is_three_pane_wide_and_focus_only_narrow() {
        let mut state = ResultWorkbenchState::default();
        let wide = result_pane_slots(Rect::new(0, 0, 160, 30), &state);
        assert_eq!(wide.len(), 3);
        assert_eq!(wide.iter().filter(|slot| slot.focused).count(), 1);

        state.focus(PaneId::ResultVerification);
        let narrow = result_pane_slots(Rect::new(0, 0, 78, 20), &state);
        assert_eq!(narrow.len(), 1);
        assert_eq!(narrow[0].pane, PaneId::ResultVerification);
        assert!(narrow[0].focused);
    }

    #[test]
    fn workbench_partition_column_navigation_is_bounded() {
        let mut state = ResultWorkbenchState::default();
        assert_eq!(state.partition_active_column(7), 0);
        assert!(!state.move_partition_active_column(true, 7));
        assert!(state.move_partition_active_column(false, 7));
        assert_eq!(state.partition_active_column(7), 1);
        for _ in 0..20 {
            state.move_partition_active_column(false, 7);
        }
        assert_eq!(state.partition_active_column(7), 6);
        assert!(!state.move_partition_active_column(false, 7));
    }

    #[test]
    fn workbench_state_keeps_pane_local_viewports() {
        let mut state = ResultWorkbenchState::default();
        state
            .viewport_mut(PaneId::ResultDiskLayout)
            .scroll_y
            .move_lines(4, 20, 5);
        assert_eq!(state.viewport(PaneId::ResultDiskLayout).scroll_y.offset, 4);
        assert_eq!(state.viewport(PaneId::ResultPartitions).scroll_y.offset, 0);
    }
}
