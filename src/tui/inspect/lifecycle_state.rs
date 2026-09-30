use super::tree_state::reset_inspect_selected_context;
use super::*;

impl AppState {
    pub fn begin_advanced_inspect(&mut self, source: AdvancedInspectSource) -> bool {
        if self.shell.critical_operation {
            self.set_notice("关键操作仍在执行，完成前不能启动全盘检查。");
            return false;
        }
        self.push_navigation_frame(NavigationLocation::from_workspace(self.shell.workspace));
        if let AdvancedInspectSource::Disk(disk) = &source {
            self.shell.pinned_disk = Some(*disk);
        }
        self.shell.workspace = crate::tui::state::Workspace::Inspect;
        let mut expanded = std::collections::BTreeSet::new();
        expanded.insert("device".to_string());
        self.inspect.advanced = Some(AdvancedInspectState {
            source,
            stage: AdvancedInspectStage::Running,
            result: None,
            tree_selected: 0,
            panel: AdvancedInspectPanel::Tree,
            view_mode: InspectViewMode::Business,
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
        self.shell.input_mode = InputMode::Normal;
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
            .inspect
            .advanced
            .as_ref()
            .ok_or_else(|| "全盘检查未打开".to_string())?;
        let device_id_override = match &state.source {
            AdvancedInspectSource::Disk(disk) => self
                .devices
                .rows
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
        let Some(state) = self.inspect.advanced.as_mut() else {
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

    pub fn close_advanced_inspect(&mut self) {
        if self
            .inspect
            .advanced
            .as_ref()
            .is_some_and(|state| state.stage != AdvancedInspectStage::Running)
        {
            self.inspect.advanced = None;
            self.restore_workspace_frame();
        }
    }
}
