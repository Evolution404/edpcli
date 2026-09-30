//! 公共常量与工具: 扇区大小、容量显示、Python 兼容舍入、退出码契约。

pub const SECTOR: usize = 512;
pub const METADATA_SECTOR_COUNT: usize = 13;
pub const METADATA_LAST_LBA: u32 = 12;
pub const METADATA_IMAGE_LEN: usize = METADATA_SECTOR_COUNT * SECTOR;

/// 退出码契约(脚本可区分失败类型; 原 Python 版一律 exit 1):
pub const EXIT_OK: i32 = 0; // 成功(含 dry-run/预览)
pub const EXIT_IO: i32 = 1; // 运行时 IO 错误
pub const EXIT_USAGE: i32 = 2; // 用法错误(未知旗标/缺参数/参数非数字)
pub const EXIT_TARGET: i32 = 3; // 目标不可用(非cems/识别失败/size越界/系统盘)
pub const EXIT_BACKUP: i32 = 5; // 备份问题(无匹配/大小不符/SHA-256不符)
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

/// 全项目被动容量展示使用的换算口径。
///
/// 修改这一处即可在十进制 1000 制与二进制 1024 制之间切换。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityUnitSystem {
    Decimal,
    Binary,
}

pub const CAPACITY_UNIT_SYSTEM: CapacityUnitSystem = CapacityUnitSystem::Decimal;

/// 全项目统一的自动量级容量展示。
///
/// Decimal: B / KB / MB / GB，按 1000 进位。
/// Binary:  B / KiB / MiB / GiB，按 1024 进位。
pub fn fmt_capacity(num_bytes: u64) -> String {
    fmt_capacity_with_system(num_bytes, CAPACITY_UNIT_SYSTEM)
}

pub fn fmt_capacity_with_system(num_bytes: u64, system: CapacityUnitSystem) -> String {
    let (base, suffixes): (u64, [&str; 4]) = match system {
        CapacityUnitSystem::Decimal => (1_000, ["B", "KB", "MB", "GB"]),
        CapacityUnitSystem::Binary => (1_024, ["B", "KiB", "MiB", "GiB"]),
    };

    let mut divisor = 1_u64;
    let mut level = 0_usize;
    while level < suffixes.len() - 1 && num_bytes >= divisor.saturating_mul(base) {
        divisor = divisor.saturating_mul(base);
        level += 1;
    }

    if level == 0 {
        return format!("{num_bytes}B");
    }

    let mut divisor = u128::from(divisor);
    let mut hundredths = (u128::from(num_bytes) * 100 + divisor / 2) / divisor;
    while level < suffixes.len() - 1 && hundredths >= u128::from(base) * 100 {
        divisor *= u128::from(base);
        level += 1;
        hundredths = (u128::from(num_bytes) * 100 + divisor / 2) / divisor;
    }
    format!(
        "{}.{:02}{}",
        hundredths / 100,
        hundredths % 100,
        suffixes[level]
    )
}

/// 扇区容量统一展示；协议与几何仍保持 sector 作为真值。
pub fn fmt_capacity_sectors(sectors: u64) -> String {
    fmt_capacity(sectors.saturating_mul(SECTOR as u64))
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
    fn capacity_display_defaults_to_decimal_auto_scale() {
        assert_eq!(CAPACITY_UNIT_SYSTEM, CapacityUnitSystem::Decimal);
        assert_eq!(fmt_capacity(999), "999B");
        assert_eq!(fmt_capacity(123_000), "123.00KB");
        assert_eq!(fmt_capacity(999_999), "1.00MB");
        assert_eq!(fmt_capacity(10_450_000), "10.45MB");
        assert_eq!(fmt_capacity(64_000_000_000), "64.00GB");
        assert_eq!(fmt_capacity(500_107_862_016), "500.11GB");
    }

    #[test]
    fn capacity_display_can_switch_globally_to_binary_1024_scale() {
        assert_eq!(
            fmt_capacity_with_system(512_000_000_000, CapacityUnitSystem::Binary),
            "476.84GiB"
        );
        assert_eq!(
            fmt_capacity_with_system(10_485_760, CapacityUnitSystem::Binary),
            "10.00MiB"
        );
        assert_eq!(
            fmt_capacity_with_system(512_000_000_000, CapacityUnitSystem::Decimal),
            "512.00GB"
        );
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
