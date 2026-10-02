use super::*;
use crate::tui::pane::PaneId;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

fn status_label(
    status: Option<crate::application::provision::ProvisionExecutionStatus>,
) -> (&'static str, crate::tui::ui::ResultTone) {
    use crate::application::provision::ProvisionExecutionStatus as Status;
    match status {
        Some(Status::Success) => ("✓ 制 盘 成 功", crate::tui::ui::ResultTone::Success),
        Some(Status::CompletedWithWarnings) => {
            ("⚠ 制盘完成 · 存在警告", crate::tui::ui::ResultTone::Warning)
        }
        Some(Status::PartialFormatFailure) => (
            "⚠ 制盘完成 · 部分格式化失败",
            crate::tui::ui::ResultTone::Warning,
        ),
        Some(Status::FatalFailure) | None => ("✗ 制 盘 失 败", crate::tui::ui::ResultTone::Danger),
    }
}

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

#[path = "result_partition_layout.rs"]
mod result_partition_layout;
use result_partition_layout::{render_layout_pane, render_partition_pane};

fn verification_lines(state: &AppState) -> Vec<(String, crate::tui::ui::ResultTone)> {
    use crate::application::provision::ProvisionCommitOutcome;
    let mut lines = Vec::new();
    if let Some(outcome) = state.provision().result_outcome.as_ref() {
        lines.push((
            "✓ 制盘前自动备份已创建".into(),
            crate::tui::ui::ResultTone::Success,
        ));
        match &outcome.commit {
            ProvisionCommitOutcome::Official(report) => {
                if report.provision_succeeded {
                    lines.push((
                        "✓ 协议与几何读回验证通过".into(),
                        crate::tui::ui::ResultTone::Success,
                    ));
                }
                for item in &report.formats {
                    lines.push(if item.result.is_ok() {
                        (
                            format!("✓ {}格式化读回通过", item.role.label()),
                            crate::tui::ui::ResultTone::Success,
                        )
                    } else {
                        (
                            format!("⚠ {}格式化失败", item.role.label()),
                            crate::tui::ui::ResultTone::Warning,
                        )
                    });
                }
            }
            ProvisionCommitOutcome::Plain { partition_count } => {
                lines.push((
                    format!("✓ {partition_count} 个 MBR 分区写入并读回"),
                    crate::tui::ui::ResultTone::Success,
                ));
                lines.push((
                    "✓ LBA3 保留且 EDP 状态已清除".into(),
                    crate::tui::ui::ResultTone::Success,
                ));
            }
        }
        lines.extend(outcome.warnings.iter().map(|warning| {
            (
                format!("⚠ {}", warning.message()),
                crate::tui::ui::ResultTone::Warning,
            )
        }));
        let file = outcome
            .backup
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("<无效文件名>");
        lines.push((
            format!("备份文件  {file}"),
            crate::tui::ui::ResultTone::Primary,
        ));
    } else {
        lines.push((
            state
                .provision()
                .message
                .as_ref()
                .map(|message| message.text().to_string())
                .unwrap_or_else(|| "写入未完成".into()),
            crate::tui::ui::ResultTone::Danger,
        ));
    }
    if let Some(run) = state.provision().run.as_ref() {
        let elapsed = run
            .last_activity_at
            .duration_since(run.started_at)
            .as_secs();
        lines.push((
            format!("总耗时  {elapsed} 秒"),
            crate::tui::ui::ResultTone::Muted,
        ));
    }
    lines
}

fn render_verification_pane(frame: &mut Frame, area: Rect, state: &AppState, focused: bool) {
    let block = crate::tui::ui::card("验收与执行", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let offset = state
        .provision()
        .result_workbench
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

pub(super) fn draw_provision_result(frame: &mut Frame, area: Rect, state: &AppState) {
    let provision = state.provision();
    let (status, tone) = status_label(provision.result_status);
    let plan = provision.result_plan.as_ref();
    let disk = plan
        .map(|plan| format!("disk{}", plan.disk))
        .or_else(|| {
            state
                .provision_target_disk()
                .map(|disk| format!("disk{disk}"))
        })
        .unwrap_or_else(|| "—".into());
    let target = plan
        .map(|plan| plan.target.full_name())
        .unwrap_or_else(|| provision.kind.title());
    let capacity = plan
        .map(|plan| crate::common::fmt_capacity(plan.total_bytes))
        .unwrap_or_else(|| "—".into());
    let detail = format!("{disk} · {target} · {capacity} · Enter / Esc 返回设备列表");
    let hero = crate::tui::result_workbench::ResultHero::new("制盘结果", status, detail, tone);
    let slots = crate::tui::result_workbench::render_result_workbench_shell(
        frame,
        area,
        &provision.result_workbench,
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
