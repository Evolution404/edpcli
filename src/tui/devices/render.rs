use super::*;
use crate::tui::{pane::PaneId, ui::ViewportClass};

mod detail_render;
mod list_render;
mod presentation;
mod tree_render;

use detail_render::draw_device_detail;
use list_render::draw_device_list;
use tree_render::draw_device_tree;

pub(super) fn draw_devices(frame: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let class = ViewportClass::for_width(area.width);
    let focus = state.devices_focused_pane();

    if state.devices().is_empty() {
        draw_device_list(frame, area, state);
        return;
    }

    if class == ViewportClass::Compact {
        match focus {
            PaneId::DevicesTree => draw_device_tree(frame, area, state),
            PaneId::DevicesDetail => draw_device_detail(frame, area, state),
            _ => draw_device_list(frame, area, state),
        }
        return;
    }

    let (list_percent, tree_percent) = match class {
        ViewportClass::Standard => (44, 40),
        ViewportClass::Wide | ViewportClass::UltraWide => (40, 30),
        ViewportClass::Compact => unreachable!(),
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(list_percent),
            Constraint::Percentage(100 - list_percent),
        ])
        .split(area);
    let workbench = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(tree_percent),
            Constraint::Percentage(100 - tree_percent),
        ])
        .split(rows[1]);

    draw_device_list(frame, rows[0], state);
    draw_device_tree(frame, workbench[0], state);
    draw_device_detail(frame, workbench[1], state);
}
