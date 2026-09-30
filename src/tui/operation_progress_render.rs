use crate::application::progress::{OperationKind, OperationRunState, Phase, Severity, Step};
use crate::tui::operation_progress_status::{draw_current_status, phase_label, unit_label};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Gauge, Paragraph, Wrap},
    Frame,
};

fn operation_title(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Backup => "备份制作",
        OperationKind::Restore => "恢复备份",
        OperationKind::Provision => "制盘执行",
    }
}

fn overall_label(basis_points: u16) -> String {
    if basis_points.is_multiple_of(100) {
        format!("{}%", basis_points / 100)
    } else {
        format!("{:.2}%", f64::from(basis_points) / 100.0)
    }
}

pub(crate) fn draw_operation_progress(frame: &mut Frame, area: Rect, run: &OperationRunState) {
    let theme = crate::tui::theme::current();
    let latest = run.latest.as_ref();
    if area.height < 18 {
        let compact_mode = match run.operation {
            OperationKind::Backup => "只读",
            OperationKind::Restore | OperationKind::Provision => "安全事务",
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
            OperationKind::Restore | OperationKind::Provision => {
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
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(8),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(area);

    let overall = latest.map_or(0.0, |event| event.overall.ratio());
    let overall_label = latest
        .map(|event| overall_label(event.overall.basis_points()))
        .unwrap_or_else(|| "0.00%".into());
    frame.render_widget(
        Gauge::default()
            .block(crate::tui::ui::card("总体进度", true))
            .gauge_style(theme.accent())
            .ratio(overall)
            .label(Span::styled(overall_label, theme.progress_label())),
        chunks[0],
    );

    draw_current_status(frame, chunks[1], run);

    let visible_log_rows = chunks[2].height.saturating_sub(3) as usize;
    let mut log_lines = vec![Line::from(Span::styled(
        "时间       状态  阶段        事件",
        theme.muted(),
    ))];
    if run.log.is_empty() {
        log_lines.push(Line::from(Span::styled("暂无运行记录", theme.muted())));
    } else {
        log_lines.extend(
            run.log
                .iter()
                .rev()
                .take(visible_log_rows)
                .rev()
                .map(|event| {
                    let style = match event.severity {
                        Severity::Info => theme.secondary_text(),
                        Severity::Warning => theme.warning(),
                        Severity::Error => theme.danger(),
                    };
                    let status = match event.severity {
                        Severity::Error => "✗",
                        Severity::Warning => "!",
                        Severity::Info
                            if event.phase == Phase::Complete || event.step == Step::Completed =>
                        {
                            "✓"
                        }
                        Severity::Info => "•",
                    };
                    let elapsed = event
                        .emitted_at
                        .saturating_duration_since(run.started_at)
                        .as_secs();
                    let stage = event
                        .stage
                        .map(|stage| {
                            format!("{}/{} {}", stage.current, stage.total, event.phase.label())
                        })
                        .unwrap_or_else(|| event.phase.label().to_string());
                    let detail = event
                        .detail
                        .as_deref()
                        .map(|detail| format!(" · {detail}"))
                        .unwrap_or_default();
                    Line::from(Span::styled(
                        format!(
                            "+{elapsed:<8}s {status:<4} {stage:<10} {}{detail}",
                            event.step.label()
                        ),
                        style,
                    ))
                }),
        );
    }
    frame.render_widget(
        Paragraph::new(log_lines)
            .block(crate::tui::ui::card("运行记录", true))
            .wrap(Wrap { trim: false }),
        chunks[2],
    );

    let safety = match run.operation {
        OperationKind::Backup => "安全提示  只读操作执行中；等待安全结束点。",
        OperationKind::Restore | OperationKind::Provision => {
            "安全提示  关键写入安全事务执行中；q / Esc / Ctrl-C 不会绕过同步、读回或回滚安全点。"
        }
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(safety, theme.muted()))),
        chunks[3],
    );
}
