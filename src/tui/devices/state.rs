use super::*;

#[derive(Debug, Clone)]
pub struct DevicesState {
    pub(super) rows: Vec<crate::disk_scan::Row>,
    pub(super) table_view: super::super::table_layout::TableViewData,
    pub(super) scan_pending: bool,
    pub(super) pane_focus: crate::tui::pane::PaneFocus,
    pub(super) summary_selected: usize,
    pub(super) summary_expanded: u8,
}

impl Default for DevicesState {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            table_view: super::super::table_layout::TableViewData::default(),
            scan_pending: false,
            pane_focus: crate::tui::pane::PaneFocus::devices(),
            summary_selected: 0,
            summary_expanded: DeviceSummarySection::Identity.bit()
                | DeviceSummarySection::Capacity.bit(),
        }
    }
}
