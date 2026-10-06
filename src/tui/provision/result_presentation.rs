//! One result-state projection shared by drawing and copied table values.
use crate::tui::state::{ProvisionResultPartition, ProvisionState};

pub(crate) fn disposition_label(disposition: crate::provision::RegionDisposition) -> &'static str {
    use crate::provision::RegionDisposition as D;
    match disposition {
        D::PreserveOpaque => "原样保留",
        D::PreserveVerified => "验证保留",
        D::RewrapVerified => "密钥已更新",
        D::Rebuild => "已重建",
        D::Drop => "已移除",
    }
}

pub(crate) fn partition_final_status(
    provision: &ProvisionState,
    plan: &crate::tui::state::ProvisionResultSnapshot,
    partition: &ProvisionResultPartition,
) -> (String, crate::tui::ui::ResultTone) {
    let outcome = provision.result_outcome.as_ref();
    if outcome.is_none() {
        return ("未确认".into(), crate::tui::ui::ResultTone::Warning);
    }
    if plan.target == crate::provision::ProvisionTarget::Plain {
        return if outcome.is_some() {
            (
                "已写入 · 读回通过".into(),
                crate::tui::ui::ResultTone::Success,
            )
        } else {
            ("未确认".into(), crate::tui::ui::ResultTone::Warning)
        };
    }

    if partition.selected_for_format {
        if let (
            Some(role),
            Some(crate::application::provision::ProvisionCommitOutcome::Official(report)),
        ) = (partition.role, outcome.map(|value| &value.commit))
        {
            if let Some(format) = report.formats.iter().find(|item| item.role == role) {
                return match &format.result {
                    Ok(()) => (
                        "已格式化 · 读回通过".into(),
                        crate::tui::ui::ResultTone::Success,
                    ),
                    Err(error) if error.is_skipped() => {
                        ("未执行".into(), crate::tui::ui::ResultTone::Muted)
                    }
                    Err(error)
                        if error
                            .operation_error()
                            .and_then(|error| error.media_state)
                            .is_some_and(|state| state.requires_reinspection()) =>
                    {
                        ("介质状态未确认".into(), crate::tui::ui::ResultTone::Danger)
                    }
                    Err(error)
                        if error.operation_error().and_then(|error| error.media_state)
                            == Some(crate::application::error::MediaState::RolledBack) =>
                    {
                        (
                            "格式化失败 · 已回滚".into(),
                            crate::tui::ui::ResultTone::Warning,
                        )
                    }
                    Err(_) => ("格式化失败".into(), crate::tui::ui::ResultTone::Warning),
                };
            }
        }
        return ("已写入".into(), crate::tui::ui::ResultTone::Primary);
    }

    (
        partition
            .disposition
            .map(disposition_label)
            .unwrap_or("未格式化")
            .into(),
        crate::tui::ui::ResultTone::Success,
    )
}
