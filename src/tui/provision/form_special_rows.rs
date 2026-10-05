use super::*;
use crate::tui::state::ProvisionFieldSection;

pub(super) fn single_column_field<'a>(
    state: &AppState,
    index: usize,
    label: &'a str,
    value: &'a str,
    secret: bool,
) -> (std::borrow::Cow<'a, str>, &'a str, bool) {
    use crate::tui::state::ProvisionFieldId;
    use crate::tui::state::SourcePasswordState;
    let Some(id) = state.provision_field_id(index) else {
        return (compact_field_label(label), value, secret);
    };
    let domain = match id {
        ProvisionFieldId::SourcePassword(domain) | ProvisionFieldId::TargetPassword(domain) => {
            domain
        }
        _ => return (compact_field_label(label), value, secret),
    };
    let region = match domain {
        crate::provision::KeyDomainRole::Share if state.provision().kind.mode() == Some(1) => {
            "二合一区"
        }
        crate::provision::KeyDomainRole::Share => "交换区",
        crate::provision::KeyDomainRole::Encrypt => "保密区",
    };
    let status = if matches!(id, ProvisionFieldId::SourcePassword(_)) {
        match state.provision_source_password_state(domain) {
            SourcePasswordState::NotApplicable => {
                return (format!("{region}{label}").into(), "— 不涉及", false)
            }
            SourcePasswordState::Verifying => " ◑",
            SourcePasswordState::VerifiedDefault | SourcePasswordState::VerifiedUser => " ✓",
            SourcePasswordState::Failed => " ✗",
            SourcePasswordState::Unknown => " —",
        }
    } else {
        ""
    };
    let passthrough = matches!(id, ProvisionFieldId::TargetPassword(_))
        && state.provision_target_password_is_passthrough(domain);
    (
        format!("{region}{label}{status}").into(),
        if passthrough { "透传" } else { value },
        secret && !passthrough,
    )
}

pub(super) fn compact_field_label(label: &str) -> std::borrow::Cow<'_, str> {
    match label {
        "初始化密码强制修改" => "首次改密".into(),
        "取消密码复杂性验证" => "取消复杂度".into(),
        "交换区密码最大错误次数" => "交换区错误上限".into(),
        "保密区密码最大错误次数" => "保密区错误上限".into(),
        _ => label
            .strip_suffix("卷标（原样保留）")
            .map(|prefix| format!("{prefix}原卷标").into())
            .unwrap_or_else(|| label.into()),
    }
}

pub(super) fn two_column_widths(
    content_width: usize,
    separator_width: usize,
    metrics: (usize, usize, usize, usize),
) -> (usize, usize) {
    let min_right_width = 2 + metrics.1 + 1 + metrics.3.max(8);
    let desired_left_width = 2 + metrics.0 + 1 + metrics.2.max(8);
    let max_left_width = content_width
        .saturating_sub(separator_width)
        .saturating_sub(min_right_width)
        .max(8);
    let left = desired_left_width.min(max_left_width);
    let right = content_width
        .saturating_sub(left)
        .saturating_sub(separator_width);
    (left, right)
}

pub(super) fn advanced_settings_row(
    state: &AppState,
    index: usize,
    parameters_focused: bool,
) -> Line<'static> {
    let provision = state.provision();
    let active = index == provision.field_selected;
    let focused_active = active && parameters_focused;
    Line::from(vec![
        Span::styled(
            if focused_active { "▌ " } else { "  " },
            if focused_active {
                selection_marker()
            } else {
                Style::default()
            },
        ),
        Span::styled(
            "高级设置",
            if focused_active {
                accent().add_modifier(Modifier::BOLD)
            } else {
                crate::tui::theme::current().secondary_text()
            },
        ),
        Span::raw("  "),
        Span::styled(
            if provision.advanced_identity_open {
                "▾  o 收起"
            } else {
                "▸  o 展开"
            },
            muted(),
        ),
    ])
}

