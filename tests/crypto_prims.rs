//! 真实盘密文可解性测试(Python test_crypto.py::TestAgainstRealDisks)
//! + md5 对仓库 .md5 sidecar 的实数据校验。

mod common;

use common::*;
use nopwd::common::SECTOR;
use nopwd::crypto::{a6b0_full, crc32_bare, xor_rolling};
use nopwd::sectors::EDPF_ENC_LEN;

#[test]
fn lba12_decrypts_to_edpf() {
    // 三种真实盘备份: LBA12 前 368B 用各自 CRC 作 key 必须解出 EDPF magic
    for key in KEYS {
        let Some(data) = load_disk_image(key) else {
            eprintln!("跳过: 真实备份不可用");
            return;
        };
        let (_, did) = fixture(key).unwrap();
        let crc_key = crc32_bare(did.as_bytes()).to_le_bytes();
        let dec = a6b0_full(&data[12 * SECTOR..12 * SECTOR + EDPF_ENC_LEN], &crc_key, 0);
        assert_eq!(&dec[..4], b"EDPF", "{}", key);
    }
}

#[test]
fn lba7_decrypts_to_edpf() {
    for key in KEYS {
        let Some(data) = load_disk_image(key) else {
            eprintln!("跳过: 真实备份不可用");
            return;
        };
        let (_, did) = fixture(key).unwrap();
        let crc = crc32_bare(did.as_bytes());
        let k0 = (crc & 0xFFFF) ^ (crc >> 16);
        let dec = xor_rolling(&data[7 * SECTOR..8 * SECTOR], k0);
        assert_eq!(&dec[..4], b"EDPF", "{}", key);
    }
}

#[test]
fn wrong_id_does_not_decrypt() {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let crc = crc32_bare(b"disk&ven_bogus&prod_x");
    let k0 = (crc & 0xFFFF) ^ (crc >> 16);
    assert_ne!(&xor_rolling(&data[7 * SECTOR..8 * SECTOR], k0)[..4], b"EDPF");
}

#[test]
fn md5_matches_committed_sidecars() {
    // Rust MD5 实现对全部已提交备份的实数据校验(sidecar 由 Python hashlib 生成)
    let mut checked = 0;
    for key in KEYS {
        let Some(p) = fixture_bin(key) else { continue };
        let data = std::fs::read(&p).unwrap();
        let want = std::fs::read_to_string(format!("{}.md5", p.display())).unwrap();
        assert_eq!(md5(&data), want.trim(), "{}", p.display());
        checked += 1;
    }
    assert!(checked > 0, "仓库 backup/ 夹具缺失");
}
