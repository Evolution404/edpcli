//! 只读扇区解析与免密状态识别门禁。

use crate::common;

use common::*;
use edpcli::common::SECTOR;
use edpcli::sectors::{looks_nopwd, parse_lba12};

fn have_all_fixtures() -> bool {
    KEYS.iter().all(|k| fixture_bin(k).is_some())
}

#[test]
fn short_sector_inputs_fail_closed_instead_of_panicking() {
    let short = vec![0u8; SECTOR - 1];
    assert!(parse_lba12(&short, "disk&ven_test&prod_test").is_none());

    let read = |_lba: u32| Ok(short.clone());
    assert!(looks_nopwd(&read, "disk&ven_test&prod_test").is_err());
}

// ══════════════════════════════════════════════════════════════════
// LBA12 EDPF 分区表解析(供 list 展示)
// ══════════════════════════════════════════════════════════════════
#[test]
fn parse_lba12_original_and_converted() {
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
    // 转换后: 2 条 — Share@63(激活) + Encrypt 指针; 错误 id 解不出 → None
    let (img, did) = passwordless_image("netac").unwrap();
    let parts = parse_lba12(&img[12 * SECTOR..13 * SECTOR], &did).unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(
        (parts[0].ptype, parts[0].start_lba, parts[0].active),
        (2, 63, 1)
    );
    assert_eq!(parts[1].ptype, 4);
    assert!(parse_lba12(&img[12 * SECTOR..13 * SECTOR], "disk&ven_bogus&prod_x").is_none());
}

// ══════════════════════════════════════════════════════════════════
// 免密盘三信号检测 (Python test_nopwd_state.py::TestLooksNopwd)
// ══════════════════════════════════════════════════════════════════
#[test]
fn looks_nopwd_originals_all_negative() {
    if !have_all_fixtures() {
        eprintln!("跳过: 真实备份不可用");
        return;
    }
    for key in KEYS {
        let data = load_disk_image(key).unwrap();
        let (_, did) = fixture(key).unwrap();
        assert!(!looks_nopwd(&read_fn_of(&data), did).unwrap(), "{}", key);
    }
}

#[test]
fn looks_nopwd_converted_all_positive() {
    if !have_all_fixtures() {
        eprintln!("跳过: 真实备份不可用");
        return;
    }
    for key in KEYS {
        let (img, did) = passwordless_image(key).unwrap();
        assert!(looks_nopwd(&read_fn_of(&img), &did).unwrap(), "{}", key);
    }
}

#[test]
fn looks_nopwd_wrong_device_id_negative() {
    let Some((img, _)) = passwordless_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    assert!(!looks_nopwd(&read_fn_of(&img), "disk&ven_bogus&prod_x").unwrap());
}

#[test]
fn looks_nopwd_core_signals_required() {
    // MBR / LBA12 任一恢复原盘即判否。LBA6 +0x1CA 位于 GSerial 固定槽内，
    // 不是独立 nopwd 状态字段，因此恢复原盘 LBA6 不应改变检测结果。
    let (Some((img, did)), Some(orig_netac), Some((img_a, did_a)), Some(orig_aigo)) = (
        passwordless_image("netac"),
        load_disk_image("netac"),
        passwordless_image("aigo"),
        load_disk_image("aigo"),
    ) else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    for lba in [0usize, 12] {
        let mut mixed = img.clone();
        mixed[lba * SECTOR..(lba + 1) * SECTOR]
            .copy_from_slice(&orig_netac[lba * SECTOR..(lba + 1) * SECTOR]);
        assert!(
            !looks_nopwd(&read_fn_of(&mixed), &did).unwrap(),
            "LBA{} 恢复原盘后仍误判为免密",
            lba
        );
    }
    let mut mixed_a = img_a;
    mixed_a[6 * SECTOR..7 * SECTOR].copy_from_slice(&orig_aigo[6 * SECTOR..7 * SECTOR]);
    assert!(
        looks_nopwd(&read_fn_of(&mixed_a), &did_a).unwrap(),
        "LBA6 GSerial/BeiZhu 槽不应参与 nopwd 状态判定"
    );
}
