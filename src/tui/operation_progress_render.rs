use crate::application::progress::{
    LogPolicy, OperationKind, OperationRunState, ProgressEvent, Severity, Step,
};
use crate::tui::operation_progress_status::{draw_current_status, phase_label, unit_label};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Cell, Gauge, Paragraph, Row, Table, Wrap},
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

fn milestone_status_symbol(event: &ProgressEvent) -> &'static str {
    match event.severity {
        Severity::Error => "✗",
        Severity::Warning => "!",
        Severity::Info => "✓",
    }
}

fn log_event_label(event: &ProgressEvent) -> String {
    match event.step {
        Step::PartitionFormat(role) => format!("{}格式化与读回", role.label()),
        _ => event.step.label().to_string(),
    }
}

fn current_snapshot(run: &OperationRunState) -> Option<&ProgressEvent> {
    run.latest.as_ref().filter(|event| {
        event.severity == Severity::Info && event.log_policy == LogPolicy::SnapshotOnly
    })
}

pub(crate) fn draw_operation_progress(
    frame: &mut Frame,
    area: Rect,
    run: &OperationRunState,
    animation_frame: u64,
) {
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
    let header = Row::new([
        Cell::from("时间"),
        Cell::from("状态"),
        Cell::from("阶段"),
        Cell::from("事件"),
    ])
    .style(theme.muted());
    let current = current_snapshot(run);
    let logged_current_index = if current.is_none()
        && run.latest.as_ref().is_some_and(|latest| {
            latest.severity == Severity::Info
                && latest.step != Step::Completed
                && run.log.back().is_some_and(|event| event == latest)
        }) {
        run.log.len().checked_sub(1)
    } else {
        None
    };
    let history_capacity = visible_log_rows.saturating_sub(usize::from(current.is_some()));
    let mut rows = run
        .log
        .iter()
        .enumerate()
        .rev()
        .take(history_capacity)
        .rev()
        .map(|(index, event)| {
            let is_running = logged_current_index == Some(index);
            let status = if is_running {
                crate::tui::animation::spinner_glyph(animation_frame)
            } else {
                milestone_status_symbol(event)
            };
            let status_style = match event.severity {
                Severity::Error => theme.danger(),
                Severity::Warning => theme.warning(),
                Severity::Info if is_running => theme.accent(),
                Severity::Info => theme.success(),
            };
            let elapsed = event
                .emitted_at
                .saturating_duration_since(run.started_at)
                .as_secs();
            let stage = event
                .stage
                .map(|stage| format!("{}/{} {}", stage.current, stage.total, event.phase.label()))
                .unwrap_or_else(|| event.phase.label().to_string());
            let detail = event
                .detail
                .as_deref()
                .map(|detail| format!(" · {detail}"))
                .unwrap_or_default();
            Row::new([
                Cell::from(format!("+{elapsed}s")).style(theme.secondary_text()),
                Cell::from(status).style(status_style),
                Cell::from(stage).style(theme.secondary_text()),
                Cell::from(format!("{}{detail}", log_event_label(event)))
                    .style(theme.secondary_text()),
            ])
        })
        .collect::<Vec<_>>();
    if let Some(event) = current {
        let elapsed = event
            .emitted_at
            .saturating_duration_since(run.started_at)
            .as_secs();
        let stage = event
            .stage
            .map(|stage| format!("{}/{} {}", stage.current, stage.total, event.phase.label()))
            .unwrap_or_else(|| event.phase.label().to_string());
        let detail = event
            .detail
            .as_deref()
            .map(|detail| format!(" · {detail}"))
            .unwrap_or_default();
        rows.push(Row::new([
            Cell::from(format!("+{elapsed}s")).style(theme.secondary_text()),
            Cell::from(crate::tui::animation::spinner_glyph(animation_frame)).style(theme.accent()),
            Cell::from(stage).style(theme.secondary_text()),
            Cell::from(format!("{}{detail}", log_event_label(event))).style(theme.secondary_text()),
        ]));
    }
    if rows.is_empty() {
        rows.push(Row::new([
            Cell::from("—").style(theme.muted()),
            Cell::from("—").style(theme.muted()),
            Cell::from("—").style(theme.muted()),
            Cell::from("暂无运行记录").style(theme.muted()),
        ]));
    }
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(8),
                Constraint::Length(6),
                Constraint::Length(16),
                Constraint::Min(1),
            ],
        )
        .header(header)
        .column_spacing(1)
        .block(crate::tui::ui::card("运行记录", true)),
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
