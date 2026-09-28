use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeviceInfoNodeKey {
    Identity,
    Capacity,
    LayoutSegment {
        start_lba: u64,
        kind: crate::disk_layout::DiskRegionKind,
    },
    TailGroup,
    Status,
    Backups,
    Protocol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfoTreeNode {
    pub key: DeviceInfoNodeKey,
    pub depth: u8,
    pub label: String,
    pub value: Option<String>,
    pub expandable: bool,
    pub expanded: bool,
}

pub struct DevicesState {
    pub(super) rows: Vec<crate::disk_scan::Row>,
    pub(super) table_view: super::super::table_layout::TableViewData,
    pub(super) scan_pending: bool,
    pub(super) pane_focus: crate::tui::pane::PaneFocus,
    pub(super) info_selected: DeviceInfoNodeKey,
    pub(super) info_expanded: BTreeSet<DeviceInfoNodeKey>,
    // Transitional compatibility for the old flat summary contract. Remove after the
    // remaining legacy summary helpers/tests have migrated to DeviceInfoNodeKey.
    pub(super) summary_selected: usize,
    pub(super) summary_expanded: u8,
}

impl Default for DevicesState {
    fn default() -> Self {
        let mut info_expanded = BTreeSet::new();
        info_expanded.insert(DeviceInfoNodeKey::Capacity);
        Self {
            rows: Vec::new(),
            table_view: super::super::table_layout::TableViewData::default(),
            scan_pending: false,
            pane_focus: crate::tui::pane::PaneFocus::devices(),
            info_selected: DeviceInfoNodeKey::Identity,
            info_expanded,
            summary_selected: 0,
            summary_expanded: DeviceSummarySection::Identity.bit()
                | DeviceSummarySection::Capacity.bit(),
        }
    }
}
