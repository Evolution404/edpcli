use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AdvancedInspectSearchTarget {
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

impl AppState {
    pub fn advanced_inspect_prompt(&self) -> Option<&AdvancedInspectPrompt> {
        self.inspect.advanced.as_ref()?.prompt.as_ref()
    }

    pub fn advanced_inspect_begin_jump(&mut self) {
        if let Some(state) = self
            .inspect
            .advanced
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
            .inspect
            .advanced
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
        if let Some(state) = self.inspect.advanced.as_mut() {
            state.prompt = None;
        }
    }

    pub fn advanced_inspect_prompt_push(&mut self, ch: char) {
        let Some(prompt) = self
            .inspect
            .advanced
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
            .inspect
            .advanced
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
            .inspect
            .advanced
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
            .inspect
            .advanced
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

        if let Some(state) = self.inspect.advanced.as_mut() {
            state.prompt = None;
        }
        Ok(request)
    }

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
        let row_path = inspect_row_path(&location.node_path);
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

    pub fn advanced_inspect_search(&mut self, query: &str) -> Result<(), String> {
        let query = query.trim();
        if query.is_empty() {
            return Err("结构化搜索不能为空".into());
        }

        let matches = {
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
        if let Some(state) = self.inspect.advanced.as_mut() {
            state.search_query = query.to_string();
            state.search_matches = matches;
            state.search_cursor = 0;
        }
        self.advanced_inspect_focus_search_target(first)
    }

    pub fn advanced_inspect_search_next(&mut self, reverse: bool) -> Result<(), String> {
        let target = {
            let state = self
                .inspect
                .advanced
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
        let state = self.inspect.advanced.as_ref()?;
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
                if let Some(state) = self.inspect.advanced.as_mut() {
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
                if let Some(state) = self.inspect.advanced.as_mut() {
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
                        self.inspect
                            .advanced
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
                if let Some(state) = self.inspect.advanced.as_mut() {
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
                if let Some(state) = self.inspect.advanced.as_mut() {
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
}
