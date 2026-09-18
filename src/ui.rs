//! 终端 UI 原语(零依赖): ANSI 语义色 + CJK 显示宽度对齐。
//!
//! 色彩规则: stdout 为终端且未设 NO_COLOR 时开启; 管道/重定向自动全关,
//! 输出保持纯文本(grep/脚本友好)。测试可强制开关。
//! 对齐规则: 中文占 2 列, Rust 按字符数填充会错位 — 所有混排列对齐必须
//! 经 disp_width/pad_to/pad_left; 着色须在填充之后(ANSI 码会破坏宽度计算)。

use std::cell::Cell;
use std::io::IsTerminal;

thread_local! {
    // 测试会并行运行；颜色强制开关必须线程隔离，避免一个测试把另一个测试的
    // render 输出从中途切成有色/无色。生产默认始终为 -1(自动检测终端)。
    static OVERRIDE: Cell<i8> = const { Cell::new(-1) }; // -1=自动 0=关 1=开
}

pub fn enabled() -> bool {
    match OVERRIDE.with(Cell::get) {
        0 => false,
        1 => true,
        _ => std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
    }
}

pub fn set_enabled_for_tests(v: bool) {
    OVERRIDE.with(|value| value.set(if v { 1 } else { 0 }));
}

pub fn reset_enabled_for_tests() {
    OVERRIDE.with(|value| value.set(-1));
}

fn wrap(code: &str, s: &str) -> String {
    if enabled() {
        format!("\x1b[{}m{}\x1b[0m", code, s)
    } else {
        s.to_string()
    }
}

pub fn bold(s: &str) -> String {
    wrap("1", s)
}
pub fn bold_cyan(s: &str) -> String {
    wrap("1;36", s)
}
pub fn dim(s: &str) -> String {
    wrap("2", s)
}
pub fn red(s: &str) -> String {
    wrap("31", s)
}
pub fn green(s: &str) -> String {
    wrap("32", s)
}
pub fn yellow(s: &str) -> String {
    wrap("33", s)
}
pub fn cyan(s: &str) -> String {
    wrap("36", s)
}
pub fn magenta(s: &str) -> String {
    wrap("35", s)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Bold,
    BoldCyan,
    Dim,
    Red,
    Green,
    Yellow,
    Cyan,
    Magenta,
}

impl Tone {
    fn paint(self, s: &str) -> String {
        match self {
            Self::Plain => s.to_string(),
            Self::Bold => bold(s),
            Self::BoldCyan => bold_cyan(s),
            Self::Dim => dim(s),
            Self::Red => red(s),
            Self::Green => green(s),
            Self::Yellow => yellow(s),
            Self::Cyan => cyan(s),
            Self::Magenta => magenta(s),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCell {
    pub text: String,
    pub align: Align,
    pub tone: Tone,
}

impl TableCell {
    pub fn left(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            align: Align::Left,
            tone,
        }
    }

    pub fn right(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            align: Align::Right,
            tone,
        }
    }
}

/// 渲染紧凑终端表格。列宽按未着色文本的可见宽度计算，填充完成后再上色，
/// 因此 ANSI 与中英文混排都不会破坏列对齐。
pub fn render_table(headers: &[&str], rows: &[Vec<TableCell>]) -> String {
    let cols = headers
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if cols == 0 {
        return String::new();
    }
    let mut widths = vec![0usize; cols];
    for (idx, header) in headers.iter().enumerate() {
        widths[idx] = widths[idx].max(disp_width(header));
    }
    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            widths[idx] = widths[idx].max(disp_width(&cell.text));
        }
    }

    let mut out = String::new();
    if !headers.is_empty() {
        out.push_str("  ");
        for (idx, width) in widths.iter().enumerate() {
            if idx > 0 {
                out.push_str("  ");
            }
            let header = headers.get(idx).copied().unwrap_or("");
            out.push_str(&bold_cyan(&pad_to(header, *width)));
        }
        out.push('\n');
        out.push_str("  ");
        for (idx, width) in widths.iter().enumerate() {
            if idx > 0 {
                out.push_str("  ");
            }
            out.push_str(&dim(&"─".repeat(*width)));
        }
        out.push('\n');
    }

    for row in rows {
        out.push_str("  ");
        for (idx, width) in widths.iter().enumerate() {
            if idx > 0 {
                out.push_str("  ");
            }
            let cell = row.get(idx);
            let raw = cell.map(|c| c.text.as_str()).unwrap_or("");
            let padded = match cell.map(|c| c.align).unwrap_or(Align::Left) {
                Align::Left => pad_to(raw, *width),
                Align::Right => pad_left(raw, *width),
            };
            let painted = cell.map(|c| c.tone).unwrap_or(Tone::Plain).paint(&padded);
            out.push_str(&painted);
        }
        out.push('\n');
    }
    out
}

