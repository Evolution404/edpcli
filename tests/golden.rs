//! 扇区转换金标(锁死行为漂移) + 三条铁律 + 字段语义 + 免密盘检测。
//! 金标值与 Python 版 test_sectors.py 逐字一致 — 本文件是 Rust 移植零漂移的核心关卡。

mod common;

use common::*;
use edpcli::common::SECTOR;
use edpcli::crypto::{a6b0_full, crc32_bare, xor_rolling};
use edpcli::sectors::{
    convert, convert_lba0, convert_lba12, convert_lba6, convert_lba7, find_type_entry, looks_nopwd,
    make_entry, parse_lba12, E12, E7, EDPF_TABLE_LEN, PWD_CRC,
};

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}

fn have_all_fixtures() -> bool {
    KEYS.iter().all(|k| fixture_bin(k).is_some())
}

#[test]
fn short_sector_inputs_fail_closed_instead_of_panicking() {
    let short = vec![0u8; SECTOR - 1];
    assert!(convert_lba0(&short, 1).is_err());
    assert!(convert_lba6(&short).is_err());
    assert!(convert_lba7(&short, 0, 1).is_err());
    assert!(convert_lba12(&short, &[0u8; 4], 1).is_err());
    assert!(parse_lba12(&short, "disk&ven_test&prod_test").is_none());

    let read = |_lba: u32| Ok(short.clone());
    assert!(looks_nopwd(&read, "disk&ven_test&prod_test").is_err());
    assert!(convert(&read, "disk&ven_test&prod_test", None, &mut |_| {}).is_err());
}

#[test]
fn convert_golden_default_all_disks() {
    if !have_all_fixtures() {
        eprintln!("跳过: 真实备份不可用");
        return;
    }
    for key in KEYS {
        let data = load_disk_image(key).unwrap();
        let (_, did) = fixture(key).unwrap();
        let r = convert(&read_fn_of(&data), did, None, &mut |_| {}).unwrap();
        let g = golden(key);
        assert_eq!(r.share, g.share, "{}", key);
        assert_eq!(r.enc_start, g.enc_start, "{}", key);
        assert_eq!(r.enc_size, g.enc_size, "{}", key);
        assert_eq!(r.crc, g.crc, "{}", key);
        assert_eq!(r.k0, g.k0, "{}", key);
        assert_eq!(r.lba9.is_none(), g.lba9_none, "{}", key);
        assert_eq!(sha256(&r.lba0), g.lba0, "{} LBA0", key);
        assert_eq!(sha256(&r.lba6), g.lba6, "{} LBA6", key);
        assert_eq!(sha256(&r.lba7), g.lba7, "{} LBA7", key);
        assert_eq!(sha256(&r.lba12), g.lba12, "{} LBA12", key);
    }
}

#[test]
fn convert_golden_size_gb_path() {
    let Some(data) = load_disk_image("aigo") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let (_, did) = fixture("aigo").unwrap();
    let r = convert(&read_fn_of(&data), did, Some(50.0), &mut |_| {}).unwrap();
    let g = golden("aigo_size50");
    assert_eq!(r.share, g.share); // 8 扇对齐: 97,656,248
    assert_eq!(r.enc_start, g.enc_start);
    assert_eq!(sha256(&r.lba0), g.lba0, "LBA0");
    assert_eq!(sha256(&r.lba6), g.lba6, "LBA6");
    assert_eq!(sha256(&r.lba7), g.lba7, "LBA7");
    assert_eq!(sha256(&r.lba12), g.lba12, "LBA12");
}

#[test]
fn convert_golden_size_overflow_rejected() {
    // 60GB 盘放不下 100GB Share → 拒绝(退出码 3), 而非越界写
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let (_, did) = fixture("netac").unwrap();
    let e = convert(&read_fn_of(&data), did, Some(100.0), &mut |_| {}).unwrap_err();
    assert_eq!(e.code, edpcli::common::EXIT_TARGET);
    assert!(e.msg.contains("越过"), "{}", e.msg);
}

