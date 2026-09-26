//! Unified focused-pane state and viewport primitives for multi-pane TUI workspaces.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PaneId {
    InspectDiskLayout,
    InspectTree,
    InspectOverview,
    InspectDetail,
    ProvisionParameters,
    ProvisionDiskLayout,
    ProvisionSummary,
    ProvisionChanges,
}

impl PaneId {
    pub const INSPECT_ORDER: [Self; 4] = [
        Self::InspectDiskLayout,
        Self::InspectTree,
        Self::InspectOverview,
        Self::InspectDetail,
    ];
    pub const PROVISION_FORM_ORDER: [Self; 2] =
        [Self::ProvisionParameters, Self::ProvisionDiskLayout];
    pub const PROVISION_REVIEW_ORDER: [Self; 3] = [
        Self::ProvisionSummary,
        Self::ProvisionDiskLayout,
        Self::ProvisionChanges,
    ];

    pub const fn is_inspect(self) -> bool {
        matches!(
            self,
            Self::InspectDiskLayout
                | Self::InspectTree
                | Self::InspectOverview
                | Self::InspectDetail
        )
    }

    pub const fn is_provision(self) -> bool {
        !self.is_inspect()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VerticalViewport {
    pub offset: usize,
}

impl VerticalViewport {
    pub fn line_up(&mut self) {
        self.offset = self.offset.saturating_sub(1);
    }

    pub fn line_down(&mut self, content_len: usize, visible_len: usize) {
        self.offset = self.offset.saturating_add(1);
        self.clamp(content_len, visible_len);
    }

    pub fn move_lines(&mut self, delta: isize, content_len: usize, visible_len: usize) {
        if delta < 0 {
            self.offset = self.offset.saturating_sub(delta.unsigned_abs());
        } else {
            self.offset = self.offset.saturating_add(delta as usize);
        }
        self.clamp(content_len, visible_len);
    }

    pub fn half_page_up(&mut self, visible_len: usize) {
        self.offset = self.offset.saturating_sub((visible_len / 2).max(1));
    }

    pub fn half_page_down(&mut self, content_len: usize, visible_len: usize) {
        self.offset = self.offset.saturating_add((visible_len / 2).max(1));
        self.clamp(content_len, visible_len);
    }

    pub fn page_up(&mut self, visible_len: usize) {
        self.offset = self.offset.saturating_sub(visible_len.max(1));
    }

    pub fn page_down(&mut self, content_len: usize, visible_len: usize) {
        self.offset = self.offset.saturating_add(visible_len.max(1));
        self.clamp(content_len, visible_len);
    }

    pub fn top(&mut self) {
        self.offset = 0;
    }

    pub fn bottom(&mut self, content_len: usize, visible_len: usize) {
        self.offset = content_len.saturating_sub(visible_len.max(1));
    }

    pub fn clamp(&mut self, content_len: usize, visible_len: usize) {
        self.offset = self
            .offset
            .min(content_len.saturating_sub(visible_len.max(1)));
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaneViewport {
    pub scroll_y: VerticalViewport,
    pub scroll_x: usize,
    pub selected: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneFocus {
    focused: PaneId,
    viewports: BTreeMap<PaneId, PaneViewport>,
}

impl PaneFocus {
    pub fn new(focused: PaneId, panes: impl IntoIterator<Item = PaneId>) -> Self {
        let mut viewports = BTreeMap::new();
        for pane in panes {
            viewports.entry(pane).or_default();
        }
        viewports.entry(focused).or_default();
        Self { focused, viewports }
    }

    pub fn inspect() -> Self {
        Self::new(PaneId::InspectDiskLayout, PaneId::INSPECT_ORDER)
    }

    pub fn provision_form() -> Self {
        Self::new(PaneId::ProvisionParameters, PaneId::PROVISION_FORM_ORDER)
    }

    pub fn provision_review() -> Self {
        Self::new(PaneId::ProvisionSummary, PaneId::PROVISION_REVIEW_ORDER)
    }

    pub const fn focused(&self) -> PaneId {
        self.focused
    }

    pub fn focus(&mut self, pane: PaneId) {
        self.viewports.entry(pane).or_default();
        self.focused = pane;
    }

    pub fn viewport(&self, pane: PaneId) -> &PaneViewport {
        self.viewports
            .get(&pane)
            .expect("pane viewport must be registered")
    }

    pub fn viewport_mut(&mut self, pane: PaneId) -> &mut PaneViewport {
        self.viewports.entry(pane).or_default()
    }

    pub fn cycle(&mut self, order: &[PaneId], reverse: bool) {
        if order.is_empty() {
            return;
        }
        let index = order
            .iter()
            .position(|pane| *pane == self.focused)
            .unwrap_or(0);
        let next = if reverse {
            index.checked_sub(1).unwrap_or(order.len() - 1)
        } else {
            (index + 1) % order.len()
        };
        self.focus(order[next]);
    }

    pub fn spatial_inspect(&mut self, dx: i8, dy: i8) {
        use PaneId::*;
        let next = match (self.focused, dx.signum(), dy.signum()) {
            (InspectDiskLayout, _, 1) => Some(InspectTree),
            (InspectTree, _, -1) | (InspectOverview, _, -1) | (InspectDetail, _, -1) => {
                Some(InspectDiskLayout)
            }
            (InspectTree, 1, _) => Some(InspectOverview),
            (InspectOverview, -1, _) => Some(InspectTree),
            (InspectOverview, 1, _) => Some(InspectDetail),
            (InspectDetail, -1, _) => Some(InspectOverview),
            _ => None,
        };
        if let Some(next) = next {
            self.focus(next);
        }
    }

    pub fn spatial_provision_form(&mut self, dx: i8, _dy: i8) {
        match (self.focused, dx.signum()) {
            (PaneId::ProvisionParameters, 1) => self.focus(PaneId::ProvisionDiskLayout),
            (PaneId::ProvisionDiskLayout, -1) => self.focus(PaneId::ProvisionParameters),
            _ => {}
        }
    }

    pub fn spatial_provision_review(&mut self, dx: i8, dy: i8) {
        use PaneId::*;
        let next = match (self.focused, dx.signum(), dy.signum()) {
            (ProvisionSummary, 1, _) | (ProvisionSummary, _, 1) => Some(ProvisionDiskLayout),
            (ProvisionDiskLayout, -1, _) | (ProvisionDiskLayout, _, -1) => Some(ProvisionSummary),
            (ProvisionDiskLayout, 1, _) | (ProvisionDiskLayout, _, 1) => Some(ProvisionChanges),
            (ProvisionChanges, -1, _) | (ProvisionChanges, _, -1) => Some(ProvisionDiskLayout),
            _ => None,
        };
        if let Some(next) = next {
            self.focus(next);
        }
    }
}