// ══════════════════════════════════════════════════════════════════
// 显示宽度(East Asian Width 简化版: CJK=2, 零宽=0, 其余=1)
// ══════════════════════════════════════════════════════════════════
fn char_width(c: char) -> usize {
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

/// 超宽时中间 … 截断(保首尾, 按显示宽度计)。
pub fn truncate_mid(s: &str, max: usize) -> String {
    if disp_width(s) <= max || max < 3 {
        return s.to_string();
    }
    let keep = max - 1; // … 记 1 列
    let fb = keep / 2;
    let bb = keep - fb;
    let mut front = String::new();
    let mut fw = 0;
    for c in s.chars() {
        let cw = char_width(c);
        if fw + cw > fb {
            break;
        }
        front.push(c);
        fw += cw;
    }
    let mut back = String::new();
    let mut bw = 0;
    for c in s.chars().rev() {
        let cw = char_width(c);
        if bw + cw > bb {
            break;
        }
        back.insert(0, c);
        bw += cw;
    }
    format!("{}…{}", front, back)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_cjk_is_two() {
        assert_eq!(disp_width("abc"), 3);
        assert_eq!(disp_width("外接盘"), 6);
        assert_eq!(disp_width("cems盘"), 4 + 2); // "cems" 4 列 + 全角"盘" 2 列
        assert_eq!(disp_width("disk26  125.83GB"), 6 + 2 + 8);
        assert_eq!(disp_width("→"), 1); // 箭头按窄字符(非对齐列使用)
        assert_eq!(disp_width("·"), 1);
    }

    #[test]
    fn pad_aligns_mixed_scripts() {
        // 中文 6 列 + 2 空格 = 与 ASCII 8 列对齐
        assert_eq!(pad_to("外接盘", 8), "外接盘  ");
        assert_eq!(pad_to("abc", 5), "abc  ");
        assert_eq!(pad_left("59.75GB", 8), " 59.75GB"); // 7 列补 1
        assert_eq!(pad_left("1.34GB", 8), "  1.34GB");
        assert_eq!(pad_to("溢出宽度", 4), "溢出宽度"); // 不截断, 由调用方处理
    }

    #[test]
    fn table_aligns_cjk_numbers_and_colored_cells() {
        set_enabled_for_tests(false);
        let rows = vec![
            vec![
                TableCell::left("LBA7", Tone::Green),
                TableCell::left("Entry[0]", Tone::Cyan),
                TableCell::left("Boot (1)", Tone::Yellow),
                TableCell::right("63", Tone::Green),
                TableCell::right("10.45 MB", Tone::Magenta),
            ],
            vec![
                TableCell::left("LBA12", Tone::Green),
                TableCell::left("Entry[2]", Tone::Cyan),
                TableCell::left("加密区", Tone::Yellow),
                TableCell::right("243116060", Tone::Green),
                TableCell::right("1.34 GB", Tone::Magenta),
            ],
        ];
        let out = render_table(&["来源", "条目", "类型", "起始LBA", "大小"], &rows);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(disp_width(lines[0]), disp_width(lines[1]));
        assert_eq!(disp_width(lines[2]), disp_width(lines[3]));

        set_enabled_for_tests(true);
        let colored = render_table(&["来源", "条目", "类型", "起始LBA", "大小"], &rows);
        assert!(colored.contains("\x1b[32m"));
        assert!(colored.contains("\x1b[35m"));
        reset_enabled_for_tests();
    }

    #[test]
    fn truncate_keeps_ends() {
        assert_eq!(truncate_mid("abcdefghij", 7), "abc…hij"); // 首尾各 3 + … = 7 列
        assert_eq!(truncate_mid("短", 7), "短");
        let long = "/Users/zhangyuxi/.edpcli-backup/disk4_245760000.bin";
        let t = truncate_mid(long, 20);
        assert!(t.starts_with("/Users") && t.contains('…') && t.ends_with(".bin"));
        assert!(disp_width(&t) <= 20);
    }

    #[test]
    fn color_toggle() {
        set_enabled_for_tests(true);
        assert!(red("x").contains("\x1b[31m"));
        assert!(bold("y").starts_with("\x1b[1m"));
        assert_eq!(green("z"), "\x1b[32mz\x1b[0m");
        set_enabled_for_tests(false);
        assert_eq!(red("x"), "x");
        assert_eq!(bold_cyan("c"), "c");
        reset_enabled_for_tests();
        // 自动模式: 测试环境非终端且无 NO_COLOR 时由环境决定, 只验证不 panic
        let _ = enabled();
    }
}
