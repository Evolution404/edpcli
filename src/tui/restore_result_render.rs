use ratatui::{layout::Rect, style::Style, Frame};

use crate::application::post_restore::PostRestorePartitionState;
use crate::tui::pane::PaneId;
use crate::tui::state::AppState;

fn tone_style(tone: crate::tui::ui::ResultTone) -> Style {
    let theme = crate::tui::theme::current();
    match tone {
        crate::tui::ui::ResultTone::Primary => theme.body_text(),
        crate::tui::ui::ResultTone::Muted => theme.muted(),
        crate::tui::ui::ResultTone::Accent => theme.accent(),
        crate::tui::ui::ResultTone::Success => theme.success(),
        crate::tui::ui::ResultTone::Warning => theme.warning(),
        crate::tui::ui::ResultTone::Danger => theme.danger(),
    }
}

#[path = "restore_result_partition_layout.rs"]
mod partition_layout;
#[path = "restore_result_verification.rs"]
mod verification;

use partition_layout::{render_layout_pane, render_partition_pane};
use verification::render_verification_pane;

pub(super) fn draw_post_restore_result(frame: &mut Frame, area: Rect, state: &AppState) {
    let Some(wizard) = state.wizard() else {
        return;
    };
    let Some(outcome) = wizard.restore_outcome.as_ref() else {
        return;
    };

    let needs_attention = outcome.layout.is_err()
        || !outcome.assessment.issues.is_empty()
        || outcome
            .assessment
            .partitions
            .iter()
            .any(|partition| partition.state != PostRestorePartitionState::Usable);
    let (status, tone) = if needs_attention {
        (
            "⚠ 元数据恢复成功 · 需要后续处理",
            crate::tui::ui::ResultTone::Warning,
        )
    } else {
        ("✓ 元数据恢复成功", crate::tui::ui::ResultTone::Success)
    };
    let detail = format!(
        "disk{} · {} 个分区 · 文件数据未恢复 · Esc 完成",
        wizard.disk,
        outcome.assessment.partitions.len()
    );
    let hero = crate::tui::result_workbench::ResultHero::new("恢复结果", status, detail, tone);
    let slots = crate::tui::result_workbench::render_result_workbench_shell(
        frame,
        area,
        &wizard.post_restore_workbench,
        &hero,
    );

    for slot in slots {
        match slot.pane {
            PaneId::ResultPartitions => {
                render_partition_pane(frame, slot.area, state, slot.focused)
            }
            PaneId::ResultDiskLayout => render_layout_pane(frame, slot.area, state, slot.focused),
            PaneId::ResultVerification => {
                render_verification_pane(frame, slot.area, state, slot.focused)
            }
            _ => {}
        }
    }
}
