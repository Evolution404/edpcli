use super::*;

pub(super) fn draw_provision_running(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    let run = state.provision().run.as_ref();
    let latest = run.and_then(|run| run.latest.as_ref());
    let progress = latest
        .map(|event| {
            format!(
                "{}/{} 步 · {}%",
                event.current,
                event.total,
                event.current.saturating_mul(100) / event.total
            )
        })
        .unwrap_or_else(|| "等待进度事件".into());
    let phase = latest.map(|event| event.phase.label()).unwrap_or("准备中");
    let step = latest
        .map(|event| match event.step {
            crate::application::progress::Step::PartitionFormat(role) => {
                format!("{} · {}", event.step.label(), role.label())
            }
            _ => event.step.label().to_string(),
        })
        .unwrap_or_else(|| "等待第一步".into());
    if area.height < 18 {
        let lines = [
            Line::from(format!("总体进度  {progress}")),
            Line::from(format!("当前阶段  {phase}")),
            Line::from(format!("当前步骤  {step}")),
            Line::from("运行日志  详见较高窗口"),
            Line::from("安全提示  退出请求仅在安全检查点生效"),
        ];
        frame.render_widget(
            Paragraph::new(lines.to_vec()).block(crate::tui::ui::card("安全事务执行中", true)),
            area,
        );
        return;
    }

    let areas = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(4),
        Constraint::Min(4),
        Constraint::Length(3),
    ])
    .split(area);
    let now = std::time::Instant::now();
    let elapsed = run
        .map(|run| now.duration_since(run.started_at).as_secs())
        .unwrap_or(0);
    let activity = run
        .map(|run| now.duration_since(run.last_activity_at).as_secs())
        .unwrap_or(0);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!("总体进度  {progress}")),
            Line::from(format!(
                "运行时间  {elapsed} 秒    最近活动  {activity} 秒前"
            )),
        ])
        .block(crate::tui::ui::card("安全事务执行中", true)),
        areas[0],
    );
    if let Some(event) = latest {
        let gauge_area = ratatui::layout::Rect {
            x: areas[0].x.saturating_add(2),
            y: areas[0].y.saturating_add(3),
            width: areas[0].width.saturating_sub(4),
            height: 1,
        };
        frame.render_widget(
            ratatui::widgets::Gauge::default()
                .ratio(event.current as f64 / event.total as f64)
                .gauge_style(accent()),
            gauge_area,
        );
    }
    let middle = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(areas[1]);
    frame.render_widget(
        Paragraph::new(format!("当前阶段  {phase}")).block(crate::tui::ui::card("当前阶段", false)),
        middle[0],
    );
    let mut step_lines = vec![Line::from(format!("当前步骤  {step}"))];
    if let Some(work) = latest.and_then(|event| event.work) {
        let percent = work.current.saturating_mul(100) / work.total.max(1);
        step_lines.push(Line::from(format!(
            "扇区活动  {}  {}/{} · {}%",
            transaction_activity_label(work.phase),
            work.current,
            work.total,
            percent
        )));
    }
    frame.render_widget(
        Paragraph::new(step_lines).block(crate::tui::ui::card("当前任务", false)),
        middle[1],
    );
    let mut log_lines = Vec::new();
    if let Some(run) = run {
        let viewport = state
            .provision()
            .pane_focus
            .viewport(crate::tui::pane::PaneId::ProvisionRunLog);
        let available = usize::from(areas[2].height.saturating_sub(2));
        let offset = if viewport.selected.is_none() {
            run.log.len().saturating_sub(available)
        } else {
            viewport.scroll_y.offset
        };
        log_lines.extend(run.log.iter().skip(offset).take(available).map(|event| {
            let detail = match event.step {
                crate::application::progress::Step::PartitionFormat(role) => {
                    format!(" {}", role.label())
                }
                _ => String::new(),
            };
            let work = event
                .work
                .map(|work| {
                    let percent = work.current.saturating_mul(100) / work.total.max(1);
                    format!(
                        "  {} {}/{} · {}%",
                        transaction_activity_label(work.phase),
                        work.current,
                        work.total,
                        percent
                    )
                })
                .unwrap_or_default();
            let message = event
                .detail
                .as_deref()
                .map(|detail| format!("  {detail}"))
                .unwrap_or_default();
            Line::from(safe(&format!(
                "[{}/{}] {}  {}{}{}{}",
                event.current,
                event.total,
                event.phase.label(),
                event.step.label(),
                detail,
                work,
                message
            )))
        }));
    }
    if log_lines.is_empty() {
        log_lines.push(Line::from("等待进度事件"));
    }
    frame.render_widget(
        Paragraph::new(log_lines).block(crate::tui::ui::card(
            "运行日志 · j/k 滚动 · G 跟随末尾",
            true,
        )),
        areas[2],
    );
    frame.render_widget(
        Paragraph::new("q / Esc / Ctrl-C 退出请求只在安全检查点生效；介质事务继续受保护。")
            .block(crate::tui::ui::card("安全提示", false)),
        areas[3],
    );
}

fn transaction_activity_label(
    phase: crate::application::progress::TransactionActivityPhase,
) -> &'static str {
    use crate::application::progress::TransactionActivityPhase;
    match phase {
        TransactionActivityPhase::Mirror => "镜像",
        TransactionActivityPhase::Write => "写入",
        TransactionActivityPhase::Readback => "读回",
        TransactionActivityPhase::RollbackWrite => "回滚写入",
        TransactionActivityPhase::RollbackReadback => "回滚读回",
        TransactionActivityPhase::FormatWrite => "格式化写入",
        TransactionActivityPhase::FormatReadback => "格式化读回",
    }
}
