//! 扇区格式与转换: 把 3 分区加密盘(Boot/Share/Encrypt)"原盘原地 patch"成
//! 2 分区免密盘。
//!
//! 原理(2026-08-27 定案, 内网实测成功):
//!     LBA0  : MBR 分区1 → type=0x07 @63 × Share扇数(数据区直挂, 系统原生挂载)
//!     LBA6  : 保留历史兼容补丁并重算校验和（逆向已确认 0x1CA/0x1D4
//!             分别落在 GSerial/BeiZhu 固定槽内，不是独立协议字段）
//!     LBA7  : EDPF 2条版本2 [Share@63, type4指针(原盘保留)] + 表尾终止符保留
//!     LBA12 : 整扇连续 A6B0；仅修改前 0x170 的 EDPF 表区，后 144B 明文保持不变
//!     LBA9  : 非零则清零(EETU)
//!     其余扇区(LBA4/8/11 及全零保留区)一律不动
//!   三条铁律: EDPF 表尾终止符必须保留(LBA7@0xC0/LBA12@0x120);
//!             LBA12 0x170-0x200 明文不可擅改; 不发明原盘没有的状态。
//!   分区参数按实际物理盘计算: Encrypt 从原盘 LBA12 type=4 读取, Share 占满其前。

use crate::common::{
    fmt_gb, group_digits, py_round_half_even, EdpCliError, EdpCliResult, EXIT_TARGET, SECTOR,
};
use crate::crypto::{
    a6b0_full, a7f0_full, crc32_bare, lba6_checksum, lba6_decode, xor_rolling, LBA6_K0,
};

pub const EDPF_TABLE_LEN: usize = 0x170; // LBA12 EDPF 表区边界；整扇 512B 都连续 A6B0
pub const E7: usize = 0x40; // entry stride: LBA7=64B
pub const E12: usize = 0x60; // entry stride: LBA12=96B
pub const PWD_CRC: u32 = 0x0429735D; // CRC32_bare("0000aaaa") 免密盘默认密码
                                     // Legacy in-place conversion recipe only. Reverse audit shows +0x1CA is inside
                                     // the fixed m_usbGSerial slot, while +0x1D4 is inside BeiZhu; neither is a
                                     // standalone SAFE6 state field. New-disk Provision must not use these values.
pub const NOPWD_LBA6_1CA: u32 = 128480;
pub const LBA6_CLEAR: (usize, usize) = (0x1D4, 0x1ED);

/// 扇区读取抽象(lba → 512B), 真盘/镜像/备份文件各提供实现。
pub type ReadFn<'a> = &'a dyn Fn(u32) -> EdpCliResult<Vec<u8>>;

fn part_type_name(t: u32) -> &'static str {
    match t {
        1 => "Boot",
        2 => "Share",
        4 => "Encrypt/IIR指针",
        _ => "?",
    }
}

fn require_sector(raw: &[u8], label: &str) -> EdpCliResult<()> {
    if raw.len() == SECTOR {
        return Ok(());
    }
    Err(EdpCliError::new(
        EXIT_TARGET,
        format!(
            "错误: {label} 长度为 {}B，预期完整扇区 {}B",
            raw.len(),
            SECTOR
        ),
    ))
}

fn read_sector(read: ReadFn, lba: u32) -> EdpCliResult<Vec<u8>> {
    let raw = read(lba)?;
    require_sector(&raw, &format!("LBA{lba}"))?;
    Ok(raw)
}

// ══════════════════════════════════════════════════════════════════
// 1. EDPF entry 工具
// ══════════════════════════════════════════════════════════════════
fn ent(dec: &[u8], i: usize, stride: usize) -> &[u8] {
    &dec[i * stride..(i + 1) * stride]
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}

pub fn find_type_entry(dec: &[u8], stride: usize, ptype: u32) -> EdpCliResult<usize> {
    let Some(required) = stride.checked_mul(3) else {
        return Err(EdpCliError::new(
            EXIT_TARGET,
            "错误: EDPF entry stride 溢出",
        ));
    };
    if stride == 0 || dec.len() < required {
        return Err(EdpCliError::new(
            EXIT_TARGET,
            format!(
                "错误: EDPF 数据不足: {}B，至少需要 {}B",
                dec.len(),
                required
            ),
        ));
    }
    for i in 0..3 {
        let e = ent(dec, i, stride);
        if &e[..4] == b"EDPF" && u32_at(e, 0x0c) == ptype {
            return Ok(i);
        }
    }
    Err(EdpCliError::new(
        EXIT_TARGET,
        format!(
            "错误: EDPF 中未找到 type={}({}) entry",
            ptype,
            part_type_name(ptype)
        ),
    ))
}

