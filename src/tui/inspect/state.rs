use super::*;

#[path = "detail_state.rs"]
mod detail_state;
#[path = "search_state.rs"]
mod search_state;
#[path = "sector_state.rs"]
mod sector_state;

pub use detail_state::{InspectDetailRow, INSPECT_DETAIL_HEADINGS};

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
    DiskLayout,
    Tree,
    Overview,
    Detail,
}

impl AdvancedInspectPanel {
    pub const fn pane_id(self) -> crate::tui::pane::PaneId {
        use crate::tui::pane::PaneId;
        match self {
            Self::DiskLayout => PaneId::InspectDiskLayout,
            Self::Tree => PaneId::InspectTree,
            Self::Overview => PaneId::InspectOverview,
            Self::Detail => PaneId::InspectDetail,
        }
    }

    pub const fn from_pane_id(pane: crate::tui::pane::PaneId) -> Option<Self> {
        use crate::tui::pane::PaneId;
        match pane {
            PaneId::InspectDiskLayout => Some(Self::DiskLayout),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvancedInspectTreeAction {
    None,
    SetLazyOffset {
        extent_id: String,
        offset: u64,
        target_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancedInspectTreeRow {
    pub id: String,
    pub label: String,
    pub depth: usize,
    pub kind: crate::application::inspect_tree::InspectNodeKind,
    pub range: crate::application::inspect_tree::InspectNodeRange,
    pub decoder: Option<crate::application::inspect::InspectDecoderKind>,
    pub status: crate::edpb::SemanticStatus,
    pub region_semantic: Option<crate::application::inspect_tree::DiskRegionSemantic>,
    pub expandable: bool,
    pub expanded: bool,
    pub action: AdvancedInspectTreeAction,
}

#[derive(Debug, Clone)]
struct InspectTreeViewModel {
    revision: u64,
    rows: std::sync::Arc<Vec<AdvancedInspectTreeRow>>,
    index: std::collections::HashMap<String, usize>,
}

fn reset_inspect_selected_context(state: &mut AdvancedInspectState) {
    use crate::tui::pane::PaneId;
    for pane in [PaneId::InspectOverview, PaneId::InspectDetail] {
        let viewport = state.pane_focus.viewport_mut(pane);
        viewport.scroll_y.top();
        if pane == PaneId::InspectDetail {
            viewport.selected = Some(0);
        }
    }
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

    pub fn begin_advanced_inspect(&mut self, source: AdvancedInspectSource) -> bool {
        if self.shell.critical_operation {
            self.set_notice("关键操作仍在执行，完成前不能启动全盘检查。");
            return false;
        }
        self.push_navigation_frame(NavigationLocation::from_workspace(self.shell.workspace));
        if let AdvancedInspectSource::Disk(disk) = &source {
            self.shell.pinned_disk = Some(*disk);
        }
        self.shell.workspace = crate::tui::state::Workspace::Inspect;
        let mut expanded = std::collections::BTreeSet::new();
        expanded.insert("device".to_string());
        self.inspect.advanced = Some(AdvancedInspectState {
            source,
            stage: AdvancedInspectStage::Running,
            result: None,
            tree_selected: 0,
            panel: AdvancedInspectPanel::Tree,
            pane_focus: crate::tui::pane::PaneFocus::inspect(),
            expanded,
            lazy_offsets: std::collections::BTreeMap::new(),
            sector: None,
            sector_cache_order: std::collections::VecDeque::new(),
            preview_load: std::collections::BTreeMap::new(),
            detail_expanded: std::collections::BTreeSet::new(),
            tree_revision: 0,
            tree_view_model: std::cell::RefCell::new(None),
            yank_register: None,
            prompt: None,
            message: Some("正在后台读取协议上下文并建立全盘结构树…".into()),
            search_query: String::new(),
            search_matches: Vec::new(),
            search_cursor: 0,
        });
        self.shell.input_mode = InputMode::Normal;
        true
    }

    pub fn advanced_inspect_request(
        &self,
    ) -> Result<
        (
            AdvancedInspectSource,
            crate::application::inspect::AdvancedInspectRequest,
        ),
        String,
    > {
        let state = self
            .inspect
            .advanced
            .as_ref()
            .ok_or_else(|| "全盘检查未打开".to_string())?;
        let device_id_override = match &state.source {
            AdvancedInspectSource::Disk(disk) => self
                .devices
                .rows
                .iter()
                .find(|row| row.disk == *disk)
                .and_then(|row| row.device_id.clone()),
            AdvancedInspectSource::Backup(_) => None,
        };
        Ok((
            state.source.clone(),
            crate::application::inspect::AdvancedInspectRequest {
                mode: crate::application::inspect::AdvancedInspectMode::Meta,
                lbas: (0..crate::common::METADATA_SECTOR_COUNT as u64).collect(),
                export_dir: None,
                device_id_override,
                fail_soft_decode: false,
            },
        ))
    }

    pub fn advanced_inspect_finish(
        &mut self,
        result: Result<crate::application::inspect::AdvancedInspectWorkspace, String>,
    ) {
        let Some(state) = self.inspect.advanced.as_mut() else {
            return;
        };
        state.stage = AdvancedInspectStage::Browser;
        state.tree_selected = 0;
        reset_inspect_selected_context(state);
        state.sector = None;
        state.sector_cache_order.clear();
        state.preview_load.clear();
        state.detail_expanded.clear();
        state.tree_revision = state.tree_revision.wrapping_add(1);
        state.prompt = None;
        state.search_query.clear();
        state.search_matches.clear();
        state.search_cursor = 0;
        match result {
            Ok(workspace) => {
                state.result = Some(workspace);
                state.message = None;
            }
            Err(message) => {
                state.result = None;
                state.message = Some(message);
            }
        }
    }

    pub fn advanced_inspect_tree_rows(&self) -> std::sync::Arc<Vec<AdvancedInspectTreeRow>> {
        const SECTOR_PAGE: usize = 64;

        fn path_id(parent: Option<&str>, node_id: &str) -> String {
            match parent {
                Some(parent) => format!("{parent}/{node_id}"),
                None => node_id.to_string(),
            }
        }

        struct PageRowSpec {
            id: String,
            label: String,
            depth: usize,
            range: crate::application::inspect_tree::InspectNodeRange,
            decoder: Option<crate::application::inspect::InspectDecoderKind>,
            status: crate::edpb::SemanticStatus,
            extent_id: String,
            offset: u64,
            target_id: String,
        }

        fn page_row(spec: PageRowSpec) -> AdvancedInspectTreeRow {
            AdvancedInspectTreeRow {
                id: spec.id,
                label: spec.label,
                depth: spec.depth,
                kind: crate::application::inspect_tree::InspectNodeKind::Group,
                range: spec.range,
                decoder: spec.decoder,
                status: spec.status,
                region_semantic: None,
                expandable: false,
                expanded: false,
                action: AdvancedInspectTreeAction::SetLazyOffset {
                    extent_id: spec.extent_id,
                    offset: spec.offset,
                    target_id: spec.target_id,
                },
            }
        }

        fn push_rows(
            node: &crate::application::inspect_tree::InspectNode,
            depth: usize,
            parent_path: Option<&str>,
            expanded: &std::collections::BTreeSet<String>,
            lazy_offsets: &std::collections::BTreeMap<String, u64>,
            workspace: &crate::application::inspect::AdvancedInspectWorkspace,
            rows: &mut Vec<AdvancedInspectTreeRow>,
        ) {
            use crate::application::inspect_tree::{InspectChildren, InspectNodeKind};

            let row_id = path_id(parent_path, &node.id);
            let expandable = match &node.children {
                InspectChildren::None => false,
                InspectChildren::Materialized(children) => !children.is_empty(),
                InspectChildren::LazySectors { sector_count, .. } => *sector_count > 0,
            };
            let is_expanded = expandable && expanded.contains(&row_id);
            rows.push(AdvancedInspectTreeRow {
                id: row_id.clone(),
                label: node.label.clone(),
                depth,
                kind: node.kind,
                range: node.range,
                decoder: node.decoder,
                status: node.status,
                region_semantic: node.region_semantic,
                expandable,
                expanded: is_expanded,
                action: AdvancedInspectTreeAction::None,
            });
            if !is_expanded {
                return;
            }

            match &node.children {
                InspectChildren::None => {}
                InspectChildren::Materialized(children) => {
                    for child in children {
                        push_rows(
                            child,
                            depth + 1,
                            Some(&row_id),
                            expanded,
                            lazy_offsets,
                            workspace,
                            rows,
                        );
                    }
                }
                InspectChildren::LazySectors {
                    start_lba,
                    sector_count,
                } => {
                    let page = SECTOR_PAGE as u64;
                    let max_offset = sector_count.saturating_sub(1) / page * page;
                    let offset = lazy_offsets
                        .get(&row_id)
                        .copied()
                        .unwrap_or(0)
                        .min(max_offset);
                    if offset > 0 {
                        let previous_offset = offset.saturating_sub(page);
                        let previous_lba = start_lba.saturating_add(previous_offset);
                        rows.push(page_row(PageRowSpec {
                            id: format!("{row_id}/page.prev.{offset}"),
                            label: format!("← 上一页 · 从 LBA{previous_lba}"),
                            depth: depth + 1,
                            range: crate::application::inspect_tree::InspectNodeRange::sectors(
                                previous_lba,
                                page.min(*sector_count - previous_offset),
                            ),
                            decoder: node.decoder,
                            status: node.status,
                            extent_id: row_id.clone(),
                            offset: previous_offset,
                            target_id: format!("{row_id}/sector.{previous_lba}"),
                        }));
                    }

                    let children = node.materialize_sector_page(offset, SECTOR_PAGE);
                    let materialized_count = children.len() as u64;
                    for child in children {
                        let child = if child.kind == InspectNodeKind::Sector {
                            workspace
                                .items
                                .iter()
                                .find(|item| item.lba == child.range.start_lba)
                                .map(|item| {
                                    crate::application::inspect_tree::sector_node_with_fields(
                                        child.range.start_lba,
                                        child.decoder,
                                        child.status,
                                        &item.fields,
                                    )
                                })
                                .unwrap_or(child)
                        } else {
                            child
                        };
                        push_rows(
                            &child,
                            depth + 1,
                            Some(&row_id),
                            expanded,
                            lazy_offsets,
                            workspace,
                            rows,
                        );
                    }

                    let next_offset = offset.saturating_add(materialized_count);
                    if next_offset < *sector_count {
                        let next_lba = start_lba.saturating_add(next_offset);
                        rows.push(page_row(PageRowSpec {
                            id: format!("{row_id}/page.next.{next_offset}"),
                            label: format!("下一页 → · 从 LBA{next_lba}"),
                            depth: depth + 1,
                            range: crate::application::inspect_tree::InspectNodeRange::sectors(
                                next_lba,
                                page.min(*sector_count - next_offset),
                            ),
                            decoder: node.decoder,
                            status: node.status,
                            extent_id: row_id.clone(),
                            offset: next_offset,
                            target_id: format!("{row_id}/sector.{next_lba}"),
                        }));
                    }
                }
            }
        }

        let Some(state) = self.inspect.advanced.as_ref() else {
            return std::sync::Arc::new(Vec::new());
        };
        let Some(workspace) = state.result.as_ref() else {
            return std::sync::Arc::new(Vec::new());
        };
        if let Some(model) = state.tree_view_model.borrow().as_ref() {
            if model.revision == state.tree_revision {
                return model.rows.clone();
            }
        }
        let mut rows = Vec::new();
        push_rows(
            &workspace.topology.root,
            0,
            None,
            &state.expanded,
            &state.lazy_offsets,
            workspace,
            &mut rows,
        );
        let index = rows
            .iter()
            .enumerate()
            .map(|(index, row): (usize, &AdvancedInspectTreeRow)| (row.id.clone(), index))
            .collect();
        let rows = std::sync::Arc::new(rows);
        *state.tree_view_model.borrow_mut() = Some(InspectTreeViewModel {
            revision: state.tree_revision,
            rows: rows.clone(),
            index,
        });
        rows
    }

    pub fn advanced_inspect_tree_index(&self, id: &str) -> Option<usize> {
        let _ = self.advanced_inspect_tree_rows();
        self.inspect
            .advanced
            .as_ref()?
            .tree_view_model
            .borrow()
            .as_ref()?
            .index
            .get(id)
            .copied()
    }

    pub fn advanced_inspect_move_tree(&mut self, delta: isize) {
        let count = self.advanced_inspect_tree_rows().len();
        let Some(state) = self.inspect.advanced.as_mut() else {
            return;
        };
        if state.stage != AdvancedInspectStage::Browser || count == 0 {
            return;
        }
        state.tree_selected = if delta < 0 {
            state.tree_selected.saturating_sub(delta.unsigned_abs())
        } else {
            (state.tree_selected + delta as usize).min(count - 1)
        };
        reset_inspect_selected_context(state);
        state.sector = None;
    }

    pub fn advanced_inspect_tree_top(&mut self) {
        if let Some(state) = self
            .inspect
            .advanced
            .as_mut()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
        {
            state.tree_selected = 0;
            reset_inspect_selected_context(state);
            state.sector = None;
        }
    }

    pub fn advanced_inspect_tree_bottom(&mut self) {
        let count = self.advanced_inspect_tree_rows().len();
        if let Some(state) = self
            .inspect
            .advanced
            .as_mut()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
        {
            state.tree_selected = count.saturating_sub(1);
            reset_inspect_selected_context(state);
            state.sector = None;
        }
    }

    pub fn advanced_inspect_collapse_or_parent(&mut self) {
        let rows = self.advanced_inspect_tree_rows();
        let Some(selected) = self
            .inspect
            .advanced
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
            .map(|state| state.tree_selected)
        else {
            return;
        };
        let Some(row) = rows.get(selected).cloned() else {
            return;
        };

        if row.expandable && row.expanded {
            self.advanced_inspect_toggle_selected();
            return;
        }

        let Some(parent_index) = rows[..selected]
            .iter()
            .rposition(|candidate| candidate.depth < row.depth)
        else {
            return;
        };
        if let Some(state) = self.inspect.advanced.as_mut() {
            state.tree_selected = parent_index;
            reset_inspect_selected_context(state);
            state.sector = None;
        }
    }

    pub fn advanced_inspect_expand_or_child(&mut self) {
        let rows = self.advanced_inspect_tree_rows();
        let Some(selected) = self
            .inspect
            .advanced
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
            .map(|state| state.tree_selected)
        else {
            return;
        };
        let Some(row) = rows.get(selected).cloned() else {
            return;
        };

        if row.expandable && !row.expanded {
            self.advanced_inspect_toggle_selected();
            return;
        }

        let refreshed = self.advanced_inspect_tree_rows();
        let Some(row) = refreshed.get(selected) else {
            return;
        };
        if let Some(child_index) = refreshed
            .iter()
            .enumerate()
            .skip(selected + 1)
            .take_while(|(_, candidate)| candidate.depth > row.depth)
            .find(|(_, candidate)| candidate.depth == row.depth + 1)
            .map(|(index, _)| index)
        {
            if let Some(state) = self.inspect.advanced.as_mut() {
                state.tree_selected = child_index;
                reset_inspect_selected_context(state);
                state.sector = None;
            }
        }
    }

    pub fn advanced_inspect_toggle_selected(&mut self) {
        let rows = self.advanced_inspect_tree_rows();
        let selected = self
            .inspect
            .advanced
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
            .map(|state| state.tree_selected);
        let Some(row) = selected.and_then(|index| rows.get(index)).cloned() else {
            return;
        };

        match row.action {
            AdvancedInspectTreeAction::SetLazyOffset {
                extent_id,
                offset,
                target_id,
            } => {
                if let Some(state) = self.inspect.advanced.as_mut() {
                    state.lazy_offsets.insert(extent_id, offset);
                    state.tree_revision = state.tree_revision.wrapping_add(1);
                }
                if let Some(target_index) = self.advanced_inspect_tree_index(&target_id) {
                    if let Some(state) = self.inspect.advanced.as_mut() {
                        state.tree_selected = target_index;
                        reset_inspect_selected_context(state);
                    }
                }
            }
            AdvancedInspectTreeAction::None => {
                if !row.expandable {
                    return;
                }
                if let Some(state) = self.inspect.advanced.as_mut() {
                    if !state.expanded.remove(&row.id) {
                        state.expanded.insert(row.id.clone());
                    }
                    state.tree_revision = state.tree_revision.wrapping_add(1);
                }
                let count = self.advanced_inspect_tree_rows().len();
                if let Some(state) = self.inspect.advanced.as_mut() {
                    state.tree_selected = state.tree_selected.min(count.saturating_sub(1));
                    reset_inspect_selected_context(state);
                }
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

    pub fn advanced_inspect_selected_sector_lba(&self) -> Option<u64> {
        let state = self
            .inspect
            .advanced
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)?;
        let rows = self.advanced_inspect_tree_rows();
        let row = rows.get(state.tree_selected)?;
        (row.kind == crate::application::inspect_tree::InspectNodeKind::Sector)
            .then_some(row.range.start_lba)
    }

    pub fn advanced_inspect_preview_request(&self) -> Option<(AdvancedInspectSource, u64)> {
        let advanced = self.inspect.advanced.as_ref()?;
        if advanced.stage != AdvancedInspectStage::Browser || advanced.sector.is_some() {
            return None;
        }
        let rows = self.advanced_inspect_tree_rows();
        let row = rows.get(advanced.tree_selected)?;
        if row.kind != crate::application::inspect_tree::InspectNodeKind::Sector
            || row.status != crate::edpb::SemanticStatus::Identified
        {
            return None;
        }
        let lba = row.range.start_lba;
        let workspace = advanced.result.as_ref()?;
        if workspace.items.iter().any(|item| item.lba == lba)
            || !matches!(
                advanced.preview_load.get(&lba),
                None | Some(PreviewLoadState::Idle)
            )
        {
            return None;
        }
        Some((advanced.source.clone(), lba))
    }

    pub fn advanced_inspect_preview_state(&self, lba: u64) -> PreviewLoadState {
        self.inspect
            .advanced
            .as_ref()
            .and_then(|advanced| advanced.preview_load.get(&lba))
            .cloned()
            .unwrap_or_default()
    }

    pub fn advanced_inspect_mark_preview_pending(&mut self, lba: u64) {
        if let Some(advanced) = self.inspect.advanced.as_mut() {
            let attempts = match advanced.preview_load.get(&lba) {
                Some(
                    PreviewLoadState::Pending { attempts }
                    | PreviewLoadState::Failed { attempts, .. },
                ) => attempts.saturating_add(1),
                _ => 1,
            };
            advanced
                .preview_load
                .insert(lba, PreviewLoadState::Pending { attempts });
        }
    }

    pub fn advanced_inspect_retry_selected_preview(
        &mut self,
    ) -> Option<(AdvancedInspectSource, u64)> {
        let lba = self.advanced_inspect_selected_sector_lba()?;
        let source = self.inspect.advanced.as_ref()?.source.clone();
        if !matches!(
            self.advanced_inspect_preview_state(lba),
            PreviewLoadState::Failed { .. }
        ) {
            return None;
        }
        self.advanced_inspect_mark_preview_pending(lba);
        Some((source, lba))
    }

    pub fn advanced_inspect_open_selected_sector(
        &mut self,
    ) -> Option<(AdvancedInspectSource, u64)> {
        let lba = self.advanced_inspect_selected_sector_lba()?;
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
            cursor: 0,
            pending: !ready,
            error: None,
            field_expanded: false,
            pinned_field: None,
        });
        state.panel = AdvancedInspectPanel::Detail;
        state
            .pane_focus
            .focus(crate::tui::pane::PaneId::InspectDetail);
        (!ready).then(|| (state.source.clone(), lba))
    }

    pub fn close_advanced_inspect(&mut self) {
        if self
            .inspect
            .advanced
            .as_ref()
            .is_some_and(|state| state.stage != AdvancedInspectStage::Running)
        {
            self.inspect.advanced = None;
            self.restore_workspace_frame();
        }
    }
}
