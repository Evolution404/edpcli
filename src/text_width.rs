//! UI-neutral display width and padding primitives.

pub(crate) fn char_width(c: char) -> usize {
    let u = c as u32;
    // 零宽: 控制字符/组合附标/变体选择符
    if u < 0x20
        || u == 0x7F
        || (0x0300..=0x036F).contains(&u)
        || (0xFE00..=0xFE0F).contains(&u)
        || u == 0x200B
    {
        return 0;
    }
    // 东亚宽: 谚文 Jamo / CJK 部首与符号(含全角标点) / 假名 / CJK 兼容 /
    // 扩展A / 统一表意 / 彝文 / 谚文音节 / 兼容表意 / 兼容形式 /
    // 全角 ASCII 与符号 / 扩展B+
    if (0x1100..=0x115F).contains(&u)
        || (0x2E80..=0x303E).contains(&u)
        || (0x3041..=0x33FF).contains(&u)
        || (0x3400..=0x4DBF).contains(&u)
        || (0x4E00..=0x9FFF).contains(&u)
        || (0xA000..=0xA4CF).contains(&u)
        || (0xAC00..=0xD7A3).contains(&u)
        || (0xF900..=0xFAFF).contains(&u)
        || (0xFE30..=0xFE4F).contains(&u)
        || (0xFF00..=0xFF60).contains(&u)
        || (0xFFE0..=0xFFE6).contains(&u)
        || (0x20000..=0x3FFFD).contains(&u)
    {
        return 2;
    }
    1
}

pub fn disp_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// 左对齐右填充到显示宽度 width。
pub fn pad_to(s: &str, width: usize) -> String {
    let w = disp_width(s);
    if w >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - w))
    }
}

/// 右对齐左填充到显示宽度 width。
pub fn pad_left(s: &str, width: usize) -> String {
    let w = disp_width(s);
    if w >= width {
        s.to_string()
    } else {
        format!("{}{}", " ".repeat(width - w), s)
    }
}
