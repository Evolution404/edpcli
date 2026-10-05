//! Compact operation status for terminals with limited vertical space.

use super::{operation_title, overall_label, stage_completion_label};
use crate::application::progress::{OperationKind, OperationRunState};
use crate::tui::operation_progress_status::{phase_label, unit_label};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

pub(super) fn draw_compact_progress(
    frame: &mut Frame,
    area: Rect,
    run: &OperationRunState,
    animation_frame: u64,
) {
    let theme = crate::tui::theme::current();
    let latest = run.latest.as_ref();
    if run.operation == OperationKind::PostRestoreFormat {
        let mut lines = vec![Line::from(run.target.clone())];
        if let Some(event) = latest {
            lines.push(Line::from(Span::styled(
                format!(
                    "{} {}",
                    crate::tui::animation::spinner_glyph(animation_frame),
                    event.step.label()
                ),
                theme.accent(),
            )));
            lines.push(Line::from(stage_completion_label(latest)));
            if let Some(work) = event.work {
                lines.push(Line::from(format!(
                    "{} / {} {} · {}%",
                    work.current,
                    work.total,
                    unit_label(work.unit),
                    work.percent()
                )));
            } else {
                lines.push(Line::from("执行中，请等待阶段完成"));
            }
        }
        lines.push(Line::from(format!(
            "已用时 {}s · 最近更新 {}s 前",
            run.started_at.elapsed().as_secs(),
            run.last_activity_at.elapsed().as_secs()
        )));
        lines.push(Line::from("退出请求将在安全结束点处理"));
        frame.render_widget(
            Paragraph::new(lines)
                .block(crate::tui::ui::card(operation_title(run.operation), true))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let compact_mode = match run.operation {
        OperationKind::Backup => "只读",
        OperationKind::Restore | OperationKind::Provision | OperationKind::PostRestoreFormat => {
            "安全事务"
        }
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("{} · {compact_mode}", operation_title(run.operation)),
            theme.accent().add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("  {}", run.target)),
    ])];
    if let Some(event) = latest {
        lines.push(Line::from(format!(
            "总体进度 {} · 当前阶段 {} · 当前步骤 {}",
            overall_label(event.overall.basis_points()),
            phase_label(event),
            event.step.label()
        )));
        if let Some(work) = event.work {
            lines.push(Line::from(format!(
                "{} / {} {} · {}%",
                work.current,
                work.total,
                unit_label(work.unit),
                work.percent()
            )));
        }
    }
    let safety = match run.operation {
        OperationKind::Backup => "安全提示  只读操作执行中；等待安全结束点。",
        OperationKind::Restore | OperationKind::Provision | OperationKind::PostRestoreFormat => {
            "安全提示  关键写入安全事务执行中；不会绕过同步、读回或回滚安全点。"
        }
    };
    lines.push(Line::from(Span::styled(safety, theme.muted())));
    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card(operation_title(run.operation), true))
            .wrap(Wrap { trim: false }),
        area,
    );
}
