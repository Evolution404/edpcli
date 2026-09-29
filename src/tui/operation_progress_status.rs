use crate::application::progress::{ProgressEvent, TransactionActivityPhase, Unit};
use ratatui::{
    layout::Rect,
    text::Line,
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

pub(super) fn activity_label(activity: TransactionActivityPhase) -> &'static str {
    use TransactionActivityPhase as Activity;
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

pub(super) fn draw_current_status(frame: &mut Frame, area: Rect, event: Option<&ProgressEvent>) {
    let theme = crate::tui::theme::current();
    let block = crate::tui::ui::card("当前状态", true);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let work = event.and_then(|event| event.work);
    let text_height = inner.height.saturating_sub(u16::from(work.is_some()));
    let text_area = Rect::new(inner.x, inner.y, inner.width, text_height);
    let mut lines = Vec::new();
    if let Some(event) = event {
        lines.push(Line::from(format!(
            "当前阶段  {}    当前步骤  {}",
            phase_label(event),
            event.step.label()
        )));
        let mut secondary = Vec::new();
        if let Some(activity) = work.and_then(|work| work.activity) {
            secondary.push(format!("扇区活动  {}", activity_label(activity)));
        }
        if let Some(detail) = event.detail.as_deref() {
            secondary.push(detail.to_string());
        }
        if !secondary.is_empty() {
            lines.push(Line::from(secondary.join(" · ")));
        }
    } else {
        lines.push(Line::from("等待进度事件"));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), text_area);

    if let Some(work) = work {
        let work_area = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
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
}