/// 以原 type=4 entry 为壳, 改 type/start/size + 免密规则字段, 其余原样。
/// entry0(Share)与 entry1 由此天然成对(材料字段相同)。
pub fn make_entry(src_e: &[u8], ptype: u32, start: u64, size: u64) -> Vec<u8> {
    let mut e = src_e.to_vec();
    e[0..4].copy_from_slice(b"EDPF");
    e[0x08..0x0C].copy_from_slice(&2u32.to_le_bytes()); // 版本 2
    e[0x0C..0x10].copy_from_slice(&ptype.to_le_bytes());
    e[0x10..0x14].copy_from_slice(&1u32.to_le_bytes()); // 激活=1 (原 Encrypt 为 0)
    e[0x14..0x18].copy_from_slice(&1u32.to_le_bytes()); // 加密使能=1 (原 Boot 为 0)
    e[0x18..0x20].copy_from_slice(&start.to_le_bytes());
    e[0x20..0x28].copy_from_slice(&0x200u64.to_le_bytes()); // bps=512
    e[0x28..0x30].copy_from_slice(&size.to_le_bytes());
    e[0x30..0x34].copy_from_slice(&PWD_CRC.to_le_bytes()); // pwdCRC=CRC32("0000aaaa")
    e
}

// ══════════════════════════════════════════════════════════════════
// 2. 单扇区转换
// ══════════════════════════════════════════════════════════════════
pub fn convert_lba0(raw: &[u8], share_sectors: u64) -> EdpCliResult<Vec<u8>> {
    require_sector(raw, "LBA0")?;
    let mut out = raw.to_vec();
    for i in 0..4 {
        out[0x1BE + i * 16..0x1BE + (i + 1) * 16].fill(0);
    }
    out[0x1BE + 4] = 0x07;
    out[0x1BE + 8..0x1BE + 12].copy_from_slice(&63u32.to_le_bytes());
    let n = u32::try_from(share_sectors).map_err(|_| {
        EdpCliError::new(
            EXIT_TARGET,
            format!("错误: Share 扇区数 {} 溢出 MBR u32 字段", share_sectors),
        )
    })?;
    out[0x1BE + 12..0x1BE + 16].copy_from_slice(&n.to_le_bytes());
    out[0x1FE..0x200].copy_from_slice(&[0x55, 0xAA]);
    Ok(out)
}

pub fn convert_lba6(raw: &[u8]) -> EdpCliResult<(Vec<u8>, Vec<u8>)> {
    require_sector(raw, "LBA6")?;
    let mut dec = lba6_decode(raw);
    if dec[0x188..0x190] == [0u8; 8] {
        return Err(EdpCliError::new(
            EXIT_TARGET,
            "错误: LBA6 解密后 0x188 magic 为零 — 非法 SAFE6",
        ));
    }
    dec[0x1CA..0x1CE].copy_from_slice(&NOPWD_LBA6_1CA.to_le_bytes());
    let (lo, hi) = LBA6_CLEAR;
    dec[lo..hi].fill(0);
    let cipher = xor_rolling(&dec[..0x1FC], LBA6_K0);
    let csum = lba6_checksum(&cipher);
    let mut new = cipher;
    new.extend_from_slice(&csum.to_le_bytes());
    if lba6_decode(&new)[..0x1FC] != dec[..0x1FC] {
        return Err(EdpCliError::new(EXIT_TARGET, "错误: LBA6 往返自检失败"));
    }
    Ok((new, dec))
}

pub fn convert_lba7(raw: &[u8], k0: u32, share_sectors: u64) -> EdpCliResult<(Vec<u8>, Vec<u8>)> {
    require_sector(raw, "LBA7")?;
    let mut dec = xor_rolling(raw, k0);
    if dec[..4] != *b"EDPF" {
        return Err(EdpCliError::new(
            EXIT_TARGET,
            format!(
                "错误: LBA7 解密后非 EDPF magic({}) — device_id/K0 不符",
                hex4(&dec[..4])
            ),
        ));
    }
    let src = ent(&dec, find_type_entry(&dec, E7, 4)?, E7).to_vec();
    dec[0..E7].copy_from_slice(&make_entry(&src, 2, 63, share_sectors * SECTOR as u64));
    // 第二次 find_type_entry 在覆写 entry0 之后的表上再扫描(与 Python 行为一致, 勿合并)
    let (s1, z1) = {
        let e1 = ent(&dec, find_type_entry(&dec, E7, 4)?, E7);
        (u64_at(e1, 0x18), u64_at(e1, 0x28))
    };
    dec[E7..2 * E7].copy_from_slice(&make_entry(&src, 4, s1, z1));
    dec[2 * E7..3 * E7].fill(0); // entry2 区清零(3条→2条)
                                 // 0xC0 表尾终止符及之后不动
    Ok((xor_rolling(&dec, k0), dec))
}

