//! 只读扇区解析门禁。

use crate::common;

use common::*;
use edpcli::common::SECTOR;
use edpcli::sectors::parse_lba12;

fn have_all_fixtures() -> bool {
    KEYS.iter().all(|k| fixture_bin(k).is_some())
}

#[test]
fn short_sector_inputs_fail_closed_instead_of_panicking() {
    let short = vec![0u8; SECTOR - 1];
    assert!(parse_lba12(&short, "disk&ven_test&prod_test").is_none());
}

// ══════════════════════════════════════════════════════════════════
// LBA12 EDPF 分区表解析(供 list 展示)
// ══════════════════════════════════════════════════════════════════
#[test]
fn parse_lba12_original_and_mode1() {
    if !have_all_fixtures() {
        eprintln!("跳过: 真实备份不可用");
        return;
    }
    for key in KEYS {
        // 原盘: 恒 3 条 EDPF; type=4 entry 的 start/size 与金标布局一致
        let data = load_disk_image(key).unwrap();
        let (_, did) = fixture(key).unwrap();
        let parts = parse_lba12(&data[12 * SECTOR..13 * SECTOR], did).unwrap();
        assert_eq!(parts.len(), 3, "{}", key);
        let enc = parts.iter().find(|p| p.ptype == 4).unwrap();
        assert!(enc.start_lba > 63, "{}", key);
        assert!(enc.size_bytes >= SECTOR as u64, "{}", key);
    }
    // mode1: 2 条 — Share@63(激活) + Encrypt 指针；错误 id 解不出 → None
    let (img, did) = mode1_fixture_image("netac").unwrap();
    let parts = parse_lba12(&img[12 * SECTOR..13 * SECTOR], &did).unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(
        (parts[0].ptype, parts[0].start_lba, parts[0].active),
        (2, 63, 1)
    );
    assert_eq!(parts[1].ptype, 4);
    assert!(parse_lba12(&img[12 * SECTOR..13 * SECTOR], "disk&ven_bogus&prod_x").is_none());
}
