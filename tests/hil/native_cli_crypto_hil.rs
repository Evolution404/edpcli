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

#[test]
#[ignore = "requires caller-proven disposable OS Disk Image, read-only full native extent SHA256"]
fn native_cli_full_extent_preservation_sha256() {
    use sha2::{Digest, Sha256};
    use std::fs::{self, File};
    use std::io::{Read, Seek, SeekFrom};

    let path = std::env::var("EDPCLI_CRYPTO_HIL_RAW").unwrap();
    assert!(
        path.starts_with("/dev/rdisk")
            && path["/dev/rdisk".len()..]
                .chars()
                .all(|ch| ch.is_ascii_digit())
    );
    let sector: u32 = std::env::var("EDPCLI_CRYPTO_HIL_SECTOR")
        .unwrap()
        .parse()
        .unwrap();
    assert!(matches!(sector, 512 | 1024 | 2048 | 4096));
    let size: u64 = std::env::var("EDPCLI_CRYPTO_HIL_BYTES")
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(size, 536_870_912);
    let snapshot = std::env::var("EDPCLI_PRESERVE_HIL_SNAPSHOT").unwrap();
    let phase = std::env::var("EDPCLI_PRESERVE_HIL_PHASE").unwrap();
    assert!(matches!(phase.as_str(), "capture" | "verify"));

    platform::set_include_virtual(true);
    let disk = platform::parse_disk_selector(&path).unwrap();
    let probe = system::native_provision_probe(&system::SysRunner, disk).unwrap();
    let device_id = TargetIdentity::from_probe(&probe, size / u64::from(sector))
        .unwrap()
        .device_id()
        .to_owned();
    let geometry = ObservedDeviceGeometry {
        capacity_bytes: size,
        logical_sector_bytes: Some(sector),
        physical_sector_bytes: None,
    }
    .native_read_geometry()
    .unwrap();
    let mut dev = NativeRawBlockDevice::open_readonly(&path, geometry).unwrap();
    let mut prefix = Vec::with_capacity(13 * sector as usize);
    for lba in 0..13 {
        prefix.extend_from_slice(&dev.read_block_fresh(lba).unwrap());
    }
    let native = NativeProtocolImage::from_native_bytes(sector, prefix).unwrap();
    let parsed = parse_existing_provision_native(&native, &device_id, size / u64::from(sector))
        .unwrap()
        .unwrap();
    assert_eq!(
        parsed.profile.source_mode,
        edpcli::provision::OfficialPartitionMode::DefaultThreePartition
    );

    // Stream the *entire* source partition ranges. No reliance on an MBR
    // projection that exposes only the OEM boot partition.
    let mut raw = File::open(&path).unwrap();
    let mut buffer = vec![0; 1024 * 1024];
    let mut hash_extent = |start: u64, count: u64| -> String {
        let start_bytes = start.checked_mul(u64::from(sector)).unwrap();
        let mut remaining = count.checked_mul(u64::from(sector)).unwrap();
        assert!(start_bytes.checked_add(remaining).unwrap() <= size);
        raw.seek(SeekFrom::Start(start_bytes)).unwrap();
        let mut digest = Sha256::new();
        while remaining > 0 {
            let chunk = remaining.min(buffer.len() as u64) as usize;
            raw.read_exact(&mut buffer[..chunk]).unwrap();
            digest.update(&buffer[..chunk]);
            remaining -= chunk as u64;
        }
        digest
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    };
    let mut summary = format!("identity={device_id} sector={sector} size={size}\n");
    let lce_count = 3072u64.div_ceil(u64::from(sector));
    let lce_start = parsed
        .records
        .iter()
        .skip(1)
        .find(|record| {
            record.lba7.sector_size == u64::from(sector)
                && record.lba7.partition_size == lce_count * u64::from(sector)
        })
        .expect("LCE source pointer")
        .lba7
        .start_sector;
    for (part, record) in parsed.profile.partitions.iter().zip(&parsed.records) {
        let extent_sha = hash_extent(part.start_lba, part.sector_count);
        let password = if part.role == edpcli::provision::PartitionRole::Encrypt {
            std::env::var("EDPCLI_CRYPTO_HIL_ENCRYPT_PASSWORD").unwrap()
        } else {
            std::env::var("EDPCLI_CRYPTO_HIL_SHARE_PASSWORD").unwrap()
        };
        let key_digest = if part.physically_encrypted {
            // A batch HIL run may retain the previous sector's test-only
            // password in its shell. Verify the default and the explicitly
            // configured candidate; neither bypasses FileKey authentication.
            let key = record
                .verified_file_key(Some(password.as_bytes()))
                .or_else(|_| record.verified_file_key(Some(DEFAULT_KEY_DOMAIN_PASSWORD)))
                .expect("known test password must independently verify FileKey");
            Sha256::digest(key)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        } else {
            "not-encrypted".to_owned()
        };
        summary.push_str(&format!(
            "{:?}:{}:{}:{}:{}\n",
            part.role, part.start_lba, part.sector_count, extent_sha, key_digest
        ));
        println!(
            "[preserve] {sector}B {:?} SHA256={} blocks={}",
            part.role, extent_sha, part.sector_count
        );
    }
    summary.push_str(&format!(
        "lce:{}:{}:{}\n",
        lce_start,
        lce_count,
        hash_extent(lce_start, lce_count)
    ));
    if phase == "capture" {
        fs::write(&snapshot, summary).unwrap();
        println!("[preserve] captured complete native extents");
    } else {
        let expected = fs::read_to_string(&snapshot).unwrap();
        assert_eq!(
            summary, expected,
            "native source full extent, LCE or original FileKey changed"
        );
        println!("[preserve] PASS full native extents + LCE + FileKeys unchanged");
    }
}

