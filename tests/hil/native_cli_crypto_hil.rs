//! Read-only independent crypto verifier of a formal CLI-authored native OS disk.
//! The caller creates and re-identifies its own disposable macOS Disk Image.
//! This test NEVER provisions, formats, mounts, or writes to a block device.
#![cfg(all(feature = "ci-virtual-disk", target_os = "macos"))]

use edpcli::application::filesystem::detect_native_boot_sector;
use edpcli::diskio::{NativeBlockDevice, NativeRawBlockDevice};

use edpcli::platform::{self, system, ObservedDeviceGeometry};
use edpcli::protocol::{
    crypto::{a6b0_full, aes128_ecb_decrypt_block, sm4_decrypt_block},
    image::NativeProtocolImage,
};
use edpcli::provision::{
    parse_existing_provision_native, unwrap_file_key, unwrap_legacy_lba7_file_key,
    DiskProvisionKind, FileKeyWrapMode, TargetIdentity, DEFAULT_KEY_DOMAIN_PASSWORD,
};

// Deliberately independent of the writer's NativeCipherDirection/transform helper.
fn decode_native(raw: &[u8], key: &[u8; 16], mode: u8, lba: u64, sector: u32) -> Vec<u8> {
    assert_eq!(raw.len(), sector as usize);
    match mode {
        1 => a6b0_full(raw, key, lba * u64::from(sector)),
        2 | 3 => raw
            .chunks_exact(16)
            .flat_map(|chunk| {
                let block: &[u8; 16] = chunk.try_into().unwrap();
                if mode == 2 {
                    sm4_decrypt_block(block, key)
                } else {
                    aes128_ecb_decrypt_block(block, key)
                }
            })
            .collect::<Vec<_>>(),
        other => panic!("unsupported native data cipher {other}"),
    }
}

fn check_fat_metadata(
    raw: &mut NativeRawBlockDevice,
    start: u64,
    count: u64,
    sector: u32,
    boot: &[u8],
    key_mode: Option<([u8; 16], u8)>,
) {
    use edpcli::application::filesystem::FilesystemKind;
    let fs = detect_native_boot_sector(boot, count, sector)
        .unwrap()
        .unwrap();
    let fat_relative = if fs == FilesystemKind::ExFat {
        u64::from(u32::from_le_bytes(boot[80..84].try_into().unwrap()))
    } else {
        u64::from(u16::from_le_bytes(boot[14..16].try_into().unwrap()))
    };
    assert!(fat_relative > 0 && fat_relative < count);
    let fat_lba = start + fat_relative;
    let raw_fat = raw.read_block_fresh(fat_lba).unwrap();
    let decoded_fat = match key_mode {
        Some((key, mode)) => {
            let decoded = decode_native(&raw_fat, &key, mode, fat_lba, sector);
            assert_ne!(raw_fat, decoded, "encrypted FAT raw/decode mismatch");
            decoded
        }
        None => raw_fat,
    };
    assert_eq!(
        decoded_fat[0], 0xf8,
        "FAT reserved entry 0 media descriptor"
    );
    assert_eq!(&decoded_fat[1..4], &[0xff, 0xff, 0xff]);
    if fs == FilesystemKind::ExFat {
        assert_eq!(&decoded_fat[4..8], &[0xff, 0xff, 0xff, 0xff]);
    }
    let mut corrupted = boot.to_vec();
    corrupted[510] ^= 0xff;
    assert!(
        detect_native_boot_sector(&corrupted, count, sector)
            .unwrap()
            .is_none(),
        "corrupted native filesystem signature accepted"
    );
}

