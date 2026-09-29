use ratatui::{
    layout::Rect,
    widgets::{Paragraph, Wrap},
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
            let lines = crate::tui::disk_layout::DiskCapacityMap::new(
                model,
                crate::tui::disk_layout::DiskCapacityMapProfile::Full,
            )
            .with_tail(crate::tui::disk_layout::TailExpansion::Collapsed)
            .with_selection(None)
            .with_marker(false)
            .lines(inner.width as usize);
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
    }
}