#[test]
#[ignore = "read-only whole-preserved-encrypted-partition hash on guarded OS Disk Image"]
fn native_os_cross_mode_encrypt_full_extent_sha256() {
    use sha2::{Digest, Sha256};
    use std::fs::{self, File};
    use std::io::{Read, Seek, SeekFrom};
    let raw = std::env::var("EDPCLI_CRYPTO_HIL_RAW").unwrap();
    assert!(
        raw.starts_with("/dev/rdisk")
            && raw["/dev/rdisk".len()..]
                .chars()
                .all(|c| c.is_ascii_digit())
    );
    let sector: u32 = std::env::var("EDPCLI_CRYPTO_HIL_SECTOR")
        .unwrap()
        .parse()
        .unwrap();
    assert!(matches!(sector, 512 | 1024 | 2048 | 4096));
    let snapshot = std::env::var("EDPCLI_CROSS_HIL_SNAPSHOT").unwrap();
    let phase = std::env::var("EDPCLI_CROSS_HIL_PHASE").unwrap();
    assert!(matches!(phase.as_str(), "capture" | "verify"));
    let capacity = 536_870_912u64;
    platform::set_include_virtual(true);
    let disk = platform::parse_disk_selector(&raw).unwrap();
    assert!(platform::confirmed_virtual_disk_image(
        &system::SysRunner,
        disk
    ));
    let geometry = ObservedDeviceGeometry {
        capacity_bytes: capacity,
        logical_sector_bytes: Some(sector),
        physical_sector_bytes: None,
    }
    .native_read_geometry()
    .unwrap();
    assert_eq!(
        system::device_geometry(&system::SysRunner, disk)
            .unwrap()
            .native_read_geometry()
            .unwrap(),
        geometry
    );
    let did = TargetIdentity::from_probe(
        &system::native_provision_probe(&system::SysRunner, disk).unwrap(),
        geometry.native_sector_count,
    )
    .unwrap()
    .device_id()
    .to_owned();
    let mut dev = NativeRawBlockDevice::open_readonly(&raw, geometry).unwrap();
    let mut prefix = Vec::with_capacity(13 * sector as usize);
    for lba in 0..13 {
        prefix.extend_from_slice(&dev.read_block_fresh(lba).unwrap());
    }
    let projected = NativeProtocolImage::from_native_bytes(sector, prefix).unwrap();
    let parsed = parse_existing_provision_native(&projected, &did, geometry.native_sector_count)
        .unwrap()
        .unwrap();
    let (part, record) = parsed
        .profile
        .partitions
        .iter()
        .zip(&parsed.records)
        .find(|(part, _)| part.role == edpcli::provision::PartitionRole::Encrypt)
        .expect("Mode0/1/2 must retain encrypted volume");
    assert!(part.physically_encrypted);
    let file_key = record
        .verified_file_key(Some(DEFAULT_KEY_DOMAIN_PASSWORD))
        .expect("retained FileKey with unchanged source password");
    let key_digest = Sha256::digest(file_key);

    let mut file = File::open(&raw).unwrap();
    file.seek(SeekFrom::Start(part.start_lba * u64::from(sector)))
        .unwrap();
    let mut remaining = part.sector_count * u64::from(sector);
    let mut hasher = Sha256::new();
    let mut chunk = vec![0u8; 1024 * 1024];
    while remaining != 0 {
        let len = remaining.min(chunk.len() as u64) as usize;
        file.read_exact(&mut chunk[..len]).unwrap();
        hasher.update(&chunk[..len]);
        remaining -= len as u64;
    }
    let full_hash: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let key_hash: String = key_digest.iter().map(|b| format!("{b:02x}")).collect();
    let summary = format!(
        "sector={sector} start={} blocks={} full_sha256={full_hash} filekey_sha256={key_hash}\n",
        part.start_lba, part.sector_count,
    );
    if phase == "capture" {
        fs::write(snapshot, &summary).unwrap();
        println!("[cross] captured complete encrypted extent: {summary}");
    } else {
        assert_eq!(
            fs::read_to_string(&snapshot).unwrap(),
            summary,
            "cross-mode retained encrypted region bytes/geometry/FileKey changed"
        );
        println!("[cross] PASS complete retained encrypted extent and FileKey: {summary}");
    }
}
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
    if fs == FilesystemKind::Fat12 {
        // FAT12 first two 12-bit entries occupy precisely F8 FF FF.
        // There is no fourth reserved byte; byte 3 is the start of FAT[2].
        assert_eq!(&decoded_fat[..3], &[0xf8, 0xff, 0xff]);
        assert_eq!(&decoded_fat[3..4], &[0x00]);
    } else {
        assert_eq!(&decoded_fat[1..4], &[0xff, 0xff, 0xff]);
        if fs == FilesystemKind::ExFat {
            assert_eq!(&decoded_fat[4..8], &[0xff, 0xff, 0xff, 0xff]);
        }
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
#[ignore = "requires independently verified, caller-owned 512/1024/2048/4096B macOS Disk Image"]
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
    assert!(matches!(sector, 512 | 1024 | 2048 | 4096));
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
        3072u64.div_ceil(u64::from(sector)) * u64::from(sector)
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
        if part.role == edpcli::provision::PartitionRole::Boot {
            let expected_blocks = 10 * 1024 * 1024 / u64::from(sector) - 63;
            let expected_fs = if sector <= 2048 {
                edpcli::application::filesystem::FilesystemKind::Fat16
            } else {
                edpcli::application::filesystem::FilesystemKind::Fat12
            };
            assert_eq!(part.start_lba, 63, "OEM independent boot start");
            assert_eq!(part.sector_count, expected_blocks, "OEM 10MiB end boundary");
            assert_eq!(fs, expected_fs, "OEM native FAT cluster-selected type");
            assert_eq!(
                u16::from_le_bytes(boot[11..13].try_into().unwrap()),
                sector as u16
            );
            let mbr = raw.read_block_fresh(0).unwrap();
            assert_eq!(mbr[446 + 4], if sector <= 2048 { 0x0e } else { 0x01 });
            assert_eq!(u32::from_le_bytes(mbr[454..458].try_into().unwrap()), 63);
            assert_eq!(
                u32::from_le_bytes(mbr[458..462].try_into().unwrap()),
                expected_blocks as u32
            );
        }
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
                edpcli::application::filesystem::FilesystemKind::Fat12
                    | edpcli::application::filesystem::FilesystemKind::Fat16
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

    // A03: capture an actual OS Disk Image through the same read-only native
    // application backup service. Compare its verified v4 EDPB raw evidence to
    // independently reopened native blocks and produce a same-geometry preview.
    // No physical media write, restore grant or WAL replay is obtained here.
    if expected == "mode0" && sector > 512 {
        use edpcli::application::evidence::{EvidenceSource, SectorReader};
        use edpcli::edpb::{RestorePolicy, VerifiedBackupReader};
        use std::time::{SystemTime, UNIX_EPOCH};
        struct NoWritePrompt;
        impl edpcli::application::Prompter for NoWritePrompt {
            fn prompt_line(&mut self, _: &str) -> String {
                String::new()
            }
            fn prompt_secret(&mut self, _: &str) -> edpcli::provision::SecretBytes {
                edpcli::provision::SecretBytes::from_owned(Vec::new())
            }
            fn confirm_yes(&mut self, _: &str) -> bool {
                false
            }
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "edpcli-native-edpb-diskimage-capture-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let report = edpcli::application::write::backup_create_on_disk(
            &system::SysRunner,
            disk,
            directory.clone(),
            &mut NoWritePrompt,
            None,
            Some(&device_id),
        )
        .expect("actual native Disk Image read-only EDPB v4 capture");
        let verified = VerifiedBackupReader::open(&report.path).unwrap();
        assert_eq!(verified.verified().manifest.schema, "edpb.manifest.v4");
        assert!(verified
            .verified()
            .manifest
            .artifacts
            .iter()
            .all(|a| a.restore_policy == RestorePolicy::EvidenceOnly));
        let preview =
            edpcli::application::evidence::native_restore_preview::plan_native_restore_readonly(
                &report.path,
                &device_id,
                geometry,
            )
            .unwrap();
        assert_eq!(preview.logical_sector_bytes, sector);
        assert_eq!(preview.native_lce_start, lce_start);
        assert_eq!(preview.native_lce_blocks, extent_count);
        assert_eq!(preview.proposed_lbas_in_write_order.last(), Some(&0));
        let mut source = EvidenceSource::open_backup(&report.path).unwrap();
        for lba in preview.proposed_lbas_in_write_order {
            assert_eq!(
                source.read_native_sector(lba).unwrap(),
                raw.read_block_fresh(lba).unwrap(),
                "OS native LBA{lba} must agree with independently verified EDPB"
            );
        }
        std::fs::remove_file(&report.path).unwrap();
        std::fs::remove_dir(&directory).unwrap();
        println!("[A03] PASS sector={sector} actual OS native EDPB capture, complete LCE and partition header comparison; preview only");
    }
    println!(
        "[P1] PASS sector={sector} target={expected} native-OS registered LBA7/LBA12, LCE={} blocks, decrypted={} plaintext={}",
        extent_count, decrypted_partitions, plaintext_partitions
    );
}
