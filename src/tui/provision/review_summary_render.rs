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
    if view.overall.cleared_regions == 0 {
        lines.push(Line::from(vec![
            Span::styled("数据      ", muted()),
            Span::styled("✓ 无区域清空", success()),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("数据      ", muted()),
            Span::styled(
                format!("⚠ {} 个区域数据将清空", view.overall.cleared_regions),
                warning(),
            ),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled("密码      ", muted()),
        Span::styled(
            if view.overall.password_changed_regions == 0 {
                "✓ 无密码变更".into()
            } else {
                format!("↻ {} 个密码域变化", view.overall.password_changed_regions)
            },
            if view.overall.password_changed_regions == 0 {
                success()
            } else {
                accent()
            },
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("文件系统  ", muted()),
        Span::styled(
            if view.overall.reformatted_regions == 0 {
                "✓ 不新建/格式化".into()
            } else {
                format!("⚠ {} 个区域新建/格式化", view.overall.reformatted_regions)
            },
            if view.overall.reformatted_regions == 0 {
                success()
            } else {
                warning()
            },
        ),
    ]));
    if let Some(note) = view.geometry_note.as_deref() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("布局说明  ", secondary()),
            Span::styled(safe(note), secondary()),
        ]));
    }
    lines
}
