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
            info_selected: DeviceInfoNodeKey::Capacity,
            info_expanded,
        }
    }
}

impl AppState {
    pub const fn devices_focused_pane(&self) -> crate::tui::pane::PaneId {
        self.devices.pane_focus.focused()
    }

    pub fn focus_devices_pane(&mut self, pane: crate::tui::pane::PaneId) {
        if pane.is_devices() {
            self.devices.pane_focus.focus(pane);
        }
    }

    pub fn activate_device_for_viewport(&mut self, _width: u16) -> Result<Option<u32>, String> {
        if self.selected_device().is_none() {
            return Err("请先选择设备。".into());
        }
        self.focus_devices_pane(crate::tui::pane::PaneId::DevicesTree);
        Ok(None)
    }

    pub fn device_info_selected_key(&self) -> DeviceInfoNodeKey {
        let selected = self.devices.info_selected;
        if self
            .device_info_tree_rows()
            .iter()
            .any(|row| row.key == selected)
        {
            selected
        } else {
            DeviceInfoNodeKey::Capacity
        }
    }

    pub(super) fn reconcile_device_info_selection(&mut self) {
        self.devices.info_selected = self.device_info_selected_key();
        self.devices
            .pane_focus
            .viewport_mut(crate::tui::pane::PaneId::DevicesDetail)
            .scroll_y
            .top();
    }

    pub fn device_info_tree_rows(&self) -> Vec<DeviceInfoTreeNode> {
        fn size_text(bytes: u64) -> String {
            if bytes >= 1_000_000_000 {
                format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
            } else if bytes >= 1_000_000 {
                format!("{:.2} MB", bytes as f64 / 1_000_000.0)
            } else if bytes >= 1_000 {
                format!("{:.2} kB", bytes as f64 / 1_000.0)
            } else {
                format!("{bytes} B")
            }
        }

        fn reliability_label(row: &crate::disk_scan::Row) -> &'static str {
            use crate::application::media_identity::SerialQuality;
            match row
                .identity_pin
                .as_ref()
                .map(|pin| pin.snapshot.hardware.serial_quality)
            {
                Some(SerialQuality::Usable) => "强",
                Some(SerialQuality::Suspicious) => "中",
                Some(SerialQuality::Missing) if row.device_id.is_some() && row.onlyid.is_some() => {
                    "中"
                }
                Some(SerialQuality::Missing) => "弱",
                None if row.serial.is_some() || row.device_id.is_some() || row.onlyid.is_some() => {
                    "待确认"
                }
                None => "未知",
            }
        }

        fn backup_summary(row: &crate::disk_scan::Row) -> String {
            let status = if row.probe_error.is_some() || row.denied {
                "异常"
            } else {
                "正常"
            };
            match row.n_possible_baks {
                0 => format!("{status} · {}", row.n_baks),
                possible => format!("{status} · {}+{possible}", row.n_baks),
            }
        }

        let expanded = |key| self.devices.info_expanded.contains(&key);
        let mut rows = vec![DeviceInfoTreeNode {
            key: DeviceInfoNodeKey::Capacity,
            depth: 0,
            label: "容量布局".into(),
            value: self.selected_device().map(|row| size_text(row.size)),
            expandable: true,
            expanded: expanded(DeviceInfoNodeKey::Capacity),
        }];

        if expanded(DeviceInfoNodeKey::Capacity) {
            if let Some(row) = self.selected_device() {
                if let Ok(model) = row.canonical_layout() {
                    let collapsed = model.collapsed_tail_model();
                    for segment in &collapsed.segments {
                        let is_tail = segment.kind == crate::disk_layout::DiskRegionKind::Tail;
                        let key = if is_tail {
                            DeviceInfoNodeKey::TailGroup
                        } else {
                            DeviceInfoNodeKey::LayoutSegment {
                                start_lba: segment.start_lba,
                                kind: segment.kind,
                            }
                        };
                        rows.push(DeviceInfoTreeNode {
                            key,
                            depth: 1,
                            label: segment.label.clone(),
                            value: Some(size_text(
                                segment
                                    .sector_count
                                    .saturating_mul(crate::common::SECTOR as u64),
                            )),
                            expandable: is_tail && model.tail_group().is_some(),
                            expanded: is_tail && expanded(DeviceInfoNodeKey::TailGroup),
                        });
                        if is_tail && expanded(DeviceInfoNodeKey::TailGroup) {
                            if let Some(tail) = model.tail_group() {
                                rows.extend(tail.children.iter().map(|child| {
                                    DeviceInfoTreeNode {
                                        key: DeviceInfoNodeKey::LayoutSegment {
                                            start_lba: child.start_lba,
                                            kind: child.kind,
                                        },
                                        depth: 2,
                                        label: child.label.clone(),
                                        value: Some(size_text(
                                            child
                                                .sector_count
                                                .saturating_mul(crate::common::SECTOR as u64),
                                        )),
                                        expandable: false,
                                        expanded: false,
                                    }
                                }));
                            }
                        }
                    }
                }
            }
        }

