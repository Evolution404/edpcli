use super::*;
use ratatui::{
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

fn verification_lines(state: &AppState) -> Vec<(String, crate::tui::ui::ResultTone)> {
    let Some(wizard) = state.wizard() else {
        return vec![];
    };
    let Some(outcome) = wizard.restore_outcome.as_ref() else {
        return vec![];
    };
    let mut lines = vec![
        (
            format!(
                "元数据写入  {}",
                if outcome.report.metadata_restored {
                    "完成 ✓"
                } else {
                    "未完成"
                }
            ),
            if outcome.report.metadata_restored {
                crate::tui::ui::ResultTone::Success
            } else {
                crate::tui::ui::ResultTone::Danger
            },
        ),
        (
            format!(
                "读回验证    {}",
                if outcome.report.readback_verified {
                    "通过 ✓"
                } else {
                    "未通过"
                }
            ),
            if outcome.report.readback_verified {
                crate::tui::ui::ResultTone::Success
            } else {
                crate::tui::ui::ResultTone::Danger
            },
        ),
        (
            format!(
                "恢复工件    {} 项",
                outcome.report.restored_artifact_ids.len()
            ),
            crate::tui::ui::ResultTone::Primary,
        ),
        (
            format!("设备状态    {}", outcome.device_state),
            crate::tui::ui::ResultTone::Primary,
        ),
        (
            format!(
                "全盘布局    {}",
                if outcome.layout.is_ok() {
                    "已验证 ✓"
                } else {
                    "投影失败 · 不影响元数据恢复成功"
                }
            ),
            if outcome.layout.is_ok() {
                crate::tui::ui::ResultTone::Success
            } else {
                crate::tui::ui::ResultTone::Warning
            },
        ),
    ];
    if let Err(message) = &outcome.layout {
        lines.push((
            format!("布局说明    {}", crate::ui::sanitize_terminal_text(message)),
            crate::tui::ui::ResultTone::Warning,
        ));
    }
    for issue in &outcome.assessment.issues {
        lines.push((
            format!("检查提示    {}", crate::ui::sanitize_terminal_text(issue)),
            crate::tui::ui::ResultTone::Warning,
        ));
    }
    if let Some(message) = &wizard.message {
        lines.push((
            format!(
                "当前提示    {}",
                crate::ui::sanitize_terminal_text(message.text())
            ),
            message.result_tone(),
        ));
    }
    lines
}

pub(super) fn render_verification_pane(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    focused: bool,
) {
    let block = crate::tui::ui::card("验收与执行", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let Some(wizard) = state.wizard() else {
        return;
    };
    let offset = wizard
        .post_restore_workbench
        .viewport(PaneId::ResultVerification)
        .scroll_y
        .offset;
    let mut lines = verification_lines(state)
        .into_iter()
        .skip(offset)
        .take(inner.height as usize)
        .map(|(text, tone)| Line::from(Span::styled(text, tone_style(tone))))
        .collect::<Vec<_>>();

    if lines.len() < inner.height as usize {
        lines.push(Line::from(""));
        lines.push(crate::tui::result_workbench::result_verification_navigation_hint());
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}
