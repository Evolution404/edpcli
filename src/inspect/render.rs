use super::*;

fn style_name(style: FieldStyle) -> &'static str {
    match style {
        FieldStyle::Magic => "魔数/签名",
        FieldStyle::Text => "文本",
        FieldStyle::Identity => "身份/密钥",
        FieldStyle::Address => "地址/LBA",
        FieldStyle::Size => "大小",
        FieldStyle::Flag => "类型/标志",
        FieldStyle::Checksum => "校验",
    }
}

fn paint(style: FieldStyle, text: &str, bad: bool) -> String {
    match style {
        FieldStyle::Magic => crate::ui::bold_cyan(text),
        FieldStyle::Text => crate::ui::cyan(text),
        FieldStyle::Identity => crate::ui::yellow(text),
        FieldStyle::Address => crate::ui::green(text),
        FieldStyle::Size => crate::ui::magenta(text),
        FieldStyle::Flag => crate::ui::yellow(text),
        FieldStyle::Checksum if bad => crate::ui::red(text),
        FieldStyle::Checksum => crate::ui::green(text),
    }
}

fn byte_style(fields: &[SectorField], idx: usize) -> Option<(FieldStyle, bool)> {
    fields
        .iter()
        .find(|f| idx >= f.start && idx < f.end)
        .map(|f| (f.style, f.value.contains('✗')))
}

/// 16B/行字段感知 hex。`raw_mode=true` 时不对加密态字节套用解密字段颜色，避免误导。
pub fn render_hex(view: &SectorView, raw_mode: bool) -> String {
    let data = if raw_mode { &view.raw } else { &view.decoded };
    let fields: &[SectorField] = if raw_mode { &[] } else { &view.fields };
    let mut out = String::new();
    out.push_str(&format!(
        "LBA{} {} hex ({}B)\n",
        view.lba,
        if raw_mode { "RAW" } else { "解码" },
        data.len()
    ));
    for (line_no, line) in data.chunks(16).enumerate() {
        let base = line_no * 16;
        out.push_str(&format!("  +0x{base:03X}: "));
        for i in 0..16 {
            if i == 8 {
                out.push(' ');
            }
            if let Some(&b) = line.get(i) {
                let token = format!("{b:02X}");
                if let Some((style, bad)) = byte_style(fields, base + i) {
                    out.push_str(&paint(style, &token, bad));
                } else {
                    out.push_str(&token);
                }
            } else {
                out.push_str("  ");
            }
            out.push(' ');
        }
        out.push(' ');
        for &b in line {
            out.push(if (0x20..=0x7e).contains(&b) {
                b as char
            } else {
                '.'
            });
        }
        out.push('\n');
    }
    if !raw_mode && !view.fields.is_empty() {
        let styles: BTreeSet<FieldStyle> = view.fields.iter().map(|f| f.style).collect();
        out.push_str("  字段图例: ");
        let mut first = true;
        for s in styles {
            if !first {
                out.push_str(" · ");
            }
            first = false;
            out.push_str(&paint(s, style_name(s), false));
        }
        out.push('\n');
    }
    out
}

pub fn render_fields(view: &SectorView) -> String {
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
                out.push_str(&format!(
                    "    {}  {}\n",
                    crate::ui::bold_cyan(group_name),
                    crate::ui::dim(&range)
                ));
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
                crate::ui::pad_to(&range, 18),
                crate::ui::pad_to(&f.label, 18)
            );
            let chunks = wrap_value(&f.value, 64);
            for (idx, chunk) in chunks.iter().enumerate() {
                if idx == 0 {
                    out.push_str(&prefix);
                } else {
                    out.push_str(&" ".repeat(6 + 18 + 2 + 18 + 2));
                }
                if chunk == "<空>" {
                    out.push_str(&crate::ui::dim(chunk));
                } else {
                    out.push_str(&paint(f.style, chunk, f.value.contains('✗')));
                }
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
                    crate::ui::pad_to(&f.label, 12),
                    crate::ui::dim(&range)
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
                            crate::ui::pad_to(&child.label, 12),
                            if chunk == "<空>" {
                                crate::ui::dim(chunk)
                            } else {
                                paint(f.style, chunk, chunk.contains('✗'))
                            }
                        ));
                    } else {
                        let rendered = if chunk == "<空>" {
                            crate::ui::dim(chunk)
                        } else {
                            paint(f.style, chunk, chunk.contains('✗'))
                        };
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
                    crate::ui::pad_to("空字段", 12),
                    crate::ui::dim(&empty_labels.join(" · "))
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

pub fn overview_line(view: &SectorView) -> String {
    let nz = view.raw.iter().filter(|&&b| b != 0).count();
    let head: String = view
        .raw
        .iter()
        .take(12)
        .map(|&b| {
            if (0x20..=0x7e).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect();
    format!(
        "LBA{:>2}  {:>3}/512  {:<12}  {}",
        view.lba, nz, head, view.method
    )
}
