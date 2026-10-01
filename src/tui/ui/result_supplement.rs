use ratatui::{
    layout::{Constraint, Layout, Rect},
    widgets::Paragraph,
    Frame,
};

#[derive(Debug, Clone)]
pub enum ResultSupplement {
    DiskCapacityMap {
        title: String,
        model: crate::tui::disk_layout::DiskLayoutModel,
    },
}

pub(super) fn render_result_supplement(
    frame: &mut Frame,
    area: Rect,
    supplement: &ResultSupplement,
) {
    match supplement {
        ResultSupplement::DiskCapacityMap { title, model } => {
            let block = crate::tui::ui::card(title.clone(), false);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            if inner.width == 0 || inner.height == 0 {
                return;
            }
            let visible_regions = model.collapsed_tail_model().segments.len() as u16;
            let full_height = 6u16
                .saturating_add(1)
                .saturating_add(2)
                .saturating_add(visible_regions);
            let profile = if inner.height >= full_height {
                crate::tui::disk_layout::DiskCapacityMapProfile::Full
            } else {
                crate::tui::disk_layout::DiskCapacityMapProfile::Compact
            };
            let map_height = match profile {
                crate::tui::disk_layout::DiskCapacityMapProfile::Full => 6,
                crate::tui::disk_layout::DiskCapacityMapProfile::Compact => 3,
                crate::tui::disk_layout::DiskCapacityMapProfile::Mini => 1,
            };
            let parts = Layout::vertical([
                Constraint::Length(map_height),
                Constraint::Length(1),
                Constraint::Min(3),
            ])
            .split(inner);
            let lines = crate::tui::disk_layout::DiskCapacityMap::new(model, profile)
                .with_tail(crate::tui::disk_layout::TailExpansion::Collapsed)
                .with_selection(None)
                .with_marker(false)
                .lines(parts[0].width as usize);
            frame.render_widget(Paragraph::new(lines), parts[0]);

            let state = crate::tui::disk_region_list::DiskRegionListState::default();
            crate::tui::disk_region_list::render_disk_region_list(
                frame,
                parts[2],
                model,
                &state,
                crate::tui::disk_region_list::DiskRegionListMode::Readonly,
            );
        }
    }
}
