use super::AdvancedInspectStage;
use crate::tui::state::AppState;

pub const INSPECT_DETAIL_HEADINGS: [&str; 11] = [
    "Status",
    "Offset",
    "Len",
    "Field",
    "Value",
    "Group",
    "Raw",
    "Decoded",
    "Logical",
    "Type",
    "Transform",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectDetailRow {
    pub cells: [String; 11],
    pub range: Option<crate::application::inspect::AbsoluteByteRange>,
    pub field_index: usize,
    pub child_index: Option<usize>,
}

fn inspect_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

impl AppState {
    pub fn advanced_inspect_detail_rows(&self) -> Vec<InspectDetailRow> {
        use crate::application::inspect::AbsoluteByteRange;
        use crate::application::inspect_tree::InspectNodeKind;

        let Some(state) = self.inspect.advanced.as_ref() else {
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
                    format!("{:?}", field.status),
                    format!("0x{offset:03X}"),
                    field.range.len().to_string(),
                    if field.key == crate::inspect::InspectFieldKey::Lba8Elabel {
                        format!("{marker}E_LABEL [{}]", field.children.len())
                    } else {
                        format!("{marker}{}", field.label)
                    },
                    field.value.clone(),
                    field.group.clone().unwrap_or_default(),
                    inspect_hex(&field.raw),
                    inspect_hex(&field.decoded),
                    field
                        .field_logical
                        .as_deref()
                        .map(inspect_hex)
                        .unwrap_or_default(),
                    format!("{:?}", field.field_type),
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
                            format!("{:?}", field.status),
                            offset,
                            len,
                            format!("  {}", child.label),
                            child.value.clone(),
                            field.group.clone().unwrap_or_default(),
                            raw,
                            decoded,
                            String::new(),
                            String::new(),
                            String::new(),
                        ],
                        range,
                        field_index,
                        child_index: Some(child_index),
                    });
                }
            }
        }
        if let Some(sort) = self.table_sort(crate::tui::table_layout::TableKind::InspectFields) {
            projected.sort_by(|left, right| {
                let a = left
                    .cells
                    .get(sort.column)
                    .map(String::as_str)
                    .unwrap_or("");
                let b = right
                    .cells
                    .get(sort.column)
                    .map(String::as_str)
                    .unwrap_or("");
                let ordering = crate::tui::table_layout::smart_cell_cmp(a, b)
                    .then_with(|| left.field_index.cmp(&right.field_index))
                    .then_with(|| left.child_index.cmp(&right.child_index));
                match sort.direction {
                    crate::tui::table_layout::SortDirection::Ascending => ordering,
                    crate::tui::table_layout::SortDirection::Descending => ordering.reverse(),
                }
            });
        }
        projected
    }

    pub fn advanced_inspect_detail_selected_row(&self) -> Option<InspectDetailRow> {
        let index = self
            .inspect
            .advanced
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
        let Some(state) = self.inspect.advanced.as_mut() else {
            return;
        };
        if !state.detail_expanded.remove(&(lba, row.field_index)) {
            state.detail_expanded.insert((lba, row.field_index));
        }
        let count = self.advanced_inspect_detail_rows().len();
        if let Some(state) = self.inspect.advanced.as_mut() {
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
        self.inspect.advanced.as_mut()?.yank_register = Some(value.clone());
        Some(value)
    }

    pub fn advanced_inspect_focused_content_len(&self) -> usize {
        use crate::application::inspect_tree::InspectNodeKind;
        use crate::tui::pane::PaneId;

        let Some(state) = self
            .inspect
            .advanced
            .as_ref()
            .filter(|state| state.stage == AdvancedInspectStage::Browser)
        else {
            return 0;
        };
        match state.pane_focus.focused() {
            PaneId::InspectTree => self.advanced_inspect_tree_rows().len(),
            PaneId::InspectOverview => {
                let rows = self.advanced_inspect_tree_rows();
                let Some(row) = rows.get(state.tree_selected) else {
                    return 2;
                };
                let manifest_lines = state
                    .result
                    .as_ref()
                    .and_then(|workspace| workspace.backup_manifest.as_ref())
                    .map_or(0, |manifest| {
                        4 + usize::from(manifest.restore_contract.is_some())
                    });
                8 + usize::from(row.range.byte_range.is_some())
                    + usize::from(row.decoder.is_some())
                    + manifest_lines
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
                let Some(workspace) = state.result.as_ref() else {
                    return 1;
                };

                // Keep scrolling aligned with the structured technical-evidence table.
                let mut count = 5usize; // object + two location rows + status + source
                count += usize::from(row.range.byte_range.is_some());
                count += usize::from(row.decoder.is_some());
                count += usize::from(row.region_semantic.is_some());

                let item = matches!(row.kind, InspectNodeKind::Sector | InspectNodeKind::Field)
                    .then(|| {
                        workspace
                            .items
                            .iter()
                            .find(|item| item.lba == row.range.start_lba)
                    })
                    .flatten();
                if let Some(item) = item {
                    if row.kind == InspectNodeKind::Field
                        && row.range.byte_range.is_some_and(|range| {
                            item.fields.iter().any(|field| field.range == range)
                        })
                    {
                        count += 10;
                    }
                    count += usize::from(!item.regions.is_empty());
                    count += 3; // RAW non-zero, RAW SHA-256, parse state
                    count += usize::from(item.decoded_sha256.is_some());
                    count += usize::from(item.method.is_some());
                    count += usize::from(item.decode_error.is_some());
                    count += item.diagnostics.len();
                    count += item
                        .meta_text
                        .as_deref()
                        .map(|text| {
                            text.lines()
                                .map(str::trim)
                                .filter(|line| {
                                    !line.is_empty()
                                        && !line.starts_with("Enter ")
                                        && !line.ends_with(':')
                                        && !line.ends_with('：')
                                })
                                .count()
                        })
                        .unwrap_or(0);
                    count += item.notes.len();
                }
                if let Some(manifest) = workspace.backup_manifest.as_ref() {
                    count += 2
                        + manifest.regions.len()
                        + manifest.extents.len()
                        + manifest.artifacts.len().saturating_mul(3);
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
        } else if let Some(state) = self.inspect.advanced.as_mut() {
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
        if let Some(state) = self.inspect.advanced.as_mut() {
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
        if let Some(state) = self.inspect.advanced.as_mut() {
            state.pane_focus.viewport_mut(pane).scroll_y.move_lines(
                delta,
                content_len,
                visible_len,
            );
        }
    }
}
