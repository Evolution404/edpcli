use crate::tui::disk_layout::{DiskCapacitySelection, DiskLayoutModel, DiskLayoutSegment};
use crate::tui::pane::VerticalViewport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiskRegionListMode {
    Readonly,
    Interactive { focused: bool },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DiskRegionListState {
    selection: Option<DiskCapacitySelection>,
    pub viewport: VerticalViewport,
}

impl DiskRegionListState {
    pub fn selection(&self) -> Option<DiskCapacitySelection> {
        self.selection.clone()
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.viewport.top();
    }

    pub fn selected_index(&self, model: &DiskLayoutModel) -> Option<usize> {
        let visible = model.collapsed_tail_model();
        let selection = self.selection.as_ref()?;
        visible
            .segments
            .iter()
            .position(|segment| segment_matches_selection(segment, selection))
    }

    pub fn select_index(
        &mut self,
        model: &DiskLayoutModel,
        index: usize,
        visible_rows: usize,
    ) -> bool {
        let visible = model.collapsed_tail_model();
        let Some(segment) = visible.segments.get(index) else {
            return false;
        };
        self.selection = DiskCapacitySelection::from_segment(segment);
        self.ensure_visible(index, visible.segments.len(), visible_rows);
        self.selection.is_some()
    }

    pub fn move_selection(
        &mut self,
        model: &DiskLayoutModel,
        delta: isize,
        visible_rows: usize,
    ) -> bool {
        let visible = model.collapsed_tail_model();
        if visible.segments.is_empty() {
            self.clear_selection();
            return false;
        }
        let current = self
            .selection
            .as_ref()
            .and_then(|selection| {
                visible
                    .segments
                    .iter()
                    .position(|segment| segment_matches_selection(segment, selection))
            })
            .unwrap_or(0);
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize)
        }
        .min(visible.segments.len().saturating_sub(1));
        self.selection = DiskCapacitySelection::from_segment(&visible.segments[next]);
        self.ensure_visible(next, visible.segments.len(), visible_rows);
        true
    }

    pub fn select_geometry(
        &mut self,
        model: &DiskLayoutModel,
        selection: &DiskCapacitySelection,
        visible_rows: usize,
    ) -> bool {
        let visible = model.collapsed_tail_model();
        let Some(index) = visible
            .segments
            .iter()
            .position(|segment| segment_matches_selection(segment, selection))
        else {
            return false;
        };
        self.selection = Some(selection.clone());
        self.ensure_visible(index, visible.segments.len(), visible_rows);
        true
    }

    pub fn reconcile(&mut self, model: &DiskLayoutModel, visible_rows: usize) {
        let visible = model.collapsed_tail_model();
        if visible.segments.is_empty() {
            self.clear_selection();
            return;
        }
        if let Some(selection) = self.selection.as_ref() {
            if let Some(index) = visible
                .segments
                .iter()
                .position(|segment| segment_matches_selection(segment, selection))
            {
                self.ensure_visible(index, visible.segments.len(), visible_rows);
                return;
            }
        }
        let _ = self.select_index(model, 0, visible_rows);
    }

    fn ensure_visible(&mut self, index: usize, content_len: usize, visible_rows: usize) {
        let visible_rows = visible_rows.max(1);
        if index < self.viewport.offset {
            self.viewport.offset = index;
        } else if index >= self.viewport.offset.saturating_add(visible_rows) {
            self.viewport.offset = index.saturating_add(1).saturating_sub(visible_rows);
        }
        self.viewport.clamp(content_len, visible_rows);
    }
}

pub(super) fn segment_matches_selection(
    segment: &DiskLayoutSegment,
    selection: &DiskCapacitySelection,
) -> bool {
    segment.start_lba == selection.start_lba
        && segment.kind == selection.kind
        && segment.end_exclusive().ok() == Some(selection.end_exclusive)
}
