//! Shared backup pane geometry for rendering and keyboard viewport calculations.
use crate::tui::{pane::PaneId, ui::ViewportClass};
use ratatui::layout::{Constraint, Layout, Rect};

pub(crate) fn pane_areas(
    area: Rect,
    focused: PaneId,
) -> (Option<Rect>, Option<Rect>, Option<Rect>) {
    let class = ViewportClass::for_width(area.width);
    if class == ViewportClass::Compact || area.height < 22 {
        return match focused {
            PaneId::BackupSummary => (None, Some(area), None),
            PaneId::BackupCoverage => (None, None, Some(area)),
            _ => (Some(area), None, None),
        };
    }
    let parts = Layout::vertical([
        // Give desktop lists room to grow while keeping the core metadata and
        // capacity layout visible. Rendering and navigation share this geometry.
        Constraint::Length(area.height.saturating_sub(24).max(area.height / 2)),
        Constraint::Min(12),
    ])
    .split(area);
    if class == ViewportClass::Standard {
        if focused == PaneId::BackupCoverage {
            (Some(parts[0]), None, Some(parts[1]))
        } else {
            (Some(parts[0]), Some(parts[1]), None)
        }
    } else {
        let bottom = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(parts[1]);
        (Some(parts[0]), Some(bottom[0]), Some(bottom[1]))
    }
}

pub(crate) fn capacity_sections(inner: Rect) -> [Rect; 3] {
    let map_height = if inner.height >= 16 { 7 } else { 1 };
    let chunks = Layout::vertical([
        Constraint::Length(map_height),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .split(inner);
    [chunks[0], chunks[1], chunks[2]]
}

/// The shell's header, navigation, content and footer geometry.
pub(crate) fn shell_areas(area: Rect, footer_height: u16) -> [Rect; 4] {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(footer_height),
    ])
    .split(area);
    [chunks[0], chunks[1], chunks[2], chunks[3]]
}
