//! Shared text export of Inspect fields.

use crate::inspect_adapter::SectorView;

pub fn render_fields_plain(view: &SectorView) -> String {
    if view.fields.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("  结构化字段:\n");
    let mut current_group: Option<&str> = None;
    for f in &view.fields {
        let group = f.group.as_deref();
        if group != current_group {
            if current_group.is_some() && group.is_some() {
                out.push('\n');
            }
            current_group = group;
            if let Some(group_name) = group {
                let (start, end) = view
                    .fields
                    .iter()
                    .filter(|candidate| candidate.group.as_deref() == Some(group_name))
                    .fold((usize::MAX, 0usize), |(min_start, max_end), candidate| {
                        (min_start.min(candidate.start), max_end.max(candidate.end))
                    });
                let range = format!("+0x{start:03X}..0x{:03X}", end.saturating_sub(1));
                out.push_str(&format!("    {}  {}\n", group_name, range));
            }
        }
        let range = if f.end == f.start + 1 {
            format!("+0x{:03X}", f.start)
        } else {
            format!("+0x{:03X}..0x{:03X}", f.start, f.end.saturating_sub(1))
        };
        if !f.value.is_empty() {
            let indent = if group.is_some() { "      " } else { "    " };
            let prefix = format!(
                "{}{}  {}  ",
                indent,
                crate::text_width::pad_to(&range, 18),
                crate::text_width::pad_to(&f.label, 18)
            );
            let chunks = wrap_value(&f.value, 64);
            for (idx, chunk) in chunks.iter().enumerate() {
                if idx == 0 {
                    out.push_str(&prefix);
                } else {
                    out.push_str(&" ".repeat(6 + 18 + 2 + 18 + 2));
                }
                out.push_str(chunk);
                out.push('\n');
            }
        }
        if !f.children.is_empty() {
            let child_indent = if f.label.is_empty() {
                "      "
            } else {
                "        "
            };
            if !f.label.is_empty() {
                out.push_str(&format!(
                    "      {}  {}\n",
                    crate::text_width::pad_to(&f.label, 12),
                    range
                ));
            }
            let mut empty_labels = Vec::new();
            for child in &f.children {
                if child.value == "<空>" {
                    empty_labels.push(child.label.as_str());
                    continue;
                }
                let child_value = wrap_value(&child.value, 72);
                for (idx, chunk) in child_value.iter().enumerate() {
                    if idx == 0 {
                        out.push_str(&format!(
                            "{}{}  {}\n",
                            child_indent,
                            crate::text_width::pad_to(&child.label, 12),
                            chunk
                        ));
                    } else {
                        let rendered = chunk;
                        out.push_str(&format!(
                            "{}{}  {}\n",
                            child_indent,
                            " ".repeat(12),
                            rendered
                        ));
                    }
                }
            }
            if !empty_labels.is_empty() {
                out.push_str(&format!(
                    "{}{}  {}\n",
                    child_indent,
                    crate::text_width::pad_to("空字段", 12),
                    empty_labels.join(" · ")
                ));
            }
        }
    }
    out
}

fn wrap_value(value: &str, max_chars: usize) -> Vec<String> {
    if value.chars().count() <= max_chars {
        return vec![value.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for token in value.split_whitespace() {
        let token_len = token.chars().count();
        if current_len > 0 && current_len + 1 + token_len > max_chars {
            lines.push(current);
            current = String::new();
            current_len = 0;
        }
        if !current.is_empty() {
            current.push(' ');
            current_len += 1;
        }
        if token_len <= max_chars {
            current.push_str(token);
            current_len += token_len;
            continue;
        }
        for ch in token.chars() {
            if current_len == max_chars {
                lines.push(current);
                current = String::new();
                current_len = 0;
            }
            current.push(ch);
            current_len += 1;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}
