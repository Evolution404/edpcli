use super::tree_state::reset_inspect_selected_context;
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
            state.prompt = Some(AdvancedInspectPrompt::Jump {
                input: String::new(),
                error: None,
                origin_view: state.view_mode,
                origin_panel: state.panel,
                origin_pane: state.pane_focus.focused(),
                origin_sector_mode: state.sector.as_ref().map(|sector| sector.mode),
            });
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
            state.view_mode = InspectViewMode::Browser;
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
            AdvancedInspectPrompt::Jump { input, error, .. } => {
                input.push(ch);
                *error = None;
            }
            AdvancedInspectPrompt::Search { input } => {
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
            AdvancedInspectPrompt::Jump { input, error, .. } => {
                input.pop();
                *error = None;
            }
            AdvancedInspectPrompt::Search { input } => {
                input.pop();
            }
        }
    }

    fn advanced_inspect_set_jump_error(&mut self, message: String) {
        if let Some(AdvancedInspectPrompt::Jump { error, .. }) = self
            .inspect
            .advanced
            .as_mut()
            .and_then(|state| state.prompt.as_mut())
        {
            *error = Some(message);
        }
    }

    pub fn advanced_inspect_submit_prompt(
        &mut self,
    ) -> Result<Option<(AdvancedInspectSource, u64)>, String> {
        let prompt = self
            .inspect
            .advanced
            .as_ref()
            .and_then(|state| state.prompt.clone())
            .ok_or_else(|| "检查输入面板未打开".to_string())?;

        let request = match prompt {
            AdvancedInspectPrompt::Jump {
                input,
                origin_view,
                origin_panel,
                origin_pane,
                origin_sector_mode,
                ..
            } => {
                let result = (|| {
                    let value = parse_inspect_jump_number(&input)?;
                    self.advanced_inspect_jump_lba(value)?;
                    self.advanced_inspect_prepare_jump_sector(
                        value,
                        origin_view,
                        origin_sector_mode.unwrap_or(SectorInspectMode::Mixed),
                    )
                })();
                match result {
                    Ok(request) => {
                        if let Some(state) = self.inspect.advanced.as_mut() {
                            state.view_mode = origin_view;
                            state.panel = origin_panel;
                            state.pane_focus.focus(origin_pane);
                        }
                        request
                    }
                    Err(message) => {
                        self.advanced_inspect_set_jump_error(message.clone());
                        return Err(message);
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
                .ok_or_else(|| "检查工作区不可用".to_string())?;
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
                let row_path = tree_state::inspect_row_path(&node_path);
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
