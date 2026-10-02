use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionSurface {
    Form,
    Review,
    Running,
    Result,
}

impl ProvisionSurface {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Form => "制盘配置",
            Self::Review => "计划确认",
            Self::Running => "执行",
            Self::Result => "完成",
        }
    }
}

impl AppState {
    pub fn provision_surface(&self) -> ProvisionSurface {
        match self.provision.stage {
            ProvisionStage::Form | ProvisionStage::Planning => ProvisionSurface::Form,
            ProvisionStage::Review
            | ProvisionStage::ExportPath
            | ProvisionStage::Exporting
            | ProvisionStage::Confirm => ProvisionSurface::Review,
            ProvisionStage::Running => ProvisionSurface::Running,
            ProvisionStage::Result => ProvisionSurface::Result,
        }
    }

    pub fn provision_step_index(&self) -> usize {
        match self.provision.stage {
            ProvisionStage::Form => 0,
            ProvisionStage::Planning => 1,
            ProvisionStage::Review
            | ProvisionStage::ExportPath
            | ProvisionStage::Exporting
            | ProvisionStage::Confirm => 2,
            ProvisionStage::Running => 3,
            ProvisionStage::Result => 4,
        }
    }

    pub fn provision_breadcrumb(&self) -> String {
        let disk = self
            .provision_target_disk()
            .map(|disk| format!("disk{disk}"))
            .unwrap_or_else(|| "未固定目标".into());
        let kind = self
            .provision
            .kind
            .mode()
            .map(|mode| format!("mode{mode}"))
            .unwrap_or_else(|| "Plain".into());
        format!(
            "设备 > {disk} > 制盘 > {kind} > {}",
            self.provision_surface().label()
        )
    }

    pub fn provision_escape_hint(&self) -> &'static str {
        match self.provision_surface() {
            ProvisionSurface::Form => "Esc 返回：设备列表",
            ProvisionSurface::Review => "Esc 返回：制盘配置",
            ProvisionSurface::Running => "写盘执行中 · Esc 暂不可返回",
            ProvisionSurface::Result => "Enter / Esc 返回：设备列表",
        }
    }
}
