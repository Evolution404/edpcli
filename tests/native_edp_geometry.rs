//! Pure read-only EDP mode/geometry policy on native 4096B and 512B media.
use edpcli::{
    application::filesystem::FilesystemKind,
    protocol::{edpf::EdpPartitionType, image::NativeProtocolImage, lba7::Lba7PartitionMode},
    provision::{
        official_partition_semantics, NativeEdpLayoutPlan, NativeEdpWriteCapability,
        OfficialPartitionMode, PartitionRole, TargetPartitionGeometry,
    },
};

fn sample(
    mode: OfficialPartitionMode,
    block_bytes: u32,
) -> (u64, u64, Vec<TargetPartitionGeometry>) {
    let total = if block_bytes == 4096 {
        62_486_528
    } else {
        500_000
    };
    let lce_start = total - 9_967;
    let mut cursor = 63u64;
    let mut parts = Vec::new();
    for (index, ptype) in mode.partition_types().iter().copied().enumerate() {
        let semantics = official_partition_semantics(mode, index, ptype).unwrap();
        let count = match (mode, index, block_bytes) {
            (Lba7PartitionMode::DefaultThreePartition, 0, 4096) => 2_497,
            (Lba7PartitionMode::DefaultThreePartition, 1, 4096) => 49_976_864,
            (Lba7PartitionMode::DefaultThreePartition, 2, 4096) => 12_494_112,
            (Lba7PartitionMode::WholeDiskEncrypted, 0, 512) => 63,
            (Lba7PartitionMode::WholeDiskEncrypted, 0, 4096) => 8,
            (_, 0, _) => 2_497,
            (_, _, _) => 24_000,
        };
        let fs = match semantics.role {
            PartitionRole::CompatibilityReserve => None,
            PartitionRole::Boot => Some(FilesystemKind::Fat12),
            PartitionRole::BootShareCombined => Some(FilesystemKind::ExFat),
            PartitionRole::Share | PartitionRole::Encrypt => Some(FilesystemKind::ExFat),
        };
        parts.push(TargetPartitionGeometry {
            role: semantics.role,
            partition_type: ptype,
            start_lba: cursor,
            sector_count: count,
            physically_encrypted: semantics.physically_encrypted(),
            filesystem: fs,
        });
        cursor += count;
    }
    // Exact U391 mode0 recorded layout has a small gap between exchange and
    // encrypted sections; source-backed geometry may legitimately contain gaps.
    if block_bytes == 4096 && mode == Lba7PartitionMode::DefaultThreePartition {
        parts[2].start_lba = 49_979_648;
    }
    assert!(parts.last().unwrap().start_lba + parts.last().unwrap().sector_count < lce_start);
    (total, lce_start, parts)
}

#[test]
fn all_official_modes_have_native_source_only_plans_with_distinct_protocol_and_lce_units() {
    for native_bytes in [512u32, 4096] {
        for mode in [
            OfficialPartitionMode::DefaultThreePartition,
            OfficialPartitionMode::BootShareCombined,
            OfficialPartitionMode::WholeDiskEncrypted,
            OfficialPartitionMode::IntranetExtranetDualPartition,
        ] {
            let (total, lce_start, parts) = sample(mode, native_bytes);
            let lce_sectors = if native_bytes == 4096 { 1 } else { 6 };
            let plan = NativeEdpLayoutPlan::from_confirmed_geometry(
                mode,
                total,
                native_bytes,
                &parts,
                lce_start,
                lce_sectors,
            )
            .unwrap();
            assert_eq!(plan.mode, mode);
            assert_eq!(plan.protocol.start_lba, 0);
            assert_eq!(plan.protocol.sector_count, 13);
            assert_eq!(plan.reserved.start_lba, 13);
            assert_eq!(plan.reserved.sector_count, 50);
            assert_eq!(plan.protocol_projection_bytes(), 6656);
            assert_eq!(
                plan.native_protocol_bytes(),
                Some(13 * u64::from(native_bytes))
            );
            assert_eq!(plan.lce_payload_bytes(), 3072);
            assert_eq!(
                plan.lce_native_bytes(),
                Some(lce_sectors * u64::from(native_bytes))
            );
            assert_eq!(plan.partitions.len(), mode.partition_types().len());
            assert_eq!(
                plan.write_capability,
                NativeEdpWriteCapability::ReadOnlySourceGeometry
            );
            assert!(!plan.may_write());
            for (index, partition) in plan.partitions.iter().enumerate() {
                assert_eq!(partition.geometry, parts[index]);
                assert_eq!(
                    partition.semantics.partition_type,
                    mode.partition_types()[index]
                );
            }
            match mode {
                Lba7PartitionMode::WholeDiskEncrypted => {
                    assert_eq!(plan.visible_mbr_type, 0x0b);
                    assert_eq!(parts[0].role, PartitionRole::CompatibilityReserve);
                    if native_bytes == 4096 {
                        assert_ne!(parts[0].sector_count * 4096, 0x7e00);
                        // Mode2 0x7E00 historical 512B byte count isn't
                        // representable as complete 4096B blocks. Do not
                        // certify any new protocol writer on this evidence.
                    } else {
                        assert_eq!(parts[0].sector_count * 512, 0x7e00);
                    }
                }
                Lba7PartitionMode::BootShareCombined => {
                    let combined = &plan.partitions[0].semantics;
                    assert!(combined.protocol_need_encrypt);
                    assert!(!combined.physically_encrypted());
                    assert_eq!(combined.role, PartitionRole::BootShareCombined);
                }
                _ => {}
            }
        }
    }
}

