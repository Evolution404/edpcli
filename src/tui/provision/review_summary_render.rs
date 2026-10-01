use super::review_render_style::action_style;
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
    let mut lines = overall_lines(view);
    if let Some(region) = view.regions.get(selected) {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("当前区域  ", muted()),
            Span::styled(safe(&region.label), action_style(region.action)),
        ]));
        lines.push(Line::from(format!("动作      {}", region.action.label())));
        lines.push(Line::from(format!(
            "数据      {}",
            region.data_effect.label()
        )));
        lines.push(Line::from(format!(
            "密码      {}",
            region.password_effect.label()
        )));
        lines.push(Line::from(format!(
            "文件系统  {}",
            region.filesystem_effect.label()
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("原因", muted())));
        lines.push(Line::from(safe(&region.reason_summary)));
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

    frame.render_widget(
        Paragraph::new(lines)
            .block(crate::tui::ui::card("执行摘要", focused))
            .wrap(Wrap { trim: true }),
        area,
    );
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
    if view.overall.migrated_regions > 0 {
        lines.push(Line::from(vec![
            Span::styled("迁移      ", muted()),
            Span::styled(
                format!("{} 个区域迁移数据", view.overall.migrated_regions),
                accent(),
            ),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled("密码      ", muted()),
        Span::styled(
            if view.overall.password_changed_regions == 0 {
                "✓ 无密码变更".into()
            } else {
                format!("{} 个密码域变化", view.overall.password_changed_regions)
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
    lines
}
