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

pub(super) fn paired_region_row(
    state: &AppState,
    section: ProvisionFieldSection,
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
    let left_index = indexes[0];
    let right_index = indexes[1];
    let left_id = state.provision_field_id(left_index)?;
    let right_id = state.provision_field_id(right_index)?;

    let (
        region,
        left_label,
        right_label,
        left_value,
        left_secret,
        right_value,
        right_secret,
        status,
        status_style,
    ) = match (section, left_id, right_id) {
        (
            ProvisionFieldSection::PasswordDomain,
            ProvisionFieldId::SourcePassword(domain),
            ProvisionFieldId::TargetPassword(target_domain),
        ) if domain == target_domain => {
            let region = match domain {
                crate::provision::KeyDomainRole::Share if provision.kind.mode() == Some(1) => {
                    "二合一区"
                }
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
            let (_, source_value, source_secret) = &fields[left_index];
            let (source_value, source_secret) = if source_not_applicable {
                ("— 不涉及", false)
            } else {
                (*source_value, *source_secret)
            };
            let (target_value, target_secret) =
                if state.provision_target_password_is_passthrough(domain) {
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
            (
                region,
                "原密码".to_string(),
                "新密码".to_string(),
                source_value,
                source_secret,
                target_value,
                target_secret,
                status,
                status_style,
            )
        }
        (
            ProvisionFieldSection::PartitionLayout,
            ProvisionFieldId::StartLba(role),
            ProvisionFieldId::Capacity(capacity_role),
        ) if role == capacity_role => {
            let region = crate::disk_layout::DiskRegionKind::from_partition_role(role).label();
            let (left_field_label, left_value, left_secret) = &fields[left_index];
            let (right_field_label, right_value, right_secret) = &fields[right_index];
            let compact_pair_label = |label: &str| {
                let compact = compact_field_label(label);
                compact
                    .strip_prefix(region)
                    .unwrap_or(compact.as_ref())
                    .trim_start()
                    .to_string()
            };
            (
                region,
                compact_pair_label(left_field_label),
                compact_pair_label(right_field_label),
                *left_value,
                *left_secret,
                *right_value,
                *right_secret,
                String::new(),
                muted(),
            )
        }
        _ => return None,
    };

    const REGION_WIDTH: usize = 8;
    const PASSWORD_STATUS_SLOT_WIDTH: usize = 2;
    let separator = " │ ";
    let separator_width = crate::ui::disp_width(separator);
    let status_slot_width = if section == ProvisionFieldSection::PasswordDomain {
        PASSWORD_STATUS_SLOT_WIDTH
    } else {
        0
    };
    let left_label_width = metrics.0.max(crate::ui::disp_width(&left_label));
    let right_label_width = metrics.1.max(crate::ui::disp_width(&right_label));
    let adjusted_metrics = (
        REGION_WIDTH + left_label_width,
        right_label_width,
        metrics.2.saturating_add(status_slot_width),
        metrics.3,
    );
    let (left_width, right_width) =
        two_column_widths(content_width, separator_width, adjusted_metrics);

    let left_active = provision.field_selected == left_index;
    let right_active = provision.field_selected == right_index;
    let left_editing = left_active
        && parameters_focused
        && state.input_mode() == InputMode::Insert
        && state.provision_selected_field_is_editable();
    let right_editing = right_active
        && parameters_focused
        && state.input_mode() == InputMode::Insert
        && state.provision_selected_field_is_editable();

    let left_prefix_width = 2 + REGION_WIDTH + left_label_width + 1;
    let left_value_width = left_width
        .saturating_sub(left_prefix_width + status_slot_width)
        .max(4);
    let right_prefix_width = 2 + right_label_width + 1;
    let right_value_width = right_width.saturating_sub(right_prefix_width).max(4);

    let left_shown = if left_editing {
        input_value_window(
            left_value,
            state.provision_field_cursor(),
            left_value_width.saturating_sub(2),
            left_secret,
        )
    } else {
        fit_display_width(&masked_value(left_value, left_secret), left_value_width)
            .trim_end()
            .to_string()
    };
    let right_shown = if right_editing {
        input_value_window(
            right_value,
            state.provision_field_cursor(),
            right_value_width.saturating_sub(2),
            right_secret,
        )
    } else {
        fit_display_width(&masked_value(right_value, right_secret), right_value_width)
            .trim_end()
            .to_string()
    };

    let mut left = vec![
        Span::styled(
            if left_active && parameters_focused {
                "▌ "
            } else {
                "  "
            },
            if left_active && parameters_focused {
                selection_marker()
            } else {
                Style::default()
            },
        ),
        Span::styled(crate::ui::pad_to(region, REGION_WIDTH), secondary()),
        Span::styled(
            format!("{} ", crate::ui::pad_to(&left_label, left_label_width)),
            muted(),
        ),
    ];
    if left_editing {
        left.push(Span::styled("[", accent()));
        left.push(Span::styled(left_shown, input_focused()));
        left.push(Span::styled("]", accent()));
    } else {
        left.push(Span::styled(
            left_shown,
            if left_active && parameters_focused {
                accent().add_modifier(Modifier::BOLD)
            } else {
                crate::tui::theme::current().secondary_text()
            },
        ));
    }
    if !status.is_empty() {
        left.push(Span::raw(" "));
        left.push(Span::styled(status, status_style));
    }
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
        if right_active && parameters_focused {
            "▌ "
        } else {
            "  "
        },
        if right_active && parameters_focused {
            selection_marker()
        } else {
            Style::default()
        },
    ));
    spans.push(Span::styled(
        format!("{} ", crate::ui::pad_to(&right_label, right_label_width)),
        muted(),
    ));
    if right_editing {
        spans.push(Span::styled("[", accent()));
        spans.push(Span::styled(right_shown, input_focused()));
        spans.push(Span::styled("]", accent()));
    } else {
        spans.push(Span::styled(
            right_shown,
            if right_active && parameters_focused {
                accent().add_modifier(Modifier::BOLD)
            } else {
                crate::tui::theme::current().secondary_text()
            },
        ));
    }
    Some(Line::from(spans))
}

pub(super) fn section_metrics(
    state: &AppState,
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
            let display_label = if indexes.len() == 2
                && *section == ProvisionFieldSection::PartitionLayout
            {
                match state.provision_field_id(index) {
                    Some(ProvisionFieldId::StartLba(role) | ProvisionFieldId::Capacity(role)) => {
                        let region =
                            crate::disk_layout::DiskRegionKind::from_partition_role(role).label();
                        display_label
                            .strip_prefix(region)
                            .unwrap_or(display_label.as_ref())
                            .trim_start()
                            .into()
                    }
                    _ => display_label,
                }
            } else {
                display_label
            };
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