#[test]
fn convert_golden_wrong_device_id_rejected() {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let e = convert(
        &read_fn_of(&data),
        "disk&ven_bogus&prod_x",
        None,
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(e.code, edpcli::common::EXIT_TARGET);
    assert!(e.msg.contains("EDPF"), "{}", e.msg);
}

#[test]
fn iron_rules_terminators_and_tail_preserved() {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let (_, did) = fixture("netac").unwrap();
    let r = convert(&read_fn_of(&data), did, None, &mut |_| {}).unwrap();

    // LBA7 0xC0 表尾终止符区: 明文未动 → 滚动 XOR 密文也不变
    assert_eq!(&r.lba7[0xC0..], &data[7 * SECTOR + 0xC0..8 * SECTOR]);
    // LBA12 0x120..0x16F 表尾状态明文不动，因此对应密文不动
    assert_eq!(
        &r.lba12[0x120..EDPF_TABLE_LEN],
        &data[12 * SECTOR + 0x120..12 * SECTOR + EDPF_TABLE_LEN]
    );
    // 0x170..0x1FF 明文未修改；确定性连续加密后密文也应逐字节不变
    assert_eq!(
        &r.lba12[EDPF_TABLE_LEN..],
        &data[12 * SECTOR + EDPF_TABLE_LEN..13 * SECTOR]
    );
}

#[test]
fn iron_rules_encrypt_entry_from_original() {
    // 改造后 entry1(type4 指针)的 start/size 取自原盘 type=4 entry, 不发明
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let (_, did) = fixture("netac").unwrap();
    let r = convert(&read_fn_of(&data), did, None, &mut |_| {}).unwrap();

    let key = crc32_bare(did.as_bytes()).to_le_bytes();
    let dec_old = a6b0_full(&data[12 * SECTOR..13 * SECTOR], &key, 0);
    let dec_new = a6b0_full(&r.lba12, &key, 0);
    let i_old = find_type_entry(&dec_old, E12, 4).unwrap();
    let e_old = &dec_old[i_old * E12..(i_old + 1) * E12];
    let e_new = &dec_new[E12..2 * E12];
    assert_eq!(u64_at(e_new, 0x18), u64_at(e_old, 0x18));
    assert_eq!(u64_at(e_new, 0x28), u64_at(e_old, 0x28));
}

#[test]
fn iron_rules_lba0_mbr_shape_and_lba9() {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let (_, did) = fixture("netac").unwrap();
    let r = convert(&read_fn_of(&data), did, None, &mut |_| {}).unwrap();

    let out = &r.lba0;
    assert_eq!(&out[0x1FE..0x200], &[0x55, 0xAA]);
    assert_eq!(out[0x1BE + 4], 0x07); // type=07
    assert_eq!(u32_at(out, 0x1BE + 8), 63);
    assert_eq!(u32_at(out, 0x1BE + 12) as u64, r.share);
    assert!(out[0x1CE..0x1FE].iter().all(|&b| b == 0)); // 分区2-4 清零
    assert_eq!(r.lba9.as_deref(), Some(&[0u8; SECTOR][..])); // netac LBA9 非零 → 清零产物
}

#[test]
fn make_entry_fields() {
    let Some(data) = load_disk_image("aigo") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let (_, did) = fixture("aigo").unwrap();
    let crc = crc32_bare(did.as_bytes());
    let dec = xor_rolling(&data[7 * SECTOR..8 * SECTOR], (crc & 0xFFFF) ^ (crc >> 16));
    let src = dec[E7..2 * E7].to_vec(); // 原 type=4 entry 作壳
    let e = make_entry(&src, 2, 63, 12345 * SECTOR as u64);
    assert_eq!(&e[..4], b"EDPF");
    assert_eq!(u32_at(&e, 0x08), 2); // 版本2
    assert_eq!(u32_at(&e, 0x0c), 2); // type=Share
    assert_eq!(u32_at(&e, 0x10), 1); // 激活
    assert_eq!(u32_at(&e, 0x14), 1); // 加密使能
    assert_eq!(u64_at(&e, 0x18), 63);
    assert_eq!(u64_at(&e, 0x20), 0x200); // bps=512
    assert_eq!(u64_at(&e, 0x28), 12345 * SECTOR as u64);
    assert_eq!(u32_at(&e, 0x30), PWD_CRC); // CRC32("0000aaaa")
    assert_eq!(&e[0x34..], &src[0x34..]); // 其余字段原样保留
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
        let g = golden(key);
        let enc = parts.iter().find(|p| p.ptype == 4).unwrap();
        assert_eq!(enc.start_lba, g.enc_start, "{}", key);
        assert_eq!(enc.size_bytes, g.enc_size, "{}", key);
    }
    // 转换后: 2 条 — Share@63(激活) + Encrypt 指针; 错误 id 解不出 → None
    let (img, did) = converted_image("netac").unwrap();
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
        let (img, did) = converted_image(key).unwrap();
        assert!(looks_nopwd(&read_fn_of(&img), &did).unwrap(), "{}", key);
    }
}

#[test]
fn looks_nopwd_wrong_device_id_negative() {
    let Some((img, _)) = converted_image("netac") else {
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
        converted_image("netac"),
        load_disk_image("netac"),
        converted_image("aigo"),
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
