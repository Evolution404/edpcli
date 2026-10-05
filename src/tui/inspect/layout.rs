use ratatui::layout::{Constraint, Direction, Layout, Rect, Size};

use crate::tui::pane::PaneId;
use crate::tui::state::AdvancedInspectPanel;
use crate::tui::ui::ViewportClass;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InspectBrowserLayout {
    pub breadcrumb_area: Rect,
    pub compact_layout_area: Rect,
    pub tree_area: Option<Rect>,
    pub overview_area: Option<Rect>,
    pub detail_area: Option<Rect>,
}

impl InspectBrowserLayout {
    pub(crate) fn from_content_area(area: Rect, panel: AdvancedInspectPanel) -> Self {
        let browser = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area);
        let content = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(browser[1]);
        let compact_layout_area = content[0];
        let content_area = content[1];
        let class = ViewportClass::for_width(content_area.width);
        let (tree_area, overview_area, detail_area) = if class == ViewportClass::Compact {
            match panel {
                AdvancedInspectPanel::Tree => (Some(content_area), None, None),
                AdvancedInspectPanel::Overview => (None, Some(content_area), None),
                AdvancedInspectPanel::Detail => (None, None, Some(content_area)),
            }
        } else {
            let upper = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(29), Constraint::Percentage(71)])
                .split(content_area);
            let right = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
                .split(upper[1]);
            (Some(upper[0]), Some(right[0]), Some(right[1]))
        };

        Self {
            breadcrumb_area: browser[0],
            compact_layout_area,
            tree_area,
            overview_area,
            detail_area,
        }
    }

    /// Inspect browser always uses the normal shell footer, so the shell contributes
    /// one header row, one navigation row and one footer row.
    pub(crate) fn from_terminal_size(size: Size, panel: AdvancedInspectPanel) -> Self {
        let content_area = Rect::new(0, 2, size.width, size.height.saturating_sub(3));
        Self::from_content_area(content_area, panel)
    }

    pub(crate) fn visible_rows(self, pane: PaneId) -> usize {
        let area = match pane {
            PaneId::InspectTree => self.tree_area,
            PaneId::InspectOverview => self.overview_area,
            PaneId::InspectDetail => self.detail_area,
            _ => None,
        };
        let Some(area) = area else {
            return 1;
        };
        match pane {
            PaneId::InspectOverview => usize::from(area.height.saturating_sub(2)).max(1),
            PaneId::InspectTree | PaneId::InspectDetail => {
                usize::from(area.height.saturating_sub(3)).max(1)
            }
            _ => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_layout_reports_real_pane_rows_instead_of_one_line() {
        let layout = InspectBrowserLayout::from_terminal_size(
            Size::new(240, 60),
            AdvancedInspectPanel::Detail,
        );
        assert!(layout.visible_rows(PaneId::InspectOverview) > 1);
        assert!(layout.visible_rows(PaneId::InspectDetail) > 1);
        assert!(
            layout.visible_rows(PaneId::InspectOverview) < usize::from(60u16.saturating_sub(3))
        );
    }

    #[test]
    fn compact_layout_exposes_only_the_active_pane() {
        let layout = InspectBrowserLayout::from_terminal_size(
            Size::new(70, 24),
            AdvancedInspectPanel::Overview,
        );
        assert!(layout.overview_area.is_some());
        assert!(layout.tree_area.is_none());
        assert!(layout.detail_area.is_none());
        assert!(layout.visible_rows(PaneId::InspectOverview) > 1);
    }
}
