use crate::application::progress::{
    OperationRunState, ProgressEvent, TransactionActivityPhase, Unit,
};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Gauge, Paragraph, Wrap},
    Frame,
};

pub(super) fn unit_label(unit: Unit) -> &'static str {
    match unit {
        Unit::Steps => "step",
        Unit::Sectors => "sector",
        Unit::Bytes => "byte",
    }
}

pub(super) fn activity_description(activity: TransactionActivityPhase) -> &'static str {
    use TransactionActivityPhase as Activity;
    match activity {
        Activity::Mirror => "正在准备镜像数据",
        Activity::Write => "正在写入目标扇区",
        Activity::Readback => "正在读回并校验写入结果",
        Activity::RollbackWrite => "正在执行回滚写入",
        Activity::RollbackReadback => "正在校验回滚结果",
        Activity::FormatWrite => "正在写入文件系统结构",
        Activity::FormatReadback => "正在校验文件系统写入",
    }
}

pub(super) fn phase_label(event: &ProgressEvent) -> String {
    match event.stage {
        Some(stage) => format!(
            "{}/{} · {}",
            stage.current,
            stage.total,
            event.phase.label()
        ),
        None => event.phase.label().to_string(),
    }
}

pub(super) fn draw_current_status(frame: &mut Frame, area: Rect, run: &OperationRunState) {
    let theme = crate::tui::theme::current();
    let block = crate::tui::ui::card("当前任务", true);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let event = run.latest.as_ref();
    let work = event.and_then(|event| event.work);
    let text_height = inner.height.saturating_sub(u16::from(work.is_some()));
    let text_area = Rect::new(inner.x, inner.y, inner.width, text_height);
    let mut lines = Vec::new();
    if let Some(event) = event {
        lines.push(Line::from(format!(
            "阶段  {}    目标  {}",
            phase_label(event),
            run.target
        )));
        lines.push(Line::from(Span::styled(
            format!("当前步骤  {}", event.step.label()),
            theme.secondary_accent().add_modifier(Modifier::BOLD),
        )));
        let description = work
            .and_then(|work| work.activity)
            .map(activity_description)
            .or(event.detail.as_deref())
            .unwrap_or("正在等待下一项任务状态");
        lines.push(Line::from(format!("动态描述  {description}")));
        let last_activity = std::time::Instant::now()
            .saturating_duration_since(run.last_activity_at)
            .as_secs();
        lines.push(Line::from(format!("最近活动  {last_activity}s 前")));
    } else {
        lines.push(Line::from(format!(
            "阶段  等待开始    目标  {}",
            run.target
        )));
        lines.push(Line::from("当前步骤  等待第一个进度事件"));
        lines.push(Line::from("动态描述  正在初始化任务状态"));
        lines.push(Line::from("最近活动  0s 前"));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), text_area);

    if let Some(work) = work {
        let work_area = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
        let label = format!(
            "工作进度  {} / {} {} · {}%",
            work.current,
            work.total,
            unit_label(work.unit),
            work.percent()
        );
        frame.render_widget(
            Gauge::default()
                .gauge_style(theme.secondary_accent())
                .ratio(work.ratio())
                .label(Span::styled(label, theme.progress_label())),
            work_area,
        );
    }
}
