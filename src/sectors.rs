//! 只读扇区解析与盘状态识别。
//!
//! 写盘/制盘构造统一由 `crate::provision` 负责；本模块只保留识别现有盘所需的
//! LBA12 EDPF 解析和免密状态检测，避免再次形成第二套元数据写入实现。

use crate::common::{EdpCliError, EdpCliResult, EXIT_TARGET, SECTOR};
use crate::crypto::{a6b0_full, crc32_bare};

pub const EDPF_TABLE_LEN: usize = 0x170;
pub const E12: usize = 0x60;

/// 扇区读取抽象(lba → 512B), 真盘/镜像/备份文件各提供实现。
pub type ReadFn<'a> = &'a dyn Fn(u32) -> EdpCliResult<Vec<u8>>;

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

// ══════════════════════════════════════════════════════════════════
// 免密盘检测
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
// LBA12 EDPF 分区表解析(供 list 展示)
// ══════════════════════════════════════════════════════════════════
#[derive(Debug, Clone)]
pub struct EdpfPartition {
    pub ptype: u32,
    pub active: u32,
    pub enc: u32,
    pub start_lba: u64,
    pub size_bytes: u64,
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
