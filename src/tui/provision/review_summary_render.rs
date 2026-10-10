use super::review_render_style::{action_style, data_style, filesystem_style, password_style};
use super::*;
use crate::tui::state::ProvisionConfirmationViewModel;

pub(super) fn draw_execution_summary(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    view: &ProvisionConfirmationViewModel,
    selected: usize,
    expanded: bool,
    focused: bool,
) {
    let mut lines = Vec::new();
    if let Some(region) = view.regions.get(selected) {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("当前区域  ", muted()),
            Span::styled(safe(&region.label), action_style(region.action)),
        ]));
        lines.push(status_line(
            "处理",
            region.action.label(),
            action_style(region.action),
        ));
        lines.push(status_line(
            "数据",
            region.data_effect.label(),
            data_style(region.data_effect),
        ));
        lines.push(status_line(
            "密码",
            region.password_effect.label(),
            password_style(region.password_effect),
        ));
        lines.push(status_line(
            "文件系统",
            region.filesystem_effect.label(),
            filesystem_style(region.filesystem_effect),
        ));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("原因", muted())));
        lines.push(Line::from(safe(&region.reason_summary)));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("结果", muted())));
        lines.push(Line::from(safe(&region.result_summary())));
        if expanded {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("技术依据", muted())));
            lines.extend(
                region
                    .technical_basis
                    .iter()
                    .map(|basis| Line::from(Span::styled(safe(basis), muted()))),
            );
        }
    }

    let block = crate::tui::ui::card("执行摘要", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width >= 120 {
        let columns = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(inner);
        frame.render_widget(
            Paragraph::new(overall_lines(view)).wrap(Wrap { trim: true }),
            columns[0],
        );
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), columns[1]);
    } else {
        let mut overall = overall_lines(view);
        overall.extend(lines);
        frame.render_widget(Paragraph::new(overall).wrap(Wrap { trim: true }), inner);
    }
}

fn status_line(label: &'static str, value: impl Into<String>, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(crate::ui::pad_to(label, 10), muted()),
        Span::styled(value.into(), style),
    ])
}

fn overall_lines(view: &ProvisionConfirmationViewModel) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled("总体", secondary()))];
    // Names are source-owned; creating multiple target partitions must never
    // multiply the number of source partitions reported as discarded.
    lines.push(source_effect_line(
        "来源数据丢弃", &view.overall.source_discarded, true,
    ));
    lines.push(source_effect_line(
        "来源数据保留", &view.overall.source_retained, false,
    ));
    lines.push(source_effect_line(
        "目标格式化", &view.overall.target_formatted, true,
    ));
    lines.push(source_effect_line(
        "密钥操作", &view.overall.key_changed, false,
    ));
    if let Some(note) = view.geometry_note.as_deref() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("布局说明  ", secondary()),
            Span::styled(safe(note), secondary()),
        ]));
    }
    lines
}

fn source_effect_line(
    label: &'static str,
    partitions: &[String],
    warning_when_present: bool,
) -> Line<'static> {
    let has_partitions = !partitions.is_empty();
    let display = if has_partitions {
        partitions.join("、")
    } else {
        "无".to_string()
    };
    let style = if has_partitions && warning_when_present {
        warning()
    } else if has_partitions {
        success()
    } else {
        muted()
    };
    Line::from(vec![
        Span::styled(crate::ui::pad_to(label, 16), muted()),
        Span::styled(safe(&display), style),
    ])
}