pub fn convert_lba12(
    raw: &[u8],
    crc_key: &[u8],
    share_sectors: u64,
) -> EdpCliResult<(Vec<u8>, Vec<u8>)> {
    require_sector(raw, "LBA12")?;
    let mut dec = a6b0_full(raw, crc_key, 0);
    if dec[..4] != *b"EDPF" {
        return Err(EdpCliError::new(
            EXIT_TARGET,
            format!(
                "错误: LBA12 解密后非 EDPF magic({}) — device_id/CRC 不符",
                hex4(&dec[..4])
            ),
        ));
    }
    let src = ent(&dec, find_type_entry(&dec, E12, 4)?, E12).to_vec();
    dec[0..E12].copy_from_slice(&make_entry(&src, 2, 63, share_sectors * SECTOR as u64));
    // s1/z1 取自保存的原表 src(与 Python 一致)
    let (s1, z1) = (u64_at(&src, 0x18), u64_at(&src, 0x28));
    dec[E12..2 * E12].copy_from_slice(&make_entry(&src, 4, s1, z1));
    dec[2 * E12..3 * E12].fill(0); // entry2 区清零
                                   // 0x120 表尾状态及 0x170..0x1FF 明文都保持原样
    let enc = a7f0_full(&dec, crc_key, 0);
    if a6b0_full(&enc, crc_key, 0) != dec {
        return Err(EdpCliError::new(
            EXIT_TARGET,
            "错误: LBA12 A6B0/a7f0 往返自检失败",
        ));
    }
    Ok((enc, dec))
}

fn hex4(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

// ══════════════════════════════════════════════════════════════════
// 3. 已改造(免密)盘检测
// ══════════════════════════════════════════════════════════════════
/// 已是免密盘? 由 MBR + LBA12 两处独立主信号共同确认:
///   MBR   分区1 = type=0x07 @LBA63 带 55AA
///   LBA12 以 device_id 派生 key 解密后: entry0=Share(type2,@63,active=1,enc=1),
///         entry1=Encrypt指针(type4,active=1), entry2 区已清零 (主信号:
///         原盘恒为 3 条 EDPF, entry0 enc=0, entry2 type4/active=0;
///         注意 aigo 原盘 entry0 也是 type=2@63, 故不能只看 type/start)
pub fn looks_nopwd(read: ReadFn, device_id: &str) -> EdpCliResult<bool> {
    let mbr = read_sector(read, 0)?;
    if !(mbr[0x1BE + 4] == 0x07
        && u32_at(&mbr, 0x1BE + 8) == 63
        && mbr[0x1FE..0x200] == [0x55, 0xAA])
    {
        return Ok(false);
    }
    let crc = crc32_bare(device_id.as_bytes());
    let crc_key = crc.to_le_bytes();
    let raw12 = read_sector(read, 12)?;
    let dec12 = a6b0_full(&raw12, &crc_key, 0);
    if dec12[..4] != *b"EDPF" {
        return Ok(false);
    }
    let e0 = &dec12[0..E12];
    let e1 = &dec12[E12..2 * E12];
    let ok_e0 = u32_at(e0, 0x0c) == 2 // type=Share
        && u32_at(e0, 0x10) == 1 // active
        && u32_at(e0, 0x14) == 1 // enc 使能
        && u64_at(e0, 0x18) == 63;
    let ok_e1 = &e1[..4] == b"EDPF" && u32_at(e1, 0x0c) == 4 && u32_at(e1, 0x10) == 1;
    Ok(ok_e0 && ok_e1 && dec12[2 * E12..3 * E12].iter().all(|&b| b == 0))
}

// ══════════════════════════════════════════════════════════════════
// 4. LBA12 EDPF 分区表解析(供 list 展示)
// ══════════════════════════════════════════════════════════════════
#[derive(Debug, Clone)]
pub struct EdpfPartition {
    pub ptype: u32,
    pub active: u32,
    pub enc: u32,
    pub start_lba: u64,
    pub size_bytes: u64, // entry 0x28 为字节数(与 convert 的 enc_size 同源)
}

impl EdpfPartition {
    pub fn type_name(&self) -> &'static str {
        match self.ptype {
            1 => "Boot",
            2 => "Share",
            4 => "Encrypt",
            _ => "?",
        }
    }
    pub fn end_lba(&self) -> u64 {
        if self.size_bytes >= SECTOR as u64 {
            self.start_lba + self.size_bytes / SECTOR as u64 - 1
        } else {
            self.start_lba
        }
    }
}