        rows.extend([
            DeviceInfoTreeNode {
                key: DeviceInfoNodeKey::Identity,
                depth: 0,
                label: "身份与协议".into(),
                value: self
                    .selected_device()
                    .map(|row| reliability_label(row).into()),
                expandable: false,
                expanded: false,
            },
            DeviceInfoTreeNode {
                key: DeviceInfoNodeKey::Status,
                depth: 0,
                label: "状态与备份".into(),
                value: self.selected_device().map(backup_summary),
                expandable: false,
                expanded: false,
            },
        ]);
        rows
    }

    pub fn device_info_move_tree(&mut self, delta: isize) {
        let rows = self.device_info_tree_rows();
        if rows.is_empty() {
            return;
        }
        let selected = self.device_info_selected_key();
        let current = rows.iter().position(|row| row.key == selected).unwrap_or(0);
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize).min(rows.len() - 1)
        };
        self.devices.info_selected = rows[next].key;
        self.devices
            .pane_focus
            .viewport_mut(crate::tui::pane::PaneId::DevicesTree)
            .selected = Some(next);
    }

    pub fn device_info_jump_tree(&mut self, to_end: bool) {
        let rows = self.device_info_tree_rows();
        if rows.is_empty() {
            return;
        }
        let index = if to_end { rows.len() - 1 } else { 0 };
        self.devices.info_selected = rows[index].key;
        let viewport = self
            .devices
            .pane_focus
            .viewport_mut(crate::tui::pane::PaneId::DevicesTree);
        viewport.selected = Some(index);
        viewport.scroll_y.top();
    }

    pub fn device_info_toggle_selected(&mut self) {
        let key = self.device_info_selected_key();
        self.devices.info_selected = key;
        if !matches!(
            key,
            DeviceInfoNodeKey::Capacity | DeviceInfoNodeKey::TailGroup
        ) {
            return;
        }
        if !self.devices.info_expanded.remove(&key) {
            self.devices.info_expanded.insert(key);
        }
        let rows = self.device_info_tree_rows();
        if !rows.iter().any(|row| row.key == self.devices.info_selected) {
            self.devices.info_selected = DeviceInfoNodeKey::Capacity;
        }
    }

    pub fn device_info_focus_detail(&mut self) {
        self.focus_devices_pane(crate::tui::pane::PaneId::DevicesDetail);
        self.pane_viewport_mut(crate::tui::pane::PaneId::DevicesDetail)
            .scroll_y
            .top();
    }

    pub(super) fn device_info_detail_line_count(&self) -> usize {
        match self.device_info_selected_key() {
            DeviceInfoNodeKey::Identity => 20,
            DeviceInfoNodeKey::Capacity => self
                .selected_device()
                .and_then(|row| row.canonical_layout().ok())
                .map(|model| model.collapsed_tail_model().segments.len() + 11)
                .unwrap_or(3),
            DeviceInfoNodeKey::TailGroup => self
                .selected_device()
                .and_then(|row| row.canonical_layout().ok())
                .and_then(|model| model.tail_group().map(|tail| tail.children.len() + 13))
                .unwrap_or(3),
            DeviceInfoNodeKey::LayoutSegment { .. } => 18,
            DeviceInfoNodeKey::Status => self
                .selected_device()
                .map(|row| 12 + row.n_baks + row.n_possible_baks)
                .unwrap_or(12),
            DeviceInfoNodeKey::Backups => 12,
            DeviceInfoNodeKey::Protocol => 20,
        }
    }
}