fn masked_value(value: &str, secret: bool) -> String {
    if value.is_empty() {
        "〈空〉".into()
    } else if secret {
        "•".repeat(value.chars().count())
    } else {
        safe(value)
    }
}

pub(super) fn password_domain_row(
    state: &AppState,
    indexes: &[usize],
    fields: &[(String, &str, bool)],
    content_width: usize,
    parameters_focused: bool,
    metrics: (usize, usize, usize, usize),
) -> Option<Line<'static>> {
    if indexes.len() != 2 {
        return None;
    }
    let provision = state.provision();
    let source_index = indexes[0];
    let target_index = indexes[1];
    let domain = match state.provision_field_id(source_index) {
        Some(ProvisionFieldId::SourcePassword(domain)) => domain,
        _ => return None,
    };
    let region = match domain {
        crate::provision::KeyDomainRole::Share if provision.kind.mode() == Some(1) => "二合一区",
        crate::provision::KeyDomainRole::Share => "交换区",
        crate::provision::KeyDomainRole::Encrypt => "保密区",
    };
    let source_not_applicable = state.provision_source_password_not_applicable(domain);
    let verification = match domain {
        crate::provision::KeyDomainRole::Share => provision.share_source_verification,
        crate::provision::KeyDomainRole::Encrypt => provision.encrypt_source_verification,
    };
    let knowledge = match domain {
        crate::provision::KeyDomainRole::Share => provision.form.share_source_knowledge,
        crate::provision::KeyDomainRole::Encrypt => provision.form.encrypt_source_knowledge,
    };
    let (status, status_style) = if source_not_applicable {
        (String::new(), muted())
    } else {
        match verification {
            ProvisionPasswordVerificationState::Verifying => (
                crate::tui::animation::spinner_glyph(state.animation_frame()).to_string(),
                secondary(),
            ),
            ProvisionPasswordVerificationState::Failed => ("✗".into(), danger()),
            ProvisionPasswordVerificationState::Idle
                if knowledge != crate::provision::SourcePasswordKnowledge::Unknown =>
            {
                ("✓".into(), success())
            }
            ProvisionPasswordVerificationState::Idle => ("—".into(), muted()),
        }
    };

    let separator = " │ ";
    let separator_width = crate::ui::disp_width(separator);
    const PASSWORD_VALUE_WIDTH: usize = 10;
    const PASSWORD_STATUS_SLOT_WIDTH: usize = 2;
    let adjusted_metrics = (
        8 + metrics.0,
        metrics.1,
        PASSWORD_VALUE_WIDTH + PASSWORD_STATUS_SLOT_WIDTH,
        PASSWORD_VALUE_WIDTH,
    );
    let (left_width, right_width) =
        two_column_widths(content_width, separator_width, adjusted_metrics);
    let source_active = provision.field_selected == source_index;
    let target_active = provision.field_selected == target_index;
    let source_editing = source_active
        && parameters_focused
        && state.input_mode() == InputMode::Insert
        && state.provision_selected_field_is_editable();
    let target_editing = target_active
        && parameters_focused
        && state.input_mode() == InputMode::Insert
        && state.provision_selected_field_is_editable();

    let (_, field_source_value, field_source_secret) = &fields[source_index];
    let (source_value, source_secret) = if source_not_applicable {
        ("— 不涉及", false)
    } else {
        (*field_source_value, *field_source_secret)
    };
    let (target_value, target_secret) = if state.provision_target_password_is_passthrough(domain) {
        ("透传", false)
    } else {
        match domain {
            crate::provision::KeyDomainRole::Share => {
                (provision.form.share_target_password.as_str(), true)
            }
            crate::provision::KeyDomainRole::Encrypt => {
                (provision.form.encrypt_target_password.as_str(), true)
            }
        }
    };
    let source_prefix_width = 2 + 8 + metrics.0 + 1;
    let source_status_width = 1 + crate::ui::disp_width(&status);
    let source_value_width = left_width
        .saturating_sub(source_prefix_width + source_status_width)
        .max(4);
    let target_prefix_width = 2 + metrics.1 + 1;
    let target_value_width = right_width.saturating_sub(target_prefix_width).max(4);

    let source_shown = if source_editing {
        input_value_window(
            source_value,
            state.provision_field_cursor(),
            source_value_width.saturating_sub(2),
            source_secret,
        )
    } else {
        fit_display_width(
            &masked_value(source_value, source_secret),
            source_value_width,
        )
        .trim_end()
        .to_string()
    };
    let target_shown = if target_editing {
        input_value_window(
            target_value,
            state.provision_field_cursor(),
            target_value_width.saturating_sub(2),
            target_secret,
        )
    } else {
        fit_display_width(
            &masked_value(target_value, target_secret),
            target_value_width,
        )
        .trim_end()
        .to_string()
    };

    let mut left = vec![
        Span::styled(
            if source_active && parameters_focused {
                "▌ "
            } else {
                "  "
            },
            if source_active && parameters_focused {
                selection_marker()
            } else {
                Style::default()
            },
        ),
        Span::styled(crate::ui::pad_to(region, 8), secondary()),
        Span::styled("原密码 ", muted()),
    ];
    if source_editing {
        left.push(Span::styled("[", accent()));
        left.push(Span::styled(source_shown, input_focused()));
        left.push(Span::styled("]", accent()));
    } else {
        left.push(Span::styled(
            source_shown,
            if source_active && parameters_focused {
                accent().add_modifier(Modifier::BOLD)
            } else {
                crate::tui::theme::current().secondary_text()
            },
        ));
    }
    left.push(Span::raw(" "));
    left.push(Span::styled(status, status_style));
    let left_used = left
        .iter()
        .map(|span| crate::ui::disp_width(span.content.as_ref()))
        .sum::<usize>();
    if left_used < left_width {
        left.push(Span::raw(" ".repeat(left_width - left_used)));
    }

    let mut spans = left;
    spans.push(Span::styled(separator, muted()));
    spans.push(Span::styled(
        if target_active && parameters_focused {
            "▌ "
        } else {
            "  "
        },
        if target_active && parameters_focused {
            selection_marker()
        } else {
            Style::default()
        },
    ));
    spans.push(Span::styled("新密码 ", muted()));
    if target_editing {
        spans.push(Span::styled("[", accent()));
        spans.push(Span::styled(target_shown, input_focused()));
        spans.push(Span::styled("]", accent()));
    } else {
        spans.push(Span::styled(
            target_shown,
            if target_active && parameters_focused {
                accent().add_modifier(Modifier::BOLD)
            } else {
                crate::tui::theme::current().secondary_text()
            },
        ));
    }
    Some(Line::from(spans))
}

