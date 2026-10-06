//! 只读扇区解析与盘状态识别。
//!
//! 写盘/制盘构造统一由 `crate::provision` 负责；本模块只保留识别现有盘所需的
//! LBA12 EDPF 解析；写入语义只由统一 provisioning 模块负责。

use crate::common::SECTOR;
use crate::protocol::crypto::{a6b0_full, crc32_bare};

pub const EDPF_TABLE_LEN: usize = 0x170;
pub const E12: usize = 0x60;

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
            self.start_lba
                .saturating_add(self.size_bytes / SECTOR as u64 - 1)
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

#[cfg(test)]
mod bounds_tests {
    use super::EdpfPartition;

    #[test]
    fn corrupted_partition_end_is_bounded_for_display() {
        let part = EdpfPartition {
            ptype: 2,
            active: 1,
            enc: 0,
            start_lba: u64::MAX - 1,
            size_bytes: u64::MAX,
        };
        assert_eq!(part.end_lba(), u64::MAX);
    }
}
