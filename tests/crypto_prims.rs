//! 真实盘密文可解性测试(Python test_crypto.py::TestAgainstRealDisks)
//! + sha256 对仓库 .sha256 sidecar 的实数据校验。

use crate::common;

use common::*;
use edpcli::application::support::SECTOR;
use edpcli::protocol::crypto::{a6b0_full, crc32_bare, xor_rolling};
use edpcli::protocol::sectors::EDPF_TABLE_LEN;

#[test]
fn lba12_decrypts_to_edpf() {
    // 三种真实盘备份: LBA12 整扇用各自 CRC 作 key 必须解出 EDPF，
    // 且 EDPF 表区之后的 144B 明文为零。
    for key in KEYS {
        let data = load_disk_image(key);
        let (_, did) = fixture(key).unwrap();
        let crc_key = crc32_bare(did.as_bytes()).to_le_bytes();
        let dec = a6b0_full(&data[12 * SECTOR..13 * SECTOR], &crc_key, 0);
        assert_eq!(&dec[..4], b"EDPF", "{}", key);
        assert!(
            dec[EDPF_TABLE_LEN..].iter().all(|byte| *byte == 0),
            "{}",
            key
        );
    }
}

#[test]
fn lba7_decrypts_to_edpf() {
    for key in KEYS {
        let data = load_disk_image(key);
        let (_, did) = fixture(key).unwrap();
        let crc = crc32_bare(did.as_bytes());
        let k0 = (crc & 0xFFFF) ^ (crc >> 16);
        let dec = xor_rolling(&data[7 * SECTOR..8 * SECTOR], k0);
        assert_eq!(&dec[..4], b"EDPF", "{}", key);
    }
}

#[test]
fn wrong_id_does_not_decrypt() {
    let data = load_disk_image("netac");
    let crc = crc32_bare(b"disk&ven_bogus&prod_x");
    let k0 = (crc & 0xFFFF) ^ (crc >> 16);
    assert_ne!(
        &xor_rolling(&data[7 * SECTOR..8 * SECTOR], k0)[..4],
        b"EDPF"
    );
}

#[test]
fn sha256_matches_committed_sidecars() {
    // Rust SHA-256 实现对全部已提交备份的实数据校验(sidecar 由 Python hashlib 生成)
    let mut checked = 0;
    for key in KEYS {
        let p = fixture_bin(key);
        let data = std::fs::read(&p).unwrap();
        let want = std::fs::read_to_string(format!("{}.sha256", p.display())).unwrap();
        assert_eq!(sha256(&data), want.trim(), "{}", p.display());
        checked += 1;
    }
    assert_eq!(checked, KEYS.len(), "required fixture population changed");
}

#[test]
fn required_fixture_missing_truncated_and_corrupt_fail_closed() {
    let tmp = TmpDir::new("required_fixture_failure");
    let path = tmp.0.join("required.bin");
    assert!(std::panic::catch_unwind(|| required_image(&path)).is_err());
    std::fs::write(&path, [0; 512]).unwrap();
    assert!(std::panic::catch_unwind(|| required_image(&path)).is_err());
    std::fs::write(&path, [0; 13 * 512]).unwrap();
    std::fs::write(format!("{}.sha256", path.display()), "ab".repeat(32)).unwrap();
    assert!(std::panic::catch_unwind(|| required_image(&path)).is_err());
}