pub(super) fn section_metrics(
    fields: &[(String, &str, bool)],
    rows: &[(ProvisionFieldSection, Vec<usize>)],
) -> std::collections::HashMap<ProvisionFieldSection, (usize, usize, usize, usize)> {
    const INPUT_EDITING_SLACK: usize = 2;
    let mut section_metrics: std::collections::HashMap<
        ProvisionFieldSection,
        (usize, usize, usize, usize),
    > = std::collections::HashMap::new();
    for (section, indexes) in rows {
        let entry = section_metrics.entry(*section).or_insert((0, 0, 0, 0));
        for (position, index) in indexes.iter().copied().enumerate() {
            let (label, value, secret) = &fields[index];
            let display_label = compact_field_label(label);
            let label_width = crate::ui::disp_width(display_label.as_ref());
            let shown_width = if value.is_empty() {
                crate::ui::disp_width("〈请输入〉")
            } else if *secret {
                value.chars().count()
            } else {
                crate::ui::disp_width(&safe(value))
            }
            .saturating_add(INPUT_EDITING_SLACK)
            .clamp(8, 26);
            if position == 0 {
                entry.0 = entry.0.max(label_width);
                entry.2 = entry.2.max(shown_width);
            } else {
                entry.1 = entry.1.max(label_width);
                entry.3 = entry.3.max(shown_width);
            }
        }
    }

    section_metrics
}
