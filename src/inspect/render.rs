use super::*;
use crate::application::inspect_text::{paint, style_name};
use std::collections::BTreeSet;

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
