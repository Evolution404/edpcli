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
                .ok_or_else(|| "检查工作区不可用".to_string())?;
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
            .ok_or_else(|| format!("LBA{lba} 位于 lazy 区域起点之前"))?;
        if relative >= location.sector_count {
            return Err(format!("LBA{lba} 超出目标 lazy 区域"));
        }
        let row_path = tree_state::inspect_row_path(&location.node_path);
        let lazy_node_id = row_path
            .last()
            .cloned()
            .ok_or_else(|| format!("LBA{lba} 的 lazy 区域路径为空"))?;
        let page_offset = relative / SECTOR_PAGE * SECTOR_PAGE;
        if let Some(state) = self.inspect.advanced.as_mut() {
            state.expanded.extend(row_path);
            state.lazy_offsets.insert(lazy_node_id.clone(), page_offset);
            state.tree_revision = state.tree_revision.wrapping_add(1);
        }

        let target_id = format!("{lazy_node_id}/sector.{lba}");
        let target = self
            .advanced_inspect_tree_index(&target_id)
            .ok_or_else(|| format!("LBA{lba} 已翻页但目标 Sector 未 materialize"))?;
        if let Some(state) = self.inspect.advanced.as_mut() {
            state.tree_selected = target;
            reset_inspect_selected_context(state);
        }
        Ok(())
    }

    pub fn advanced_inspect_prepare_jump_sector(
        &mut self,
        lba: u64,
        view_mode: InspectViewMode,
        mode: SectorInspectMode,
    ) -> Result<Option<(AdvancedInspectSource, u64)>, String> {
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
        state.sector = if view_mode == InspectViewMode::Hex {
            Some(SectorInspectorState {
                lba,
                mode,
                cursor: 0,
                pending: !ready,
                error: None,
                field_expanded: false,
                pinned_field: None,
            })
        } else {
            None
        };
        Ok((!ready).then(|| (state.source.clone(), lba)))
    }
}