/// 以 device_id 解密 LBA12 并解析 EDPF 分区表(至多 3 条, 遇非 EDPF entry 即止)。
/// 解不出 EDPF magic(非 cems 盘/盘未识别)返回 None。
pub fn parse_lba12(raw12: &[u8], device_id: &str) -> Option<Vec<EdpfPartition>> {
    if raw12.len() != SECTOR {
        return None;
    }
    let crc = crc32_bare(device_id.as_bytes());
    let key = crc.to_le_bytes();
    let dec = a6b0_full(raw12, &key, 0);
    if dec[..4] != *b"EDPF" {
        return None;
    }
    let mut out = Vec::new();
    for i in 0..3 {
        let e = ent(&dec, i, E12);
        if &e[..4] != b"EDPF" {
            break;
        }
        out.push(EdpfPartition {
            ptype: u32_at(e, 0x0c),
            active: u32_at(e, 0x10),
            enc: u32_at(e, 0x14),
            start_lba: u64_at(e, 0x18),
            size_bytes: u64_at(e, 0x28),
        });
    }
    Some(out)
}

// ══════════════════════════════════════════════════════════════════
// 5. 主转换
// ══════════════════════════════════════════════════════════════════
#[derive(Debug)]
pub struct ConvertResult {
    pub lba0: Vec<u8>,
    pub lba6: Vec<u8>,
    pub lba7: Vec<u8>,
    pub lba12: Vec<u8>,
    pub lba9: Option<Vec<u8>>,
    pub share: u64,
    pub enc_start: u64,
    pub enc_size: u64,
    pub k0: u32,
    pub crc: u32,
    pub plain7: Vec<u8>,
    pub plain12: Vec<u8>,
    pub raw12: Vec<u8>,
}