#[test]
fn native_edp_source_mode0_u391_sector_boundaries_and_projection_tails_preserved() {
    let mode = OfficialPartitionMode::DefaultThreePartition;
    let (total, lce_start, parts) = sample(mode, 4096);
    assert_eq!(parts[0].start_lba, 63);
    assert_eq!(parts[0].sector_count, 2497);
    assert_eq!(parts[1].start_lba, 2560);
    assert_eq!(parts[1].sector_count, 49_976_864);
    assert_eq!(parts[2].start_lba, 49_979_648);
    assert_eq!(parts[2].sector_count, 12_494_112);
    let plan =
        NativeEdpLayoutPlan::from_confirmed_geometry(mode, total, 4096, &parts, lce_start, 1)
            .unwrap();
    assert_eq!(plan.native_protocol_bytes(), Some(53_248));
    assert_eq!(plan.lce_native_bytes(), Some(4096));
    // Synthetic bytes stand in for a source-backed protocol capture; these
    // do NOT pretend to be an authentic manufacturer's protocol golden.
    let raw = (0..13 * 4096)
        .map(|index| ((index / 4096) * 13 + (index % 4096) / 512) as u8)
        .collect::<Vec<_>>();
    let native = NativeProtocolImage::from_native_bytes(4096, raw.clone()).unwrap();
    let projection = native.protocol_projection();
    assert_eq!(projection.len(), 6656);
    for lba in 0..13 {
        assert_eq!(
            &projection[lba * 512..(lba + 1) * 512],
            &raw[lba * 4096..lba * 4096 + 512]
        );
        assert_eq!(
            native.block(lba).unwrap(),
            &raw[lba * 4096..(lba + 1) * 4096]
        );
    }
    let mut modified = native;
    let new_protocol = [0x5Au8; 6656];
    modified
        .overlay_protocol_preserve_native_tail(&new_protocol)
        .unwrap();
    assert_eq!(modified.protocol_projection(), new_protocol);
    for lba in 0..13 {
        assert_eq!(
            &modified.block(lba).unwrap()[512..],
            &raw[lba * 4096 + 512..(lba + 1) * 4096]
        );
    }
}

#[test]
fn native_edp_rejects_unverified_geometry_and_role_mismatches() {
    let mode = OfficialPartitionMode::DefaultThreePartition;
    let (total, lce_start, parts) = sample(mode, 4096);
    let valid = |parts: &[TargetPartitionGeometry], total, size, lce, count| {
        NativeEdpLayoutPlan::from_confirmed_geometry(mode, total, size, parts, lce, count)
    };
    for block_bytes in [0, 256, 1024, 2048, 8192] {
        assert!(valid(&parts, total, block_bytes, lce_start, 1).is_err());
    }
    assert!(valid(&parts, 0, 4096, lce_start, 1).is_err());
    assert!(valid(&parts, u64::MAX, 4096, lce_start, 1).is_err());
    assert!(valid(&parts, total, 4096, lce_start, 6).is_err());
    assert!(valid(&parts, total, 4096, 7, 1).is_err());
    assert!(valid(&parts, total, 4096, total, 1).is_err());
    assert!(valid(&parts[..2], total, 4096, lce_start, 1).is_err());
    let mut bad = parts.clone();
    bad[0].partition_type = EdpPartitionType::Share;
    assert!(valid(&bad, total, 4096, lce_start, 1).is_err());
    bad = parts.clone();
    bad[0].role = PartitionRole::Share;
    assert!(valid(&bad, total, 4096, lce_start, 1).is_err());
    bad = parts.clone();
    bad[1].physically_encrypted = false;
    assert!(valid(&bad, total, 4096, lce_start, 1).is_err());
    bad = parts.clone();
    bad[1].filesystem = None;
    assert!(valid(&bad, total, 4096, lce_start, 1).is_err());
    bad = parts.clone();
    bad[1].start_lba = bad[0].start_lba + 1;
    assert!(valid(&bad, total, 4096, lce_start, 1).is_err());
    bad = parts.clone();
    bad[2].start_lba = lce_start;
    assert!(valid(&bad, total, 4096, lce_start, 1).is_err());
    bad = parts.clone();
    bad[2].sector_count = total;
    assert!(valid(&bad, total, 4096, lce_start, 1).is_err());
    bad = parts.clone();
    bad[2].sector_count = 0;
    assert!(valid(&bad, total, 4096, lce_start, 1).is_err());
}

