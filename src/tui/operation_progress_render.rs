use crate::application::progress::{OperationKind, OperationRunState, Severity, Unit};
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

fn unit_label(unit: Unit) -> &'static str {
    match unit {
        Unit::Steps => "step",
        Unit::Sectors => "sector",
        Unit::Bytes => "byte",
    }
}

fn activity_label(
    activity: crate::application::progress::TransactionActivityPhase,
) -> &'static str {
    use crate::application::progress::TransactionActivityPhase as Activity;
    match activity {
        Activity::Mirror => "镜像准备",
        Activity::Write => "写入",
        Activity::Readback => "读回",
        Activity::RollbackWrite => "回滚写入",
        Activity::RollbackReadback => "回滚读回",
        Activity::FormatWrite => "格式化写入",
        Activity::FormatReadback => "格式化读回",
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
                event.phase.label(),
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
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(6),
            Constraint::Length(2),
        ])
        .split(area);

    let elapsed = std::time::Instant::now()
        .saturating_duration_since(run.started_at)
        .as_secs();
    let last_activity = std::time::Instant::now()
        .saturating_duration_since(run.last_activity_at)
        .as_secs();
    let summary = vec![
        Line::from(vec![
            Span::styled(
                operation_title(run.operation),
                theme.accent().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("  {}", run.target)),
        ]),
        Line::from(format!(
            "运行时间  {elapsed}s    最近活动  {last_activity}s 前"
        )),
    ];
    frame.render_widget(
        Paragraph::new(summary).block(crate::tui::ui::card("操作 / 目标", true)),
        chunks[0],
    );

    let overall = latest.map_or(0.0, |event| event.overall.ratio());
    let overall_label = latest
        .map(|event| overall_label(event.overall.basis_points()))
        .unwrap_or_else(|| "0.00%".into());
    frame.render_widget(
        Gauge::default()
            .block(crate::tui::ui::card("总体进度", true))
            .gauge_style(theme.accent())
            .ratio(overall)
            .label(overall_label),
        chunks[1],
    );

    let mut current_lines = Vec::new();
    if let Some(event) = latest {
        current_lines.push(Line::from(format!(
            "当前阶段  {}    当前步骤  {}",
            event.phase.label(),
            event.step.label()
        )));
        if let Some(detail) = event.detail.as_deref() {
            current_lines.push(Line::from(detail));
        }
        if let Some(activity) = event.work.and_then(|work| work.activity) {
            current_lines.push(Line::from(format!(
                "扇区活动  {}",
                activity_label(activity)
            )));
        }
    } else {
        current_lines.push(Line::from("等待进度事件"));
    }
    frame.render_widget(
        Paragraph::new(current_lines)
            .block(crate::tui::ui::card("当前状态", true))
            .wrap(Wrap { trim: false }),
        chunks[2],
    );

    let log_lines = if run.log.is_empty() {
        vec![Line::from(Span::styled("暂无语义活动日志", theme.muted()))]
    } else {
        run.log
            .iter()
            .rev()
            .take(chunks[3].height.saturating_sub(2) as usize)
            .rev()
            .map(|event| {
                let style = match event.severity {
                    Severity::Info => theme.secondary_text(),
                    Severity::Warning => theme.warning(),
                    Severity::Error => theme.danger(),
                };
                let activity = event
                    .work
                    .and_then(|work| work.activity)
                    .map(|activity| format!("扇区活动 {} · ", activity_label(activity)))
                    .unwrap_or_default();
                Line::from(Span::styled(
                    format!(
                        "{activity}{} · {}{}",
                        event.phase.label(),
                        event.step.label(),
                        event
                            .detail
                            .as_deref()
                            .map(|detail| format!(" · {detail}"))
                            .unwrap_or_default()
                    ),
                    style,
                ))
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(log_lines)
            .block(crate::tui::ui::card("运行日志 · 最近语义活动", true))
            .wrap(Wrap { trim: false }),
        chunks[3],
    );

    let work_area = Rect::new(
        chunks[2].x,
        chunks[2].bottom().saturating_sub(2),
        chunks[2].width,
        2,
    );
    if let Some(work) = latest.and_then(|event| event.work) {
        let label = format!(
            "{} / {} {} · {}%",
            work.current,
            work.total,
            unit_label(work.unit),
            work.percent()
        );
        frame.render_widget(
            Gauge::default()
                .gauge_style(theme.secondary_accent())
                .ratio(work.ratio())
                .label(label),
            work_area,
        );
    }

    let safety = match run.operation {
        OperationKind::Backup => "安全提示  只读操作执行中；等待安全结束点。",
        OperationKind::Restore | OperationKind::Provision => {
            "安全提示  关键写入安全事务执行中；q / Esc / Ctrl-C 不会绕过同步、读回或回滚安全点。"
        }
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(safety, theme.muted()))),
        chunks[4],
    );
}
