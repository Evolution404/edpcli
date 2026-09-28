use super::*;

impl AppState {
    pub fn advanced_inspect_sector(&self) -> Option<&SectorInspectorState> {
        self.inspect.advanced.as_ref()?.sector.as_ref()
    }

    pub fn advanced_inspect_decode_request(&self) -> Option<(AdvancedInspectSource, u64)> {
        let advanced = self.inspect.advanced.as_ref()?;
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
        let state = self.inspect.advanced.as_ref()?;
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
        let Some(state) = self.inspect.advanced.as_mut() else {
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
        if let Some(cursor) = self.inspect.advanced_sector().map(|sector| sector.cursor) {
            self.inspect.advanced_sector_set_cursor((cursor / 16) * 16);
        }
    }

    pub fn advanced_inspect_sector_row_end(&mut self) {
        if let Some(cursor) = self.inspect.advanced_sector().map(|sector| sector.cursor) {
            self.inspect.advanced_sector_set_cursor(
                ((cursor / 16) * 16 + 15).min(crate::common::SECTOR - 1),
            );
        }
    }

    pub fn advanced_inspect_sector_top(&mut self) {
        self.inspect.advanced_sector_set_cursor(0);
    }

    pub fn advanced_inspect_sector_bottom(&mut self) {
        self.inspect.advanced_sector_set_cursor(crate::common::SECTOR - 1);
    }

    pub fn advanced_inspect_sector_half_page(&mut self, up: bool) {
        self.inspect.advanced_sector_move_cursor(if up { -128 } else { 128 });
    }

    pub fn advanced_inspect_sector_page(&mut self, up: bool) {
        self.inspect.advanced_sector_move_cursor(if up { -256 } else { 256 });
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
        let state = self.inspect.advanced.as_ref()?;
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
        let field = self.inspect.advanced_sector_active_field();
        let byte = self.inspect.advanced_sector_item().and_then(|item| {
            let cursor = self.inspect.advanced_sector()?.cursor;
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
        if let Some(state) = self.inspect.advanced.as_mut() {
            state.yank_register = Some(value.clone());
        }
        Some(value)
    }

    pub fn advanced_inspect_yank_register(&self) -> Option<&str> {
        self.inspect.advanced.as_ref()?.yank_register.as_deref()
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
        let state = self.inspect.advanced.as_mut()?;
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
        let Some(state) = self.inspect.advanced.as_mut() else {
            return false;
        };
        if state.sector.take().is_some() {
            if let Some(frame) = self.shell.navigation.pop() {
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
}
