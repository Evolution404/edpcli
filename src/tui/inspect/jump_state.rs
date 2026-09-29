use super::tree_state::reset_inspect_selected_context;
use super::*;

impl AppState {
    pub fn advanced_inspect_jump_lba(&mut self, lba: u64) -> Result<(), String> {
        const SECTOR_PAGE: u64 = 64;

        let location = {
            let state = self
                .inspect
                .advanced
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
        let row_path = tree_state::inspect_row_path(&location.node_path);
        let extent_id = row_path
            .last()
            .cloned()
            .ok_or_else(|| format!("LBA{lba} 的 extent 路径为空"))?;
        let page_offset = relative / SECTOR_PAGE * SECTOR_PAGE;
        if let Some(state) = self.inspect.advanced.as_mut() {
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
        if let Some(state) = self.inspect.advanced.as_mut() {
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
            .inspect
            .advanced
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
            .inspect
            .advanced
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
}
