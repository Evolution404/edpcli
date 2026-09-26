use super::*;

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum AdvancedInspectSearchTarget {
    Topology(Vec<String>),
    CachedSector {
        lba: u64,
        relative_path: Vec<String>,
    },
}

fn parse_inspect_jump_number(input: &str) -> Result<u64, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Jump 输入不能为空".into());
    }
    if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        if hex.is_empty() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("十六进制 Jump 必须使用 0x 前缀并只包含 0-9/A-F".into());
        }
        return u64::from_str_radix(hex, 16).map_err(|_| "Jump 数值超出 u64 范围".into());
    }
    if !input.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Jump 必须是十进制整数或 0x 前缀十六进制整数".into());
    }
    input
        .parse::<u64>()
        .map_err(|_| "Jump 数值超出 u64 范围".into())
}

fn inspect_row_path(node_path: &[String]) -> Vec<String> {
    let mut rows = Vec::with_capacity(node_path.len());
    let mut current = String::new();
    for node_id in node_path {
        if current.is_empty() {
            current.push_str(node_id);
        } else {
            current.push('/');
            current.push_str(node_id);
        }
        rows.push(current.clone());
    }
    rows
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

pub const INSPECT_DETAIL_HEADINGS: [&str; 11] = [
    "Offset",
    "Len",
    "Group",
    "Field",
    "Value",
    "Raw",
    "Decoded",
    "Logical",
    "Type",
    "Status",
    "Transform",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectDetailRow {
    pub cells: [String; 11],
    pub range: Option<crate::application::inspect::AbsoluteByteRange>,
    pub field_index: usize,
    pub child_index: Option<usize>,
}

#[derive(Debug, Clone)]
struct InspectTreeViewModel {
    revision: u64,
    rows: std::sync::Arc<Vec<AdvancedInspectTreeRow>>,
    index: std::collections::HashMap<String, usize>,
}

fn inspect_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
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
    search_matches: Vec<AdvancedInspectSearchTarget>,
    search_cursor: usize,
}

impl AppState {
    pub fn advanced_inspect_breadcrumb(&self) -> Option<BreadcrumbModel> {
        let advanced = self.advanced_inspect.as_ref()?;
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
            back_target: self.navigation.back_target(),
        })
    }

    pub fn advanced_inspect(&self) -> Option<&AdvancedInspectState> {
        self.advanced_inspect.as_ref()
    }

    pub fn advanced_inspect_focused_pane(&self) -> Option<crate::tui::pane::PaneId> {
        self.advanced_inspect
            .as_ref()
            .map(|state| state.pane_focus.focused())
    }

    pub fn advanced_inspect_focus_pane(&mut self, pane: crate::tui::pane::PaneId) {
        let Some(panel) = AdvancedInspectPanel::from_pane_id(pane) else {
            return;
        };
        if let Some(state) = self
            .advanced_inspect
            .as_mut()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
        {
            state.pane_focus.focus(pane);
            state.panel = panel;
        }
    }

    pub fn advanced_inspect_spatial_focus(&mut self, dx: i8, dy: i8) {
        if let Some(state) = self
            .advanced_inspect
            .as_mut()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
        {
            state.pane_focus.spatial_inspect(dx, dy);
            if let Some(panel) = AdvancedInspectPanel::from_pane_id(state.pane_focus.focused()) {
                state.panel = panel;
            }
        }
    }

    pub fn advanced_inspect_detail_rows(&self) -> Vec<InspectDetailRow> {
        use crate::application::inspect::AbsoluteByteRange;
        use crate::application::inspect_tree::InspectNodeKind;

        let Some(state) = self.advanced_inspect.as_ref() else {
            return Vec::new();
        };
        let rows = self.advanced_inspect_tree_rows();
        let Some(selected) = rows.get(state.tree_selected) else {
            return Vec::new();
        };
        if selected.kind != InspectNodeKind::Sector {
            return Vec::new();
        }
        let lba = selected.range.start_lba;
        let Some(item) = state
            .result
            .as_ref()
            .and_then(|workspace| workspace.items.iter().find(|item| item.lba == lba))
        else {
            return Vec::new();
        };
        let mut projected = Vec::new();
        for (field_index, field) in item.fields.iter().enumerate() {
            let has_children = !field.children.is_empty();
            let expanded = state.detail_expanded.contains(&(lba, field_index));
            let marker = if !has_children {
                ""
            } else if expanded {
                "▾ "
            } else {
                "▸ "
            };
            let sector_start = lba.saturating_mul(crate::common::SECTOR as u64);
            let offset = field.range.start.saturating_sub(sector_start);
            projected.push(InspectDetailRow {
                cells: [
                    format!("0x{offset:03X}"),
                    field.range.len().to_string(),
                    field.group.clone().unwrap_or_default(),
                    format!("{marker}{}", field.label),
                    field.value.clone(),
                    inspect_hex(&field.raw),
                    inspect_hex(&field.decoded),
                    field
                        .field_logical
                        .as_deref()
                        .map(inspect_hex)
                        .unwrap_or_default(),
                    format!("{:?}", field.field_type),
                    format!("{:?}", field.status),
                    field
                        .transform
                        .map(|transform| format!("{transform:?}"))
                        .unwrap_or_default(),
                ],
                range: Some(field.range),
                field_index,
                child_index: None,
            });
            if expanded {
                for (child_index, child) in field.children.iter().enumerate() {
                    let relative = child.relative_range.filter(|(start, end)| {
                        start < end && *end <= field.raw.len() && *end <= field.decoded.len()
                    });
                    let range = relative.and_then(|(start, end)| {
                        Some(AbsoluteByteRange {
                            start: field.range.start.checked_add(start as u64)?,
                            end_exclusive: field.range.start.checked_add(end as u64)?,
                        })
                    });
                    let (offset, len, raw, decoded) = if let Some((start, end)) = relative {
                        (
                            format!("0x{:03X}", offset.saturating_add(start as u64)),
                            (end - start).to_string(),
                            inspect_hex(&field.raw[start..end]),
                            inspect_hex(&field.decoded[start..end]),
                        )
                    } else {
                        (String::new(), String::new(), String::new(), String::new())
                    };
                    projected.push(InspectDetailRow {
                        cells: [
                            offset,
                            len,
                            field.group.clone().unwrap_or_default(),
                            format!("  {}", child.label),
                            child.value.clone(),
                            raw,
                            decoded,
                            String::new(),
                            String::new(),
                            format!("{:?}", field.status),
                            String::new(),
                        ],
                        range,
                        field_index,
                        child_index: Some(child_index),
                    });
                }
            }
        }
        projected
    }

    pub fn advanced_inspect_detail_selected_row(&self) -> Option<InspectDetailRow> {
        let index = self
            .advanced_inspect
            .as_ref()?
            .pane_focus
            .viewport(crate::tui::pane::PaneId::InspectDetail)
            .selected
            .unwrap_or(0);
        self.advanced_inspect_detail_rows().get(index).cloned()
    }

    pub fn advanced_inspect_detail_toggle_selected(&mut self) {
        let Some(row) = self.advanced_inspect_detail_selected_row() else {
            return;
        };
        if row.child_index.is_some() {
            return;
        }
        let Some(lba) = self.advanced_inspect_selected_sector_lba() else {
            return;
        };
        let Some(state) = self.advanced_inspect.as_mut() else {
            return;
        };
        if !state.detail_expanded.remove(&(lba, row.field_index)) {
            state.detail_expanded.insert((lba, row.field_index));
        }
        let count = self.advanced_inspect_detail_rows().len();
        if let Some(state) = self.advanced_inspect.as_mut() {
            let viewport = state
                .pane_focus
                .viewport_mut(crate::tui::pane::PaneId::InspectDetail);
            viewport.selected = Some(viewport.selected.unwrap_or(0).min(count.saturating_sub(1)));
        }
    }

    pub fn advanced_inspect_detail_yank(&mut self, raw: bool) -> Option<String> {
        let row = self.advanced_inspect_detail_selected_row()?;
        if raw && row.range.is_none() {
            return None;
        }
        let value = row.cells[if raw { 5 } else { 4 }].clone();
        self.advanced_inspect.as_mut()?.yank_register = Some(value.clone());
        Some(value)
    }

    pub fn advanced_inspect_focused_content_len(&self) -> usize {
        use crate::application::inspect_tree::InspectNodeKind;
        use crate::tui::pane::PaneId;

        let Some(state) = self
            .advanced_inspect
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
        else {
            return 0;
        };
        match state.pane_focus.focused() {
            PaneId::InspectTree => self.advanced_inspect_tree_rows().len(),
            PaneId::InspectDiskLayout => state
                .result
                .as_ref()
                .map(|workspace| {
                    crate::tui::disk_layout::DiskLayoutModel::from_topology(&workspace.topology)
                        .pane_line_count("summary", &[])
                })
                .unwrap_or(0),
            PaneId::InspectOverview => {
                let rows = self.advanced_inspect_tree_rows();
                let Some(row) = rows.get(state.tree_selected) else {
                    return 2;
                };
                8 + usize::from(row.range.byte_range.is_some()) + usize::from(row.decoder.is_some())
            }
            PaneId::InspectDetail => {
                let detail_count = self.advanced_inspect_detail_rows().len();
                if detail_count > 0 {
                    return detail_count;
                }
                let rows = self.advanced_inspect_tree_rows();
                let Some(row) = rows.get(state.tree_selected) else {
                    return 1;
                };
                let mut count = match row.kind {
                    InspectNodeKind::Sector => state
                        .result
                        .as_ref()
                        .and_then(|workspace| {
                            workspace
                                .items
                                .iter()
                                .find(|item| item.lba == row.range.start_lba)
                        })
                        .map(|item| {
                            let body = if !item.fields.is_empty() {
                                item.fields
                                    .iter()
                                    .map(|field| 1 + field.children.len())
                                    .sum::<usize>()
                            } else if let Some(meta_text) = &item.meta_text {
                                meta_text.lines().count()
                            } else {
                                1
                            };
                            body + item.notes.len() + 1
                        })
                        .unwrap_or(2),
                    InspectNodeKind::Field => {
                        if self.advanced_inspect_selected_field().is_some() {
                            4
                        } else {
                            1
                        }
                    }
                    InspectNodeKind::Group => 2,
                    _ => 2,
                };
                if state.prompt.is_some() {
                    count += 5;
                }
                if state.message.is_some() {
                    count += 2;
                }
                count.max(1)
            }
            _ => 0,
        }
    }

    pub fn advanced_inspect_focused_top(&mut self) {
        let Some(pane) = self.advanced_inspect_focused_pane() else {
            return;
        };
        if pane == crate::tui::pane::PaneId::InspectTree {
            self.advanced_inspect_tree_top();
        } else if pane == crate::tui::pane::PaneId::InspectDetail
            && !self.advanced_inspect_detail_rows().is_empty()
        {
            let viewport = self.pane_viewport_mut(pane);
            viewport.selected = Some(0);
            viewport.scroll_y.top();
        } else if let Some(state) = self.advanced_inspect.as_mut() {
            state.pane_focus.viewport_mut(pane).scroll_y.top();
        }
    }

    pub fn advanced_inspect_focused_bottom(&mut self) {
        let Some(pane) = self.advanced_inspect_focused_pane() else {
            return;
        };
        if pane == crate::tui::pane::PaneId::InspectTree {
            self.advanced_inspect_tree_bottom();
            return;
        }
        let content_len = self.advanced_inspect_focused_content_len();
        if pane == crate::tui::pane::PaneId::InspectDetail
            && !self.advanced_inspect_detail_rows().is_empty()
        {
            let viewport = self.pane_viewport_mut(pane);
            viewport.selected = Some(content_len.saturating_sub(1));
            viewport.scroll_y.bottom(content_len, 1);
            return;
        }
        if let Some(state) = self.advanced_inspect.as_mut() {
            state
                .pane_focus
                .viewport_mut(pane)
                .scroll_y
                .bottom(content_len, 1);
        }
    }

    pub fn advanced_inspect_move_focused_vertical(
        &mut self,
        delta: isize,
        visible_len: usize,
        content_len: usize,
    ) {
        let Some(pane) = self.advanced_inspect_focused_pane() else {
            return;
        };
        if pane == crate::tui::pane::PaneId::InspectTree {
            self.advanced_inspect_move_tree(delta);
            return;
        }
        if pane == crate::tui::pane::PaneId::InspectDetail {
            let rows = self.advanced_inspect_detail_rows();
            if !rows.is_empty() {
                let viewport = self.pane_viewport_mut(pane);
                let current = viewport.selected.unwrap_or(0).min(rows.len() - 1);
                let next = if delta < 0 {
                    current.saturating_sub(delta.unsigned_abs())
                } else {
                    current.saturating_add(delta as usize).min(rows.len() - 1)
                };
                viewport.selected = Some(next);
                if next < viewport.scroll_y.offset {
                    viewport.scroll_y.offset = next;
                } else if next >= viewport.scroll_y.offset.saturating_add(visible_len.max(1)) {
                    viewport.scroll_y.offset = next + 1 - visible_len.max(1);
                }
                return;
            }
        }
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.pane_focus.viewport_mut(pane).scroll_y.move_lines(
                delta,
                content_len,
                visible_len,
            );
        }
    }

    pub fn begin_advanced_inspect(&mut self, source: AdvancedInspectSource) -> bool {
        if self.critical_operation {
            self.set_notice("关键操作仍在执行，完成前不能启动全盘检查。");
            return false;
        }
        self.push_navigation_frame(NavigationLocation::from_workspace(self.workspace));
        let mut expanded = std::collections::BTreeSet::new();
        expanded.insert("device".to_string());
        self.advanced_inspect = Some(AdvancedInspectState {
            source,
            stage: AdvancedInspectStage::Running,
            result: None,
            tree_selected: 0,
            panel: AdvancedInspectPanel::DiskLayout,
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
        self.input_mode = InputMode::Normal;
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
            .advanced_inspect
            .as_ref()
            .ok_or_else(|| "全盘检查未打开".to_string())?;
        let device_id_override = match &state.source {
            AdvancedInspectSource::Disk(disk) => self
                .devices
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
        let Some(state) = self.advanced_inspect.as_mut() else {
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

        let Some(state) = self.advanced_inspect.as_ref() else {
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
        self.advanced_inspect
            .as_ref()?
            .tree_view_model
            .borrow()
            .as_ref()?
            .index
            .get(id)
            .copied()
    }

    pub fn advanced_inspect_prompt(&self) -> Option<&AdvancedInspectPrompt> {
        self.advanced_inspect.as_ref()?.prompt.as_ref()
    }

    pub fn advanced_inspect_begin_jump(&mut self) {
        if let Some(state) = self
            .advanced_inspect
            .as_mut()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
        {
            state.sector = None;
            state.panel = AdvancedInspectPanel::Tree;
            state
                .pane_focus
                .focus(crate::tui::pane::PaneId::InspectTree);
            state.prompt = Some(AdvancedInspectPrompt::Jump {
                unit: AdvancedInspectJumpUnit::Lba,
                input: String::new(),
            });
            state.message = None;
        }
    }

    pub fn advanced_inspect_begin_search(&mut self) {
        if let Some(state) = self
            .advanced_inspect
            .as_mut()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
        {
            state.sector = None;
            state.panel = AdvancedInspectPanel::Tree;
            state
                .pane_focus
                .focus(crate::tui::pane::PaneId::InspectTree);
            state.prompt = Some(AdvancedInspectPrompt::Search {
                input: String::new(),
            });
            state.message = None;
        }
    }

    pub fn advanced_inspect_cancel_prompt(&mut self) {
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.prompt = None;
        }
    }

    pub fn advanced_inspect_prompt_push(&mut self, ch: char) {
        let Some(prompt) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.prompt.as_mut())
        else {
            return;
        };
        match prompt {
            AdvancedInspectPrompt::Jump { input, .. } | AdvancedInspectPrompt::Search { input } => {
                input.push(ch);
            }
        }
    }

    pub fn advanced_inspect_prompt_backspace(&mut self) {
        let Some(prompt) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.prompt.as_mut())
        else {
            return;
        };
        match prompt {
            AdvancedInspectPrompt::Jump { input, .. } | AdvancedInspectPrompt::Search { input } => {
                input.pop();
            }
        }
    }

    pub fn advanced_inspect_toggle_jump_unit(&mut self) {
        let Some(AdvancedInspectPrompt::Jump { unit, .. }) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.prompt.as_mut())
        else {
            return;
        };
        *unit = match *unit {
            AdvancedInspectJumpUnit::Lba => AdvancedInspectJumpUnit::ByteOffset,
            AdvancedInspectJumpUnit::ByteOffset => AdvancedInspectJumpUnit::Lba,
        };
    }

    pub fn advanced_inspect_submit_prompt(
        &mut self,
    ) -> Result<Option<(AdvancedInspectSource, u64)>, String> {
        let prompt = self
            .advanced_inspect
            .as_ref()
            .and_then(|state| state.prompt.clone())
            .ok_or_else(|| "Inspect 输入面板未打开".to_string())?;

        let request = match prompt {
            AdvancedInspectPrompt::Jump { unit, input } => {
                let value = parse_inspect_jump_number(&input)?;
                match unit {
                    AdvancedInspectJumpUnit::Lba => {
                        self.advanced_inspect_jump_lba(value)?;
                        None
                    }
                    AdvancedInspectJumpUnit::ByteOffset => {
                        self.advanced_inspect_jump_byte_offset(value)?
                    }
                }
            }
            AdvancedInspectPrompt::Search { input } => {
                self.advanced_inspect_search(&input)?;
                None
            }
        };

        if let Some(state) = self.advanced_inspect.as_mut() {
            state.prompt = None;
        }
        Ok(request)
    }

    pub fn advanced_inspect_jump_lba(&mut self, lba: u64) -> Result<(), String> {
        const SECTOR_PAGE: u64 = 64;

        let location = {
            let state = self
                .advanced_inspect
                .as_ref()
                .filter(|state| state.stage == AdvancedInspectStage::Browser)
                .ok_or_else(|| "全盘检查未处于 Browser 状态".to_string())?;
            let workspace = state
                .result
                .as_ref()
                .ok_or_else(|| "Inspect workspace 不可用".to_string())?;
            if !workspace.topology.root.range.contains_lba(lba) {
                return Err(format!("LBA{lba} 超出磁盘范围"));
            }
            workspace
                .topology
                .lazy_sector_location(lba)
                .ok_or_else(|| format!("LBA{lba} 没有可定位的 lazy extent"))?
        };

        let relative = lba
            .checked_sub(location.start_lba)
            .ok_or_else(|| format!("LBA{lba} 位于 extent 起点之前"))?;
        if relative >= location.sector_count {
            return Err(format!("LBA{lba} 超出目标 extent"));
        }
        let row_path = inspect_row_path(&location.node_path);
        let extent_id = row_path
            .last()
            .cloned()
            .ok_or_else(|| format!("LBA{lba} 的 extent 路径为空"))?;
        let page_offset = relative / SECTOR_PAGE * SECTOR_PAGE;
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.expanded.extend(row_path);
            state.lazy_offsets.insert(extent_id.clone(), page_offset);
            state.tree_revision = state.tree_revision.wrapping_add(1);
            state.sector = None;
            state.panel = AdvancedInspectPanel::Tree;
            state
                .pane_focus
                .focus(crate::tui::pane::PaneId::InspectTree);
            state.message = None;
        }

        let target_id = format!("{extent_id}/sector.{lba}");
        let target = self
            .advanced_inspect_tree_index(&target_id)
            .ok_or_else(|| format!("LBA{lba} 已翻页但目标 Sector 未 materialize"))?;
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.tree_selected = target;
            reset_inspect_selected_context(state);
        }
        Ok(())
    }

    pub fn advanced_inspect_jump_byte_offset(
        &mut self,
        offset: u64,
    ) -> Result<Option<(AdvancedInspectSource, u64)>, String> {
        let total_sectors = self
            .advanced_inspect
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
            .and_then(|state| state.result.as_ref())
            .map(|workspace| workspace.topology.root.range.sector_count)
            .ok_or_else(|| "Inspect workspace 不可用".to_string())?;
        let total_bytes = total_sectors
            .checked_mul(crate::common::SECTOR as u64)
            .ok_or_else(|| "磁盘总字节数溢出 u64".to_string())?;
        if offset >= total_bytes {
            return Err(format!(
                "byte offset 0x{offset:X} 超出磁盘范围 0x0..0x{total_bytes:X}"
            ));
        }

        let lba = offset / crate::common::SECTOR as u64;
        let cursor = (offset % crate::common::SECTOR as u64) as usize;
        self.advanced_inspect_jump_lba(lba)?;
        self.advanced_inspect_open_sector_at(lba, cursor)
    }

    fn advanced_inspect_open_sector_at(
        &mut self,
        lba: u64,
        cursor: usize,
    ) -> Result<Option<(AdvancedInspectSource, u64)>, String> {
        if cursor >= crate::common::SECTOR {
            return Err(format!("sector-relative byte {cursor} 越界"));
        }
        let state = self
            .advanced_inspect
            .as_mut()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
            .ok_or_else(|| "全盘检查未处于 Browser 状态".to_string())?;
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
            field_expanded: false,
            pinned_field: None,
        });
        state.panel = AdvancedInspectPanel::Detail;
        state
            .pane_focus
            .focus(crate::tui::pane::PaneId::InspectDetail);
        Ok((!ready).then(|| (state.source.clone(), lba)))
    }

    pub fn advanced_inspect_search(&mut self, query: &str) -> Result<(), String> {
        let query = query.trim();
        if query.is_empty() {
            return Err("结构化搜索不能为空".into());
        }

        let matches = {
            let state = self
                .advanced_inspect
                .as_ref()
                .filter(|state| state.stage == AdvancedInspectStage::Browser)
                .ok_or_else(|| "全盘检查未处于 Browser 状态".to_string())?;
            let workspace = state
                .result
                .as_ref()
                .ok_or_else(|| "Inspect workspace 不可用".to_string())?;
            let mut matches = workspace
                .topology
                .find_label_paths(query)
                .into_iter()
                .map(AdvancedInspectSearchTarget::Topology)
                .collect::<Vec<_>>();
            for item in &workspace.items {
                let region = workspace.topology.primary_region_for_lba(item.lba);
                for relative_path in crate::application::inspect_tree::find_sector_structured_paths(
                    item.lba,
                    region.and_then(|value| value.decoder),
                    region
                        .map(|value| value.status)
                        .unwrap_or(crate::edpb::SemanticStatus::Unknown),
                    &item.fields,
                    query,
                ) {
                    matches.push(AdvancedInspectSearchTarget::CachedSector {
                        lba: item.lba,
                        relative_path,
                    });
                }
            }
            matches
        };
        if matches.is_empty() {
            return Err(format!("未找到结构化匹配: {query}"));
        }
        let first = matches[0].clone();
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.search_query = query.to_string();
            state.search_matches = matches;
            state.search_cursor = 0;
        }
        self.advanced_inspect_focus_search_target(first)
    }

    pub fn advanced_inspect_search_next(&mut self, reverse: bool) -> Result<(), String> {
        let target = {
            let state = self
                .advanced_inspect
                .as_mut()
                .filter(|state| state.stage == AdvancedInspectStage::Browser)
                .ok_or_else(|| "全盘检查未处于 Browser 状态".to_string())?;
            if state.search_matches.is_empty() {
                return Err("请先使用 / 执行结构化搜索".into());
            }
            let len = state.search_matches.len();
            state.search_cursor = if reverse {
                (state.search_cursor + len - 1) % len
            } else {
                (state.search_cursor + 1) % len
            };
            state.search_matches[state.search_cursor].clone()
        };
        self.advanced_inspect_focus_search_target(target)
    }

    pub fn advanced_inspect_search_status(&self) -> Option<(&str, usize, usize)> {
        let state = self.advanced_inspect.as_ref()?;
        if state.search_matches.is_empty() {
            return None;
        }
        Some((
            state.search_query.as_str(),
            state.search_cursor + 1,
            state.search_matches.len(),
        ))
    }

    fn advanced_inspect_focus_search_target(
        &mut self,
        target: AdvancedInspectSearchTarget,
    ) -> Result<(), String> {
        match target {
            AdvancedInspectSearchTarget::Topology(node_path) => {
                let row_path = inspect_row_path(&node_path);
                let target_id = row_path.last().cloned().unwrap_or_default();
                if let Some(state) = self.advanced_inspect.as_mut() {
                    state.expanded.extend(
                        row_path
                            .iter()
                            .take(node_path.len().saturating_sub(1))
                            .cloned(),
                    );
                    state.tree_revision = state.tree_revision.wrapping_add(1);
                    state.sector = None;
                    state.panel = AdvancedInspectPanel::Tree;
                    state
                        .pane_focus
                        .focus(crate::tui::pane::PaneId::InspectTree);
                    state.message = None;
                }
                let selected = self
                    .advanced_inspect_tree_index(&target_id)
                    .ok_or_else(|| "搜索命中节点未能在 Tree 中定位".to_string())?;
                if let Some(state) = self.advanced_inspect.as_mut() {
                    state.tree_selected = selected;
                    reset_inspect_selected_context(state);
                }
                Ok(())
            }
            AdvancedInspectSearchTarget::CachedSector { lba, relative_path } => {
                self.advanced_inspect_jump_lba(lba)?;
                let rows = self.advanced_inspect_tree_rows();
                let base_id = rows
                    .get(
                        self.advanced_inspect
                            .as_ref()
                            .map(|state| state.tree_selected)
                            .unwrap_or(0),
                    )
                    .filter(|row| {
                        row.kind == crate::application::inspect_tree::InspectNodeKind::Sector
                    })
                    .map(|row| row.id.clone())
                    .ok_or_else(|| format!("LBA{lba} Sector 定位失败"))?;

                let mut target_id = base_id.clone();
                if let Some(state) = self.advanced_inspect.as_mut() {
                    if relative_path.len() > 1 {
                        state.expanded.insert(base_id.clone());
                    }
                    for (index, node_id) in relative_path.iter().skip(1).enumerate() {
                        target_id = format!("{target_id}/{node_id}");
                        if index + 2 < relative_path.len() {
                            state.expanded.insert(target_id.clone());
                        }
                    }
                    state.tree_revision = state.tree_revision.wrapping_add(1);
                }
                let selected = self
                    .advanced_inspect_tree_index(&target_id)
                    .ok_or_else(|| "搜索命中结构未能自动展开到目标节点".to_string())?;
                if let Some(state) = self.advanced_inspect.as_mut() {
                    state.tree_selected = selected;
                    reset_inspect_selected_context(state);
                    state.panel = AdvancedInspectPanel::Tree;
                    state
                        .pane_focus
                        .focus(crate::tui::pane::PaneId::InspectTree);
                    state.message = None;
                }
                Ok(())
            }
        }
    }

    pub fn advanced_inspect_move_tree(&mut self, delta: isize) {
        let count = self.advanced_inspect_tree_rows().len();
        let Some(state) = self.advanced_inspect.as_mut() else {
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
            .advanced_inspect
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
            .advanced_inspect
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
            .advanced_inspect
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
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.tree_selected = parent_index;
            reset_inspect_selected_context(state);
            state.sector = None;
        }
    }

    pub fn advanced_inspect_expand_or_child(&mut self) {
        let rows = self.advanced_inspect_tree_rows();
        let Some(selected) = self
            .advanced_inspect
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
            if let Some(state) = self.advanced_inspect.as_mut() {
                state.tree_selected = child_index;
                reset_inspect_selected_context(state);
                state.sector = None;
            }
        }
    }

    pub fn advanced_inspect_toggle_selected(&mut self) {
        let rows = self.advanced_inspect_tree_rows();
        let selected = self
            .advanced_inspect
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
                if let Some(state) = self.advanced_inspect.as_mut() {
                    state.lazy_offsets.insert(extent_id, offset);
                    state.tree_revision = state.tree_revision.wrapping_add(1);
                }
                if let Some(target_index) = self.advanced_inspect_tree_index(&target_id) {
                    if let Some(state) = self.advanced_inspect.as_mut() {
                        state.tree_selected = target_index;
                        reset_inspect_selected_context(state);
                    }
                }
            }
            AdvancedInspectTreeAction::None => {
                if !row.expandable {
                    return;
                }
                if let Some(state) = self.advanced_inspect.as_mut() {
                    if !state.expanded.remove(&row.id) {
                        state.expanded.insert(row.id.clone());
                    }
                    state.tree_revision = state.tree_revision.wrapping_add(1);
                }
                let count = self.advanced_inspect_tree_rows().len();
                if let Some(state) = self.advanced_inspect.as_mut() {
                    state.tree_selected = state.tree_selected.min(count.saturating_sub(1));
                    reset_inspect_selected_context(state);
                }
            }
        }
    }

    pub fn advanced_inspect_shift_panel(&mut self, reverse: bool) {
        if let Some(state) = self
            .advanced_inspect
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
            .advanced_inspect
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
            .map(|state| state.tree_selected);
        let Some(row) = selected.and_then(|index| rows.get(index)) else {
            return;
        };
        if row.expandable || row.action != AdvancedInspectTreeAction::None {
            self.advanced_inspect_toggle_selected();
        } else if let Some(state) = self.advanced_inspect.as_mut() {
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
            .advanced_inspect
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
            .advanced_inspect
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
            let state = self.advanced_inspect.as_ref()?;
            (state.panel, state.tree_selected, state.pane_focus.clone())
        };
        self.navigation.push(NavigationFrame {
            location: NavigationLocation::Inspect,
            selection: self.selected,
            item_count: self.item_count,
            panel: Some(panel),
            tree_selection,
            pane_focus: Some(pane_focus),
            table_scroll: None,
        });
        let state = self.advanced_inspect.as_mut()?;
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
            .advanced_inspect
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)?;
        let rows = self.advanced_inspect_tree_rows();
        let row = rows.get(state.tree_selected)?;
        (row.kind == crate::application::inspect_tree::InspectNodeKind::Sector)
            .then_some(row.range.start_lba)
    }

    pub fn advanced_inspect_preview_request(&self) -> Option<(AdvancedInspectSource, u64)> {
        let advanced = self.advanced_inspect.as_ref()?;
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
        self.advanced_inspect
            .as_ref()
            .and_then(|advanced| advanced.preview_load.get(&lba))
            .cloned()
            .unwrap_or_default()
    }

    pub fn advanced_inspect_mark_preview_pending(&mut self, lba: u64) {
        if let Some(advanced) = self.advanced_inspect.as_mut() {
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
        let source = self.advanced_inspect.as_ref()?.source.clone();
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
            let state = self.advanced_inspect.as_ref()?;
            (state.panel, state.tree_selected, state.pane_focus.clone())
        };
        self.navigation.push(NavigationFrame {
            location: NavigationLocation::Inspect,
            selection: self.selected,
            item_count: self.item_count,
            panel: Some(panel),
            tree_selection,
            pane_focus: Some(pane_focus),
            table_scroll: None,
        });
        let state = self.advanced_inspect.as_mut()?;
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

    pub fn advanced_inspect_sector(&self) -> Option<&SectorInspectorState> {
        self.advanced_inspect.as_ref()?.sector.as_ref()
    }

    pub fn advanced_inspect_decode_request(&self) -> Option<(AdvancedInspectSource, u64)> {
        let advanced = self.advanced_inspect.as_ref()?;
        let sector = advanced.sector.as_ref()?;
        if sector.pending || sector.error.is_some() {
            return None;
        }
        let ready = advanced.result.as_ref()?.items.iter().any(|item| {
            item.lba == sector.lba && (item.decoded.is_some() || item.decode_error.is_some())
        });
        (!ready).then(|| (advanced.source.clone(), sector.lba))
    }

    pub fn advanced_inspect_mark_decode_pending(&mut self, lba: u64, pending: bool) {
        if let Some(sector) = self
            .advanced_inspect
            .as_mut()
            .and_then(|advanced| advanced.sector.as_mut())
            .filter(|sector| sector.lba == lba)
        {
            sector.pending = pending;
            if pending {
                sector.error = None;
            }
        }
    }

    pub fn advanced_inspect_sector_item(
        &self,
    ) -> Option<&crate::application::inspect::AdvancedInspectItem> {
        let state = self.advanced_inspect.as_ref()?;
        let sector = state.sector.as_ref()?;
        state
            .result
            .as_ref()?
            .items
            .iter()
            .find(|item| item.lba == sector.lba)
    }

    pub fn advanced_inspect_sector_finish(
        &mut self,
        lba: u64,
        result: Result<crate::application::inspect::AdvancedInspectItem, String>,
    ) {
        let Some(state) = self.advanced_inspect.as_mut() else {
            return;
        };
        match result {
            Ok(mut item) => {
                state.preview_load.insert(lba, PreviewLoadState::Ready);
                const ON_DEMAND_CACHE_LIMIT: usize = 5;
                if let Some(workspace) = state.result.as_mut() {
                    if let Some(index) = workspace.items.iter().position(|old| old.lba == lba) {
                        if item.meta_text.is_none() {
                            item.meta_text = workspace.items[index].meta_text.clone();
                        }
                        workspace.items[index] = item;
                    } else {
                        workspace.items.push(item);
                        workspace.items.sort_by_key(|item| item.lba);
                    }
                    if lba >= crate::common::METADATA_SECTOR_COUNT as u64 {
                        state.sector_cache_order.retain(|cached| *cached != lba);
                        state.sector_cache_order.push_back(lba);
                        while state.sector_cache_order.len() > ON_DEMAND_CACHE_LIMIT {
                            let Some(evicted) = state.sector_cache_order.pop_front() else {
                                break;
                            };
                            if state
                                .sector
                                .as_ref()
                                .is_some_and(|sector| sector.lba == evicted)
                            {
                                state.sector_cache_order.push_back(evicted);
                                continue;
                            }
                            workspace.items.retain(|value| value.lba != evicted);
                            state.preview_load.remove(&evicted);
                        }
                    }
                    state.tree_revision = state.tree_revision.wrapping_add(1);
                }
                if let Some(sector) = state.sector.as_mut().filter(|sector| sector.lba == lba) {
                    sector.pending = false;
                    sector.error = state
                        .result
                        .as_ref()
                        .and_then(|workspace| workspace.items.iter().find(|item| item.lba == lba))
                        .and_then(|item| item.decode_error.clone());
                }
            }
            Err(message) => {
                let attempts = match state.preview_load.get(&lba) {
                    Some(
                        PreviewLoadState::Pending { attempts }
                        | PreviewLoadState::Failed { attempts, .. },
                    ) => *attempts,
                    _ => 1,
                };
                state.preview_load.insert(
                    lba,
                    PreviewLoadState::Failed {
                        message: message.clone(),
                        attempts,
                    },
                );
                if let Some(sector) = state.sector.as_mut().filter(|sector| sector.lba == lba) {
                    sector.pending = false;
                    sector.error = Some(message);
                }
            }
        }
    }

    pub fn advanced_inspect_sector_set_cursor(&mut self, cursor: usize) {
        let Some(sector) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.sector.as_mut())
        else {
            return;
        };
        sector.cursor = cursor.min(crate::common::SECTOR - 1);
        sector.pinned_field = None;
        sector.field_expanded = false;
    }

    pub fn advanced_inspect_sector_row_start(&mut self) {
        if let Some(cursor) = self.advanced_inspect_sector().map(|sector| sector.cursor) {
            self.advanced_inspect_sector_set_cursor((cursor / 16) * 16);
        }
    }

    pub fn advanced_inspect_sector_row_end(&mut self) {
        if let Some(cursor) = self.advanced_inspect_sector().map(|sector| sector.cursor) {
            self.advanced_inspect_sector_set_cursor(
                ((cursor / 16) * 16 + 15).min(crate::common::SECTOR - 1),
            );
        }
    }

    pub fn advanced_inspect_sector_top(&mut self) {
        self.advanced_inspect_sector_set_cursor(0);
    }

    pub fn advanced_inspect_sector_bottom(&mut self) {
        self.advanced_inspect_sector_set_cursor(crate::common::SECTOR - 1);
    }

    pub fn advanced_inspect_sector_half_page(&mut self, up: bool) {
        self.advanced_inspect_sector_move_cursor(if up { -128 } else { 128 });
    }

    pub fn advanced_inspect_sector_page(&mut self, up: bool) {
        self.advanced_inspect_sector_move_cursor(if up { -256 } else { 256 });
    }

    pub fn advanced_inspect_sector_move_cursor(&mut self, delta: isize) {
        let Some(sector) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.sector.as_mut())
        else {
            return;
        };
        sector.cursor = if delta < 0 {
            sector.cursor.saturating_sub(delta.unsigned_abs())
        } else {
            sector
                .cursor
                .saturating_add(delta as usize)
                .min(crate::common::SECTOR - 1)
        };
        sector.pinned_field = None;
        sector.field_expanded = false;
    }

    pub fn advanced_inspect_sector_active_field(
        &self,
    ) -> Option<crate::application::inspect::InspectField> {
        let state = self.advanced_inspect.as_ref()?;
        let sector = state.sector.as_ref()?;
        let absolute = sector
            .lba
            .checked_mul(crate::common::SECTOR as u64)?
            .checked_add(sector.cursor as u64)?;
        if let Some(field) = sector
            .pinned_field
            .as_ref()
            .filter(|field| absolute >= field.range.start && absolute < field.range.end_exclusive)
        {
            return Some(field.clone());
        }
        state
            .result
            .as_ref()?
            .items
            .iter()
            .find(|item| item.lba == sector.lba)?
            .fields
            .iter()
            .find(|field| absolute >= field.range.start && absolute < field.range.end_exclusive)
            .cloned()
    }

    pub fn advanced_inspect_sector_yank(&mut self, raw_range: bool) -> Option<String> {
        let field = self.advanced_inspect_sector_active_field();
        let byte = self.advanced_inspect_sector_item().and_then(|item| {
            let cursor = self.advanced_inspect_sector()?.cursor;
            item.raw.get(cursor).copied()
        });
        let value = match (raw_range, field) {
            (true, Some(field)) => field
                .raw
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
            (false, Some(field)) => format!("{} = {}", field.label, field.value),
            (_, None) => format!("0x{:02X}", byte?),
        };
        if let Some(state) = self.advanced_inspect.as_mut() {
            state.yank_register = Some(value.clone());
        }
        Some(value)
    }

    pub fn advanced_inspect_yank_register(&self) -> Option<&str> {
        self.advanced_inspect.as_ref()?.yank_register.as_deref()
    }

    pub fn advanced_inspect_sector_set_mode(&mut self, mode: SectorInspectMode) {
        if let Some(sector) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.sector.as_mut())
        {
            sector.mode = mode;
        }
    }

    pub fn advanced_inspect_sector_cycle_mode(&mut self) {
        let Some(sector) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.sector.as_mut())
        else {
            return;
        };
        sector.mode = match sector.mode {
            SectorInspectMode::Raw => SectorInspectMode::Decode,
            SectorInspectMode::Decode => SectorInspectMode::Mixed,
            SectorInspectMode::Mixed => SectorInspectMode::Raw,
        };
    }

    pub fn advanced_inspect_sector_toggle_field(&mut self) {
        if let Some(sector) = self
            .advanced_inspect
            .as_mut()
            .and_then(|state| state.sector.as_mut())
        {
            sector.field_expanded = !sector.field_expanded;
        }
    }

    pub fn advanced_inspect_shift_sector(
        &mut self,
        delta: i64,
    ) -> Option<(AdvancedInspectSource, u64)> {
        let state = self.advanced_inspect.as_mut()?;
        let sector = state.sector.as_mut()?;
        let total = state.result.as_ref()?.topology.root.range.sector_count;
        if total == 0 {
            return None;
        }
        let next = if delta < 0 {
            sector.lba.saturating_sub(delta.unsigned_abs())
        } else {
            sector.lba.saturating_add(delta as u64).min(total - 1)
        };
        if next == sector.lba {
            return None;
        }
        sector.lba = next;
        sector.error = None;
        sector.field_expanded = false;
        if let Some(field) = sector.pinned_field.as_ref() {
            let sector_start = next.saturating_mul(crate::common::SECTOR as u64);
            let sector_end = sector_start.saturating_add(crate::common::SECTOR as u64);
            if field.range.start < sector_end && field.range.end_exclusive > sector_start {
                sector.cursor = field
                    .range
                    .start
                    .max(sector_start)
                    .saturating_sub(sector_start) as usize;
            } else {
                sector.pinned_field = None;
            }
        }
        let ready = state.result.as_ref().is_some_and(|workspace| {
            workspace.items.iter().any(|item| {
                item.lba == next && (item.decoded.is_some() || item.decode_error.is_some())
            })
        });
        sector.pending = !ready;
        (!ready).then(|| (state.source.clone(), next))
    }

    pub fn advanced_inspect_close_sector(&mut self) -> bool {
        let Some(state) = self.advanced_inspect.as_mut() else {
            return false;
        };
        if state.sector.take().is_some() {
            if let Some(frame) = self.navigation.pop() {
                state.panel = frame.panel.unwrap_or(AdvancedInspectPanel::Tree);
                state.tree_selected = frame.tree_selection;
                if let Some(pane_focus) = frame.pane_focus {
                    state.pane_focus = pane_focus;
                    if let Some(panel) =
                        AdvancedInspectPanel::from_pane_id(state.pane_focus.focused())
                    {
                        state.panel = panel;
                    }
                }
            }
            true
        } else {
            false
        }
    }

    pub fn close_advanced_inspect(&mut self) {
        if self
            .advanced_inspect
            .as_ref()
            .is_some_and(|state| state.stage != AdvancedInspectStage::Running)
        {
            self.advanced_inspect = None;
            if let Some(frame) = self.navigation.pop() {
                self.selected = frame.selection.min(self.item_count.saturating_sub(1));
            }
        }
    }
}
