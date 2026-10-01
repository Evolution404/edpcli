use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvancedInspectTreeAction {
    None,
    SetLazyOffset {
        lazy_node_id: String,
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
pub(super) struct InspectTreeViewModel {
    revision: u64,
    rows: std::sync::Arc<Vec<AdvancedInspectTreeRow>>,
    index: std::collections::HashMap<String, usize>,
}

pub(super) fn reset_inspect_selected_context(state: &mut AdvancedInspectState) {
    use crate::tui::pane::PaneId;
    for pane in [PaneId::InspectOverview, PaneId::InspectDetail] {
        let viewport = state.pane_focus.viewport_mut(pane);
        viewport.scroll_y.top();
        if pane == PaneId::InspectDetail {
            viewport.selected = Some(0);
        }
    }
}

pub(super) fn inspect_row_path(node_path: &[String]) -> Vec<String> {
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

impl AppState {
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
            lazy_node_id: String,
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
                    lazy_node_id: spec.lazy_node_id,
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
                            lazy_node_id: row_id.clone(),
                            offset: previous_offset,
                            target_id: format!("{row_id}/sector.{previous_lba}"),
                        }));
                    }

                    let children = node.materialize_sector_page(offset, SECTOR_PAGE);
                    let materialized_count = children.len() as u64;
                    for child in children {
                        let child = if child.kind == InspectNodeKind::Sector {
                            if let Some(item) = workspace
                                .items
                                .iter()
                                .find(|item| item.lba == child.range.start_lba)
                            {
                                crate::application::inspect_tree::enrich_sector_node(
                                    child,
                                    &item.fields,
                                )
                            } else {
                                child
                            }
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
                            lazy_node_id: row_id.clone(),
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

    pub fn advanced_inspect_scroll_tree_horizontal(&mut self, reverse: bool) {
        let viewport = self.pane_viewport_mut(crate::tui::pane::PaneId::InspectTree);
        viewport.scroll_x = if reverse {
            viewport.scroll_x.saturating_sub(2)
        } else {
            viewport.scroll_x.saturating_add(2)
        };
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
                lazy_node_id,
                offset,
                target_id,
            } => {
                if let Some(state) = self.inspect.advanced.as_mut() {
                    state.lazy_offsets.insert(lazy_node_id, offset);
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
}
