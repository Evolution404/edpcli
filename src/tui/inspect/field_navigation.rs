use super::*;

impl AppState {
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