pub fn convert(
    read: ReadFn,
    device_id: &str,
    size_gb: Option<f64>,
    verbose: bool,
) -> EdpCliResult<ConvertResult> {
    let crc = crc32_bare(device_id.as_bytes());
    let k0 = (crc & 0xFFFF) ^ (crc >> 16);
    let crc_key = crc.to_le_bytes();
    if verbose {
        println!(
            "{}  {}  (CRC32 0x{:08X}, K0 0x{:04X})",
            crate::ui::bold("标识"),
            device_id,
            crc,
            k0
        );
    }

    let raw12 = read_sector(read, 12)?;
    let dec12 = a6b0_full(&raw12, &crc_key, 0);
    if dec12[..4] != *b"EDPF" {
        return Err(EdpCliError::new(
            EXIT_TARGET,
            format!(
                "错误: LBA12 解密后非 EDPF({}) — device_id 不符或非 cems 盘",
                hex4(&dec12[..4])
            ),
        ));
    }
    let enc_e = ent(&dec12, find_type_entry(&dec12, E12, 4)?, E12);
    let enc_start = u64_at(enc_e, 0x18);
    let enc_size = u64_at(enc_e, 0x28);
    let share: u64 = if let Some(gb) = size_gb {
        // GB(10^9), 8 扇对齐; round 为 Python 银行家舍入
        (py_round_half_even(gb * 1e9 / SECTOR as f64) / 8 * 8) as u64
    } else {
        enc_start - 63
    };
    if 63 + share > enc_start {
        return Err(EdpCliError::new(
            EXIT_TARGET,
            format!(
                "错误: Share@63+{} 越过 Encrypt@{}",
                group_digits(share),
                group_digits(enc_start)
            ),
        ));
    }
    if verbose {
        let enc_end = enc_start + enc_size / SECTOR as u64 - 1;
        let share_range = format!("LBA 63 ~ {}", group_digits(63 + share - 1));
        let enc_range = format!(
            "LBA {} ~ {}",
            group_digits(enc_start),
            group_digits(enc_end)
        );
        println!("{}", crate::ui::bold_cyan("布局"));
        let rows = vec![
            vec![
                crate::ui::TableCell::left("Share", crate::ui::Tone::Green),
                crate::ui::TableCell::right(share_range, crate::ui::Tone::Green),
                crate::ui::TableCell::right(
                    fmt_gb(share * SECTOR as u64),
                    crate::ui::Tone::Magenta,
                ),
                crate::ui::TableCell::left("明文数据区，系统直接挂载读写", crate::ui::Tone::Plain),
            ],
            vec![
                crate::ui::TableCell::left("Encrypt", crate::ui::Tone::Yellow),
                crate::ui::TableCell::right(enc_range, crate::ui::Tone::Green),
                crate::ui::TableCell::right(fmt_gb(enc_size), crate::ui::Tone::Magenta),
                crate::ui::TableCell::left("原样保留不动", crate::ui::Tone::Dim),
            ],
        ];
        print!(
            "{}",
            crate::ui::render_table(&["区域", "LBA 范围", "大小", "说明"], &rows)
        );
    }

    let (new12, plain12) = convert_lba12(&raw12, &crc_key, share)?;
    let raw7 = read_sector(read, 7)?;
    let (new7, plain7) = convert_lba7(&raw7, k0, share)?;
    let raw0 = read_sector(read, 0)?;
    let new0 = convert_lba0(&raw0, share)?;
    let raw6 = read_sector(read, 6)?;
    let (new6, _dec6) = convert_lba6(&raw6)?;
    let raw9 = read_sector(read, 9)?;
    let new9 = if raw9.iter().any(|&b| b != 0) {
        Some(vec![0u8; SECTOR])
    } else {
        None
    };

    if verbose {
        println!();
        println!("{}", crate::ui::bold_cyan("将写入 5 个扇区:"));
        let rows = vec![
            vec![
                crate::ui::TableCell::left("LBA0", crate::ui::Tone::Green),
                crate::ui::TableCell::left("MBR", crate::ui::Tone::BoldCyan),
                crate::ui::TableCell::left(
                    format!(
                        "单分区(type=07) 指向 Share: @LBA63 × {} 扇",
                        group_digits(share)
                    ),
                    crate::ui::Tone::Plain,
                ),
            ],
            vec![
                crate::ui::TableCell::left("LBA6", crate::ui::Tone::Green),
                crate::ui::TableCell::left("盘标签", crate::ui::Tone::BoldCyan),
                crate::ui::TableCell::left(
                    "兼容旧免密改造补丁，重算 SAFE6 校验和",
                    crate::ui::Tone::Plain,
                ),
            ],
            vec![
                crate::ui::TableCell::left("LBA7", crate::ui::Tone::Green),
                crate::ui::TableCell::left("分区表", crate::ui::Tone::BoldCyan),
                crate::ui::TableCell::left("2 条目: Share@63 + Encrypt", crate::ui::Tone::Plain),
            ],
            vec![
                crate::ui::TableCell::left("LBA12", crate::ui::Tone::Green),
                crate::ui::TableCell::left("分区表", crate::ui::Tone::BoldCyan),
                crate::ui::TableCell::left("2 条目: Share@63 + Encrypt", crate::ui::Tone::Plain),
            ],
            vec![
                crate::ui::TableCell::left("LBA9", crate::ui::Tone::Green),
                crate::ui::TableCell::left("临时区", crate::ui::Tone::BoldCyan),
                crate::ui::TableCell::left(
                    if new9.is_some() {
                        "清零(当前存在)"
                    } else {
                        "已是零，不写"
                    },
                    if new9.is_some() {
                        crate::ui::Tone::Plain
                    } else {
                        crate::ui::Tone::Dim
                    },
                ),
            ],
        ];
        print!(
            "{}",
            crate::ui::render_table(&["LBA", "区域", "动作"], &rows)
        );
        println!(
            "{}",
            crate::ui::dim(
                "不动   LBA4/8/11(盘身份) · 其余保留扇区 · 表尾状态 · LBA12 0x170..0x1FF 明文 · 盘尾区域"
            )
        );
    }

    Ok(ConvertResult {
        lba0: new0,
        lba6: new6,
        lba7: new7,
        lba12: new12,
        lba9: new9,
        share,
        enc_start,
        enc_size,
        k0,
        crc,
        plain7,
        plain12,
        raw12,
    })
}