#[test]
#[ignore = "requires independently verified, caller-owned 512B/4Kn macOS Disk Image"]
fn formal_cli_native_crypto_readback() {
    let path = std::env::var("EDPCLI_CRYPTO_HIL_RAW").expect("owned raw disk path");
    assert!(
        path.starts_with("/dev/rdisk")
            && path["/dev/rdisk".len()..]
                .chars()
                .all(|ch| ch.is_ascii_digit()),
        "read-only verifier requires raw whole disk device"
    );
    let sector: u32 = std::env::var("EDPCLI_CRYPTO_HIL_SECTOR")
        .expect("native sector")
        .parse()
        .unwrap();
    assert!(matches!(sector, 512 | 4096));
    let expected = std::env::var("EDPCLI_CRYPTO_HIL_MODE").expect("mode");
    assert!(matches!(
        expected.as_str(),
        "plain" | "mode0" | "mode1" | "mode2" | "mode3"
    ));
    let bytes: u64 = std::env::var("EDPCLI_CRYPTO_HIL_BYTES")
        .expect("disk total bytes")
        .parse()
        .unwrap();
    assert_eq!(bytes, 536_870_912);
    assert_eq!(bytes % u64::from(sector), 0);
    let disk = platform::parse_disk_selector(&path).unwrap();
    let geometry = ObservedDeviceGeometry {
        capacity_bytes: bytes,
        logical_sector_bytes: Some(sector),
        physical_sector_bytes: None,
    }
    .native_read_geometry()
    .unwrap();
    let mut raw = NativeRawBlockDevice::open_readonly(&path, geometry).unwrap();
    assert_eq!(raw.sector_bytes(), sector);
    let total = bytes / u64::from(sector);

    if expected == "plain" {
        let mbr = raw.read_block_fresh(0).unwrap();
        assert_eq!(&mbr[510..512], &[0x55, 0xaa]);
        let start = u64::from(u32::from_le_bytes(mbr[454..458].try_into().unwrap()));
        let count = u64::from(u32::from_le_bytes(mbr[458..462].try_into().unwrap()));
        assert_eq!(start, 2048);
        let boot = raw.read_block_fresh(start).unwrap();
        assert_eq!(
            detect_native_boot_sector(&boot, count, sector).unwrap(),
            Some(edpcli::application::filesystem::FilesystemKind::ExFat)
        );
        println!("[P1] PASS sector={sector} plain exFAT native boot/MBR");
        check_fat_metadata(&mut raw, start, count, sector, &boot, None);
        return;
    }

    // Match --include-virtual discovery and its stable Disk Image probe.
    platform::set_include_virtual(true);
    let probe = system::native_provision_probe(&system::SysRunner, disk).unwrap();
    let device_id = TargetIdentity::from_probe(&probe, total)
        .unwrap()
        .device_id()
        .to_owned();
    let mut native_prefix = Vec::with_capacity(13 * sector as usize);
    for lba in 0..13 {
        native_prefix.extend_from_slice(&raw.read_block_fresh(lba).unwrap());
    }
    let projection = NativeProtocolImage::from_native_bytes(sector, native_prefix).unwrap();
    let parsed = parse_existing_provision_native(&projection, &device_id, total)
        .unwrap()
        .expect("formal CLI should create EDPF registration");
    assert_eq!(
        parsed.profile.partitions.len(),
        parsed.records.len(),
        "all native partition roles need independent decoded records"
    );
    assert_eq!(
        format!(
            "{:?}",
            DiskProvisionKind::from_mode(parsed.profile.source_mode)
        )
        .to_ascii_lowercase(),
        expected
    );
    // Reconstruct LCE location solely from freshly read and independently decoded
    // LBA7 entry pointers. Never assume a fixed native block index.
    let extent_count = 3072u64.div_ceil(u64::from(sector));
    let pointers = parsed
        .records
        .iter()
        .skip(1)
        .filter(|entry| {
            entry.lba7.partition_size == extent_count * u64::from(sector)
                && entry.lba7.sector_size == u64::from(sector)
        })
        .map(|entry| entry.lba7.start_sector)
        .collect::<Vec<_>>();
    assert!(!pointers.is_empty(), "LBA7 has no native LCE pointer");
    assert!(pointers.iter().all(|value| *value == pointers[0]));
    let lce_start = pointers[0];
    assert!(lce_start > 12 && lce_start + extent_count <= total);
    assert_eq!(
        extent_count * u64::from(sector),
        if sector == 512 { 3072 } else { 4096 }
    );
    let mut ciphertext = Vec::with_capacity(extent_count as usize * sector as usize);
    for lba in lce_start..lce_start + extent_count {
        ciphertext.extend(raw.read_block_fresh(lba).unwrap());
    }
    let plaintext = a6b0_full(&ciphertext, &[0; 8], lce_start * u64::from(sector));
    assert_eq!(&plaintext[..3072], edpcli::provision::lce_plaintext());
    assert!(plaintext[3072..].iter().all(|b| *b == 0));
    let wrong_lce = a6b0_full(&ciphertext, &[0; 8], (lce_start + 1) * u64::from(sector));
    assert_ne!(&wrong_lce[..3072], edpcli::provision::lce_plaintext());

    let mut decrypted_partitions = 0;
    let mut plaintext_partitions = 0;
    for (part, entry) in parsed.profile.partitions.iter().zip(&parsed.records) {
        if part.role == edpcli::provision::PartitionRole::CompatibilityReserve {
            continue;
        }
        let boot_raw = raw.read_block_fresh(part.start_lba).unwrap();
        let boot = if part.physically_encrypted {
            assert_ne!(
                entry.lba12.need_encrypt, 0,
                "encrypted role needs key record"
            );
            let material = entry.lba12_key_material().unwrap();
            let configured = if part.role == edpcli::provision::PartitionRole::Encrypt {
                std::env::var("EDPCLI_CRYPTO_HIL_ENCRYPT_PASSWORD").ok()
            } else {
                std::env::var("EDPCLI_CRYPTO_HIL_SHARE_PASSWORD").ok()
            };
            let password = configured
                .as_deref()
                .map(str::as_bytes)
                .unwrap_or(DEFAULT_KEY_DOMAIN_PASSWORD);
            let key = unwrap_file_key(Some(password), material)
                .expect("target password must independently unwrap the disk FileKey+CRC");
            if password != DEFAULT_KEY_DOMAIN_PASSWORD {
                assert!(
                    unwrap_file_key(Some(DEFAULT_KEY_DOMAIN_PASSWORD), material).is_err(),
                    "custom target password silently fell back to default"
                );
            }
            assert!(entry
                .verified_file_key(Some(b"definitely-wrong-password"))
                .is_err());
            let mut bad_crc = material;
            bad_crc.file_key_crc ^= 1;
            assert!(unwrap_file_key(Some(password), bad_crc).is_err());
            let mut bad_mode = material;
            bad_mode.encrypt_mode = FileKeyWrapMode::Aes128Ecb;
            if material.encrypt_mode != bad_mode.encrypt_mode {
                assert!(unwrap_file_key(Some(password), bad_mode).is_err());
            }
            assert!(FileKeyWrapMode::from_raw(255).is_none());
            unwrap_legacy_lba7_file_key(password, entry.lba7_key_material())
                .expect("legacy LBA7 key+CRC");
            // Decode from public, independent protocol crypto primitives, rather
            // than reusing the native writer's sector transformation helper.
            let decrypted = decode_native(
                &boot_raw,
                &key,
                entry.lba12.encrypt_mode,
                part.start_lba,
                sector,
            );
            assert!(
                detect_native_boot_sector(&boot_raw, part.sector_count, sector)
                    .ok()
                    .flatten()
                    .is_none(),
                "encrypted partition raw block unexpectedly has a filesystem signature"
            );
            decrypted_partitions += 1;
            decrypted
        } else {
            // mode1's BootShareCombined retains key-domain metadata but its
            // physical bytes must remain plaintext. Verify BOTH properties.
            if part.role == edpcli::provision::PartitionRole::BootShareCombined {
                assert_ne!(
                    entry.lba12.need_encrypt, 0,
                    "combined role needs password domain"
                );
                let password = std::env::var("EDPCLI_CRYPTO_HIL_SHARE_PASSWORD")
                    .unwrap_or_else(|_| "0000aaaa".to_owned());
                let password = password.as_bytes();
                entry
                    .verified_file_key(Some(password))
                    .expect("combined plaintext role still has a valid wrapped FileKey");
                assert!(entry
                    .verified_file_key(Some(b"definitely-wrong-password"))
                    .is_err());
                unwrap_legacy_lba7_file_key(password, entry.lba7_key_material())
                    .expect("combined role legacy key+CRC");
            }
            plaintext_partitions += 1;
            boot_raw
        };
        let fs = detect_native_boot_sector(&boot, part.sector_count, sector)
            .unwrap()
            .expect("decrypted native filesystem boot must parse");
        let decoded_key = if part.physically_encrypted {
            let configured = if part.role == edpcli::provision::PartitionRole::Encrypt {
                std::env::var("EDPCLI_CRYPTO_HIL_ENCRYPT_PASSWORD").ok()
            } else {
                std::env::var("EDPCLI_CRYPTO_HIL_SHARE_PASSWORD").ok()
            };
            let password = configured
                .as_deref()
                .map(str::as_bytes)
                .unwrap_or(DEFAULT_KEY_DOMAIN_PASSWORD);
            Some((
                entry.verified_file_key(Some(password)).unwrap(),
                entry.lba12.encrypt_mode,
            ))
        } else {
            assert_eq!(raw.read_block_fresh(part.start_lba).unwrap(), boot);
            None
        };
        check_fat_metadata(
            &mut raw,
            part.start_lba,
            part.sector_count,
            sector,
            &boot,
            decoded_key,
        );
        assert!(
            matches!(
                fs,
                edpcli::application::filesystem::FilesystemKind::Fat16
                    | edpcli::application::filesystem::FilesystemKind::Fat32
                    | edpcli::application::filesystem::FilesystemKind::ExFat
            ),
            "unsupported formatted filesystem {fs:?}"
        );
        println!(
            "[P1] verified native role={:?} fs={:?} encrypted={}",
            part.role, fs, part.physically_encrypted
        );
    }
    assert!(
        decrypted_partitions + plaintext_partitions > 0,
        "official target must have formatted data"
    );
    println!(
        "[P1] PASS sector={sector} target={expected} native-OS registered LBA7/LBA12, LCE={} blocks, decrypted={} plaintext={}",
        extent_count, decrypted_partitions, plaintext_partitions
    );
}
