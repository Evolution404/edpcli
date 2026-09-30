use super::*;

impl AppState {
    pub fn provision_begin_export(&mut self) {
        if self.provision.stage != ProvisionStage::Review || self.provision.prepared.is_none() {
            return;
        }
        self.provision.export_path = match self.provision.kind.mode() {
            Some(mode) => format!("./edp-mode{mode}.img"),
            None => "./edp-plain.img".into(),
        };
        self.provision_transition_begin_export_path();
    }

    pub fn provision_export_push_char(&mut self, ch: char) {
        if self.provision.stage == ProvisionStage::ExportPath
            && !ch.is_control()
            && self.provision.export_path.chars().count() < 512
        {
            self.provision.export_path.push(ch);
            self.provision.message = None;
        }
    }

    pub fn provision_export_backspace(&mut self) {
        if self.provision.stage == ProvisionStage::ExportPath {
            self.provision.export_path.pop();
            self.provision.message = None;
        }
    }

    pub fn provision_take_export(
        &mut self,
    ) -> Option<(
        crate::application::provision::PreparedProvision,
        std::path::PathBuf,
    )> {
        if self.provision.stage != ProvisionStage::ExportPath {
            return None;
        }
        let path = self.provision.export_path.trim();
        if path.is_empty() {
            self.provision.message = Some("镜像导出路径不能为空".into());
            return None;
        }
        let prepared = self.provision.prepared.as_ref()?.clone();
        let path = std::path::PathBuf::from(path);
        self.provision_transition_begin_exporting();
        self.provision.message = Some(format!("正在后台导出 {}…", path.display()));
        Some((prepared, path))
    }

    pub fn provision_finish_export(&mut self, result: Result<std::path::PathBuf, String>) {
        self.provision_transition_return_to_review();
        self.provision.message = Some(match result {
            Ok(path) => format!("镜像导出完成：{}", path.display()),
            Err(message) => message,
        });
    }

    pub fn provision_cancel_export(&mut self) {
        if self.provision.stage == ProvisionStage::ExportPath {
            self.provision_transition_return_to_review();
            self.provision.message = None;
        }
    }

    pub fn provision_begin_confirm(&mut self) {
        if self.provision.prepared.is_some() {
            self.provision_transition_begin_confirm();
        }
    }

    pub fn provision_push_confirmation(&mut self, ch: char) {
        if self.provision.stage == ProvisionStage::Confirm && self.provision.confirmation.len() < 16
        {
            self.provision.confirmation.push(ch);
            self.provision.message = None;
        }
    }

    pub fn provision_backspace_confirmation(&mut self) {
        if self.provision.stage == ProvisionStage::Confirm {
            self.provision.confirmation.pop();
            self.provision.message = None;
        }
    }

    pub fn provision_take_for_write(&mut self) -> Option<ProvisionPrepared> {
        if self.provision.stage != ProvisionStage::Confirm {
            return None;
        }
        if self.provision.confirmation != "YES" {
            self.provision.message = Some("必须精确输入 YES 才会开始向目标设备写入".into());
            return None;
        }
        let total_bytes = self
            .selected_device()
            .map(|row| row.size)
            .unwrap_or_default();
        let prepared = self.provision.prepared.take()?;
        self.provision.result_plan = Some(ProvisionResultSnapshot::from_prepared(
            &prepared,
            total_bytes,
        ));
        self.provision_transition_begin_running();
        self.provision.message = Some("事务写盘进行中；退出请求会延迟到安全检查点".into());
        self.provision.result_status = None;
        self.provision.result_outcome = None;
        self.provision.run = Some(crate::application::progress::OperationRunState::new(
            crate::application::progress::OperationKind::Provision,
            format!("disk{}", prepared.disk()),
        ));
        self.provision.pane_focus = crate::tui::pane::PaneFocus::provision_running();
        self.shell.critical_operation = true;
        Some(prepared)
    }

    pub fn provision_finish_write(
        &mut self,
        result: Result<crate::application::provision::ProvisionWriteOutcome, String>,
    ) {
        self.shell.critical_operation = false;
        match result {
            Ok(outcome) => {
                self.provision.result_status = Some(outcome.execution_status());
                self.provision.message = None;
                self.provision.result_outcome = Some(outcome);
            }
            Err(message) => {
                self.provision.result_status =
                    Some(crate::application::provision::ProvisionExecutionStatus::FatalFailure);
                self.provision.result_outcome = None;
                self.provision.message = Some(message);
            }
        }
        self.provision_initialize_result_workbench();
        self.provision_transition_finish_running();
    }
}
