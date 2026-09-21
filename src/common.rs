//! 公共常量与工具: 扇区大小、容量显示、Python 兼容舍入、退出码契约。

pub const SECTOR: usize = 512;
pub const METADATA_SECTOR_COUNT: usize = 13;
pub const METADATA_LAST_LBA: u32 = 12;
pub const METADATA_IMAGE_LEN: usize = METADATA_SECTOR_COUNT * SECTOR;
pub const LEGACY_METADATA_IMAGE_LEN: usize = (METADATA_SECTOR_COUNT + 1) * SECTOR;

/// 退出码契约(脚本可区分失败类型; 原 Python 版一律 exit 1):
pub const EXIT_OK: i32 = 0; // 成功(含 dry-run/预览)
pub const EXIT_IO: i32 = 1; // 运行时 IO 错误
pub const EXIT_USAGE: i32 = 2; // 用法错误(未知旗标/缺参数/参数非数字)
pub const EXIT_TARGET: i32 = 3; // 目标不可用(非cems/识别失败/size越界/系统盘)
pub const EXIT_ALREADY_NOPWD: i32 = 4; // 已免密盘拒绝重复写入(需 --force)
pub const EXIT_BACKUP: i32 = 5; // 备份问题(无匹配/大小不符/MD5不符)
pub const EXIT_INTERMEDIATE: i32 = 6; // 写失败且回滚失败(中间态, 需人处理)
pub const EXIT_ROLLED_BACK: i32 = 7; // 写失败但已完整回滚(可安全重试)
pub const EXIT_CANCELLED: i32 = 130; // 用户取消(空选择/未输 YES)

/// 工具级错误: message 走 stderr, code 走 process::exit。
#[derive(Debug)]
pub struct EdpCliError {
    pub code: i32,
    pub msg: String,
}

impl EdpCliError {
    pub fn new(code: i32, msg: impl Into<String>) -> Self {
        Self {
            code,
            msg: msg.into(),
        }
    }
}

pub type EdpCliResult<T> = Result<T, EdpCliError>;

/// 容量显示: GB(10^9) 两位小数, 四舍五入(纯整数运算, 与 macOS 显示一致)。
/// Python 版为 `(b+5e6)//1e7/100` 再 `:.2f`; 本实现整数路径可证逐位等价。
pub fn fmt_gb(num_bytes: u64) -> String {
    let cent = (num_bytes + 5_000_000) / 10_000_000; // GB×100, 四舍五入
    format!("{}.{:02}GB", cent / 100, cent % 100)
}

/// Python `format(n, ',')` 千分位等价。
pub fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

/// Python 3 `round()` 语义: 银行家舍入(ties to even)。
/// Rust `f64::round` 是 ties away from zero, 整扇区 tie(如 --size 50.5)会漂移。
pub fn py_round_half_even(x: f64) -> i64 {
    let floor = x.floor();
    let diff = x - floor;
    if diff < 0.5 {
        floor as i64
    } else if diff > 0.5 {
        (floor + 1.0) as i64
    } else {
        // 恰为 .5: 取偶数邻域
        let lo = floor as i64;
        if lo % 2 == 0 {
            lo
        } else {
            lo + 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_gb_matches_python() {
        // Python: f'{(b + 5*10**6)//10**7/100:.2f}GB'
        assert_eq!(fmt_gb(0), "0.00GB");
        assert_eq!(fmt_gb(62_914_560_000), "62.91GB"); // netac 总容
        assert_eq!(fmt_gb(59_750_819_680), "59.75GB"); // netac Share
        assert_eq!(fmt_gb(64_000_000_000), "64.00GB");
        assert_eq!(fmt_gb(500_107_862_016), "500.11GB");
        // 舍入边界: 恰在 .005 处四舍五入
        assert_eq!(fmt_gb(1_004_999_999), "1.00GB");
        assert_eq!(fmt_gb(1_005_000_000), "1.01GB");
    }

    #[test]
    fn py_round_ties_to_even() {
        assert_eq!(py_round_half_even(2.5), 2);
        assert_eq!(py_round_half_even(3.5), 4);
        assert_eq!(py_round_half_even(0.5), 0);
        assert_eq!(py_round_half_even(1.5), 2);
        assert_eq!(py_round_half_even(2.4), 2);
        assert_eq!(py_round_half_even(2.6), 3);
        assert_eq!(py_round_half_even(98_632_812.5), 98_632_812); // --size 50.5 场景
        assert_eq!(py_round_half_even(-1.5), -2); // Python round(-1.5) == -2
        assert_eq!(py_round_half_even(-0.5), 0);
    }
}
