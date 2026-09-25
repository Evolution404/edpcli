//! Return targets and selection snapshots for nested TUI workspaces.

use super::{AdvancedInspectPanel, Workspace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationLocation {
    Devices,
    Backups,
    Provision,
    Inspect,
    SectorInspector,
}

impl NavigationLocation {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Devices => "设备列表",
            Self::Backups => "备份列表",
            Self::Provision => "制盘",
            Self::Inspect => "Inspect",
            Self::SectorInspector => "Sector Inspector",
        }
    }

    pub const fn from_workspace(workspace: Workspace) -> Self {
        match workspace {
            Workspace::Devices => Self::Devices,
            Workspace::Backups => Self::Backups,
            Workspace::Provision => Self::Provision,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavigationFrame {
    pub location: NavigationLocation,
    pub selection: usize,
    pub item_count: usize,
    pub panel: Option<AdvancedInspectPanel>,
    pub tree_selection: usize,
    pub detail_scroll: usize,
    pub table_scroll: Option<(super::super::table_layout::TableKind, usize)>,
}

#[derive(Debug, Default, Clone)]
pub struct NavigationStack {
    frames: Vec<NavigationFrame>,
}

impl NavigationStack {
    pub fn push(&mut self, frame: NavigationFrame) {
        self.frames.push(frame);
    }

    pub fn pop(&mut self) -> Option<NavigationFrame> {
        self.frames.pop()
    }

    pub fn back_target(&self) -> Option<NavigationLocation> {
        self.frames.last().map(|frame| frame.location)
    }

    pub fn depth(&self) -> usize {
        self.frames.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreadcrumbModel {
    pub path: Vec<String>,
    pub back_target: Option<NavigationLocation>,
}

impl BreadcrumbModel {
    pub fn display(&self) -> String {
        self.path.join(" > ")
    }

    pub fn escape_hint(&self) -> String {
        match self.back_target {
            Some(target) => format!("Esc 返回：{}", target.label()),
            None => "Esc 返回：当前页".into(),
        }
    }
}