#[test]
fn native_edp_replay_copies_complete_source_blocks_and_lce_without_rewriting_opaque_tails() {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Seek, SeekFrom, Write};
    let mode = OfficialPartitionMode::IntranetExtranetDualPartition;
    for native_bytes in [512u32, 4096] {
        let partitions = vec![
            TargetPartitionGeometry {
                role: PartitionRole::Boot,
                partition_type: EdpPartitionType::Boot,
                start_lba: 63,
                sector_count: 2_497,
                physically_encrypted: false,
                filesystem: Some(FilesystemKind::Fat12),
            },
            TargetPartitionGeometry {
                role: PartitionRole::Share,
                partition_type: EdpPartitionType::Share,
                start_lba: 2_560,
                sector_count: 16_000,
                physically_encrypted: true,
                filesystem: Some(FilesystemKind::ExFat),
            },
        ];
        let lce_count = if native_bytes == 4096 { 1usize } else { 6 };
        let layout = NativeEdpLayoutPlan::from_confirmed_geometry(
            mode,
            50_000,
            native_bytes,
            &partitions,
            40_000,
            lce_count as u64,
        )
        .unwrap();
        let size = native_bytes as usize;
        let mut raw = vec![0u8; 13 * size];
        for lba in 0..13 {
            let block = &mut raw[lba * size..(lba + 1) * size];
            block.fill((lba as u8) + 0x30);
        }
        let mbr = &mut raw[..size];
        mbr[446 + 4] = layout.visible_mbr_type;
        mbr[454..458].copy_from_slice(&63u32.to_le_bytes());
        mbr[458..462].copy_from_slice(&2497u32.to_le_bytes());
        mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
        let native = NativeProtocolImage::from_native_bytes(native_bytes, raw.clone()).unwrap();
        let lce = (0..lce_count)
            .map(|i| vec![0xce + i as u8; size])
            .collect::<Vec<_>>();
        let writes = layout.source_replay_native_blocks(&native, &lce).unwrap();
        assert_eq!(writes.len(), 13 + lce_count);
        assert_eq!(writes.last().unwrap().relative_lba, 0);
        assert_eq!(writes[0].relative_lba, 1);
        for (lba, block) in (1..13).zip(&writes[..12]) {
            assert_eq!(block.relative_lba, lba);
            assert_eq!(
                block.data,
                raw[lba as usize * size..(lba as usize + 1) * size]
            );
        }
        for (offset, block) in writes[12..writes.len() - 1].iter().enumerate() {
            assert_eq!(block.relative_lba, 40_000 + offset as u64);
            assert_eq!(block.data, lce[offset]);
        }
        assert_eq!(writes.last().unwrap().data, raw[..size]);
        assert!(!layout.may_write());

        // Test only a disposable ordinary file and never a /dev path.
        if native_bytes == 4096 {
            let path = std::env::temp_dir().join(format!(
                "edpcli-native-edp-replay-{}-{}.img",
                std::process::id(),
                native_bytes
            ));
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            assert!(file.metadata().unwrap().file_type().is_file());
            file.set_len(50_000 * u64::from(native_bytes)).unwrap();
            for write in &writes[..writes.len() - 1] {
                file.seek(SeekFrom::Start(
                    write.relative_lba * u64::from(native_bytes),
                ))
                .unwrap();
                file.write_all(&write.data).unwrap();
            }
            file.sync_all().unwrap();
            let mut pending_mbr = vec![0u8; size];
            file.seek(SeekFrom::Start(0)).unwrap();
            file.read_exact(&mut pending_mbr).unwrap();
            assert!(pending_mbr.iter().all(|&b| b == 0));
            let mbr_block = writes.last().unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            file.write_all(&mbr_block.data).unwrap();
            file.sync_all().unwrap();
            drop(file);
            let mut readback = std::fs::File::open(&path).unwrap();
            for write in &writes {
                let mut block = vec![0u8; size];
                readback
                    .seek(SeekFrom::Start(
                        write.relative_lba * u64::from(native_bytes),
                    ))
                    .unwrap();
                readback.read_exact(&mut block).unwrap();
                assert_eq!(Sha256::digest(&block)[..], Sha256::digest(&write.data)[..]);
            }
            drop(readback);
            std::fs::remove_file(&path).unwrap();
        }

        let mut invalid = raw.clone();
        invalid[510] = 0;
        let bad = NativeProtocolImage::from_native_bytes(native_bytes, invalid).unwrap();
        assert!(layout.source_replay_native_blocks(&bad, &lce).is_err());
        let mut invalid = raw.clone();
        invalid[458..462].copy_from_slice(&42u32.to_le_bytes());
        let bad = NativeProtocolImage::from_native_bytes(native_bytes, invalid).unwrap();
        assert!(layout.source_replay_native_blocks(&bad, &lce).is_err());
        let mut invalid = raw.clone();
        invalid[446 + 4] ^= 1;
        let bad = NativeProtocolImage::from_native_bytes(native_bytes, invalid).unwrap();
        assert!(layout.source_replay_native_blocks(&bad, &lce).is_err());
        let mut invalid_lce = lce.clone();
        invalid_lce[0].truncate(size - 1);
        assert!(layout
            .source_replay_native_blocks(&native, &invalid_lce)
            .is_err());
        assert!(layout
            .source_replay_native_blocks(&native, &lce[..lce_count - 1])
            .is_err());
    }
}

