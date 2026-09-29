use super::*;

impl AppState {
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
}
