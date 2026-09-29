use super::*;

#[path = "detail_state.rs"]
mod detail_state;
#[path = "jump_state.rs"]
mod jump_state;
#[path = "lifecycle_state.rs"]
mod lifecycle_state;
#[path = "preview_state.rs"]
mod preview_state;
#[path = "search_state.rs"]
mod search_state;
#[path = "sector_state.rs"]
mod sector_state;
#[path = "tree_state.rs"]
mod tree_state;

pub use detail_state::{InspectDetailRow, INSPECT_DETAIL_HEADINGS};
use tree_state::InspectTreeViewModel;
pub use tree_state::{AdvancedInspectTreeAction, AdvancedInspectTreeRow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvancedInspectSource {
    Disk(u32),
    Backup(std::path::PathBuf),
}

impl AdvancedInspectSource {
    pub fn label(&self) -> String {
        match self {
            Self::Disk(disk) => format!("物理盘 disk{disk}"),
            Self::Backup(path) => format!("备份 {}", path.display()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedInspectStage {
    Running,
    Browser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedInspectPanel {
    Tree,
    Overview,
    Detail,
}

impl AdvancedInspectPanel {
    pub const fn pane_id(self) -> crate::tui::pane::PaneId {
        use crate::tui::pane::PaneId;
        match self {
            Self::Tree => PaneId::InspectTree,
            Self::Overview => PaneId::InspectOverview,
            Self::Detail => PaneId::InspectDetail,
        }
    }

    pub const fn from_pane_id(pane: crate::tui::pane::PaneId) -> Option<Self> {
        use crate::tui::pane::PaneId;
        match pane {
            PaneId::InspectTree => Some(Self::Tree),
            PaneId::InspectOverview => Some(Self::Overview),
            PaneId::InspectDetail => Some(Self::Detail),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorInspectMode {
    Raw,
    Decode,
    Mixed,
}

impl SectorInspectMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Raw => "Raw",
            Self::Decode => "Decode",
            Self::Mixed => "Mixed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedInspectJumpUnit {
    Lba,
    ByteOffset,
}

impl AdvancedInspectJumpUnit {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Lba => "LBA",
            Self::ByteOffset => "byte offset",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvancedInspectPrompt {
    Jump {
        unit: AdvancedInspectJumpUnit,
        input: String,
    },
    Search {
        input: String,
    },
}

#[derive(Debug, Clone)]
pub struct SectorInspectorState {
    pub lba: u64,
    pub mode: SectorInspectMode,
    pub cursor: usize,
    pub pending: bool,
    pub error: Option<String>,
    pub field_expanded: bool,
    pub pinned_field: Option<crate::application::inspect::InspectField>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PreviewLoadState {
    #[default]
    Idle,
    Pending {
        attempts: u32,
    },
    Ready,
    Failed {
        message: String,
        attempts: u32,
    },
}

#[derive(Debug, Clone)]
pub struct AdvancedInspectState {
    pub source: AdvancedInspectSource,
    pub stage: AdvancedInspectStage,
    pub result: Option<crate::application::inspect::AdvancedInspectWorkspace>,
    pub tree_selected: usize,
    pub panel: AdvancedInspectPanel,
    pub pane_focus: crate::tui::pane::PaneFocus,
    pub expanded: std::collections::BTreeSet<String>,
    pub lazy_offsets: std::collections::BTreeMap<String, u64>,
    pub sector: Option<SectorInspectorState>,
    pub sector_cache_order: std::collections::VecDeque<u64>,
    pub preview_load: std::collections::BTreeMap<u64, PreviewLoadState>,
    pub detail_expanded: std::collections::BTreeSet<(u64, usize)>,
    tree_revision: u64,
    tree_view_model: std::cell::RefCell<Option<InspectTreeViewModel>>,
    pub yank_register: Option<String>,
    pub prompt: Option<AdvancedInspectPrompt>,
    pub message: Option<String>,
    search_query: String,
    search_matches: Vec<search_state::AdvancedInspectSearchTarget>,
    search_cursor: usize,
}

#[derive(Debug, Clone, Default)]
pub struct InspectState {
    pub(super) advanced: Option<AdvancedInspectState>,
}

impl AppState {
    pub fn advanced_inspect_breadcrumb(&self) -> Option<BreadcrumbModel> {
        let advanced = self.inspect.advanced.as_ref()?;
        let source = match &advanced.source {
            AdvancedInspectSource::Disk(disk) => vec!["设备".into(), format!("disk{disk}")],
            AdvancedInspectSource::Backup(path) => vec![
                "备份".into(),
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ],
        };
        let mut path = source;
        path.push("Inspect".into());
        if let Some(row) = self
            .advanced_inspect_tree_rows()
            .get(advanced.tree_selected)
        {
            let rows = self.advanced_inspect_tree_rows();
            let ids = row.id.split('/').collect::<Vec<_>>();
            for prefix_len in 2..=ids.len() {
                let id = ids[..prefix_len].join("/");
                if let Some(ancestor) = rows.iter().find(|candidate| candidate.id == id) {
                    path.push(ancestor.label.clone());
                }
            }
        }
        if advanced.sector.is_some() {
            path.push("Sector Inspector".into());
        }
        Some(BreadcrumbModel {
            path,
            back_target: self.shell.navigation.back_target(),
        })
    }

    pub fn advanced_inspect(&self) -> Option<&AdvancedInspectState> {
        self.inspect.advanced.as_ref()
    }

    pub fn advanced_inspect_focused_pane(&self) -> Option<crate::tui::pane::PaneId> {
        self.inspect
            .advanced
            .as_ref()
            .map(|state| state.pane_focus.focused())
    }

    pub fn advanced_inspect_focus_pane(&mut self, pane: crate::tui::pane::PaneId) {
        let Some(panel) = AdvancedInspectPanel::from_pane_id(pane) else {
            return;
        };
        if let Some(state) = self
            .inspect
            .advanced
            .as_mut()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
        {
            state.pane_focus.focus(pane);
            state.panel = panel;
        }
    }

    pub fn advanced_inspect_spatial_focus(&mut self, dx: i8, dy: i8) {
        if let Some(state) = self
            .inspect
            .advanced
            .as_mut()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
        {
            state.pane_focus.spatial_inspect(dx, dy);
            if let Some(panel) = AdvancedInspectPanel::from_pane_id(state.pane_focus.focused()) {
                state.panel = panel;
            }
        }
    }

    pub fn advanced_inspect_shift_panel(&mut self, reverse: bool) {
        if let Some(state) = self
            .inspect
            .advanced
            .as_mut()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
        {
            state
                .pane_focus
                .cycle(&crate::tui::pane::PaneId::INSPECT_ORDER, reverse);
            if let Some(panel) = AdvancedInspectPanel::from_pane_id(state.pane_focus.focused()) {
                state.panel = panel;
            }
        }
    }

    pub fn advanced_inspect_enter_selected(&mut self) {
        // Sector rows are opened by `advanced_inspect_open_selected_sector()`
        // so the event loop can schedule the read without putting I/O in state.
        let rows = self.advanced_inspect_tree_rows();
        let selected = self
            .inspect
            .advanced
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
            .map(|state| state.tree_selected);
        if selected.and_then(|index| rows.get(index)).is_none() {
            return;
        }
        if let Some(state) = self.inspect.advanced.as_mut() {
            state.panel = AdvancedInspectPanel::Overview;
            state
                .pane_focus
                .focus(crate::tui::pane::PaneId::InspectOverview);
        }
    }

    pub fn advanced_inspect_selected_field(
        &self,
    ) -> Option<crate::application::inspect::InspectField> {
        let state = self
            .inspect
            .advanced
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)?;
        let rows = self.advanced_inspect_tree_rows();
        let row = rows.get(state.tree_selected)?;
        if row.kind != crate::application::inspect_tree::InspectNodeKind::Field {
            return None;
        }
        let range = row.range.byte_range?;
        state
            .result
            .as_ref()?
            .items
            .iter()
            .flat_map(|item| item.fields.iter())
            .find(|field| field.range == range)
            .cloned()
    }

    pub fn advanced_inspect_view_selected_field(&mut self) -> bool {
        let Some(field) = self.advanced_inspect_selected_field() else {
            return false;
        };
        let lba = field.range.start_lba();
        if self.advanced_inspect_jump_lba(lba).is_err() {
            return false;
        }
        let index = self
            .advanced_inspect_detail_rows()
            .iter()
            .position(|row| row.range == Some(field.range) && row.child_index.is_none())
            .unwrap_or(0);
        if let Some(state) = self.inspect.advanced.as_mut() {
            state.panel = AdvancedInspectPanel::Detail;
            state
                .pane_focus
                .focus(crate::tui::pane::PaneId::InspectDetail);
            let viewport = state
                .pane_focus
                .viewport_mut(crate::tui::pane::PaneId::InspectDetail);
            viewport.selected = Some(index);
            viewport.scroll_y.offset = index;
        }
        true
    }

    pub fn advanced_inspect_open_selected_field(&mut self) -> Option<(AdvancedInspectSource, u64)> {
        let field = self.advanced_inspect_selected_field()?;
        self.open_inspect_field_at(field.clone(), field.range.start)
    }

    pub fn advanced_inspect_detail_open_selected(
        &mut self,
    ) -> Option<(AdvancedInspectSource, u64)> {
        let row = self.advanced_inspect_detail_selected_row()?;
        let range = row.range?;
        let lba = self.advanced_inspect_selected_sector_lba()?;
        let field = self
            .inspect
            .advanced
            .as_ref()?
            .result
            .as_ref()?
            .items
            .iter()
            .find(|item| item.lba == lba)?
            .fields
            .get(row.field_index)?
            .clone();
        self.open_inspect_field_at(field, range.start)
    }

    fn open_inspect_field_at(
        &mut self,
        field: crate::application::inspect::InspectField,
        absolute: u64,
    ) -> Option<(AdvancedInspectSource, u64)> {
        let lba = absolute / crate::common::SECTOR as u64;
        let cursor = (absolute % crate::common::SECTOR as u64) as usize;
        let (panel, tree_selection, pane_focus) = {
            let state = self.inspect.advanced.as_ref()?;
            (state.panel, state.tree_selected, state.pane_focus.clone())
        };
        self.shell.navigation.push(NavigationFrame {
            location: NavigationLocation::Inspect,
            selection: self.shell.selected,
            item_count: self.shell.item_count,
            panel: Some(panel),
            tree_selection,
            pane_focus: Some(pane_focus),
            table_scroll: None,
        });
        let state = self.inspect.advanced.as_mut()?;
        let ready = state.result.as_ref().is_some_and(|workspace| {
            workspace.items.iter().any(|item| {
                item.lba == lba && (item.decoded.is_some() || item.decode_error.is_some())
            })
        });
        state.sector = Some(SectorInspectorState {
            lba,
            mode: SectorInspectMode::Mixed,
            cursor,
            pending: !ready,
            error: None,
            field_expanded: true,
            pinned_field: Some(field),
        });
        state.panel = AdvancedInspectPanel::Detail;
        state
            .pane_focus
            .focus(crate::tui::pane::PaneId::InspectDetail);
        (!ready).then(|| (state.source.clone(), lba))
    }
}