/// Matrix of already-supported *offline* native-sector transforms. This tests
/// data cipher semantics, NOT manufacturer's native4Kn protocol production,
/// password validity, secure hardware callback selection, or USB writes.
#[test]
fn offline_cipher_matrix_four_layouts_two_native_sector_widths() {
    use edpcli::application::inspect::{
        transform_native_sector_offline, NativeCipherDirection, NativePartitionDataCipher,
    };

    let key = std::array::from_fn(|index| (index as u8).wrapping_add(7));
    let mut visited = 0usize;
    let mut unencrypted = 0usize;
    for native_bytes in [512u32, 4096] {
        for mode in [
            OfficialPartitionMode::DefaultThreePartition,
            OfficialPartitionMode::BootShareCombined,
            OfficialPartitionMode::WholeDiskEncrypted,
            OfficialPartitionMode::IntranetExtranetDualPartition,
        ] {
            let (total, lce_start, parts) = sample(mode, native_bytes);
            let lce_count = if native_bytes == 4096 { 1 } else { 6 };
            let plan = NativeEdpLayoutPlan::from_confirmed_geometry(
                mode, total, native_bytes, &parts, lce_start, lce_count
            ).unwrap();
            assert!(!plan.may_write());
            for partition in &plan.partitions {
                let sample_plain = (0..native_bytes as usize)
                    .map(|i| (i as u8).wrapping_mul(29).wrapping_add(11))
                    .collect::<Vec<_>>();
                if !partition.semantics.physically_encrypted() {
                    // Mode1 combined BootShare has NeedEncrypt protocol set,
                    // but is physically PLAINTEXT. Never infer encryption from
                    // LBA12 NeedEncrypt alone.
                    assert_eq!(sample_plain.len(), native_bytes as usize);
                    unencrypted += 1;
                    continue;
                }
                for encrypt_mode in [1, 2, 3] {
                    let algorithm =
                        NativePartitionDataCipher::from_encrypt_mode(encrypt_mode).unwrap();
                    let ciphertext = transform_native_sector_offline(
                        algorithm, NativeCipherDirection::Encrypt, &sample_plain, &key,
                        partition.geometry.start_lba, native_bytes
                    ).unwrap();
                    assert_ne!(ciphertext, sample_plain);
                    let recovered = transform_native_sector_offline(
                        algorithm, NativeCipherDirection::Decrypt, &ciphertext, &key,
                        partition.geometry.start_lba, native_bytes
                    ).unwrap();
                    assert_eq!(recovered, sample_plain);
                    visited += 1;
                }
            }
            for not_supported in [0, 4, 255] {
                assert!(NativePartitionDataCipher::from_encrypt_mode(not_supported).is_err());
            }
        }
    }
    assert!(visited >= 24, "the matrix must exercise encrypted regions");
    assert!(unencrypted >= 4, "the matrix must exercise physical plaintext regions");
}
