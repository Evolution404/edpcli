use crate::common;
use crate::gold_name;

use edpcli::backup_deep::{assess_partition, AnalysisStatus};
use edpcli::backup_metadata::PartitionGeometry;
use edpcli::edpb::{ArtifactCompleteness, RestorePolicy};

fn partition() -> PartitionGeometry {
    PartitionGeometry {
        index: 1,
        partition_type: 2,
        partition_count: 3,
        need_disturb: 1,
        need_encrypt: 1,
        start_sector: 20480,
        sector_size: 512,
        partition_size: 118477684736,
        sector_count: 231401728,
        user_key_crc: 69825373,
        file_key_crc: 1099700748,
        encrypt_mode: 2,
    }
}

#[test]
fn encrypted_metadata_never_becomes_an_empty_filesystem() {
    let p = partition();
    // Even a coincidental plaintext signature is not a verified decoded view.
    let mut raw = vec![0; 512];
    raw[3..11].copy_from_slice(b"NTFS    ");
    let report = assess_partition(&p, Some(&raw));
    assert_eq!(report.status, AnalysisStatus::Locked);
    assert!(report.filesystem.is_none());
    assert!(report.total_bytes.is_none());
    assert!(report.used_bytes.is_none());
    assert!(report.free_bytes.is_none());
    assert!(report.file_count.is_none());
    assert!(report.directory_count.is_none());
    assert!(report.entries.is_none());
    assert_eq!(report.partition_bytes, p.partition_size);
    let artifact = report.into_artifact().unwrap();
    assert_eq!(artifact.restore_policy, RestorePolicy::DerivedOnly);
    assert_eq!(artifact.completeness, ArtifactCompleteness::NotCaptured);
    assert_eq!(
        artifact.derivation.unwrap().source_artifact_ids,
        ["raw.protocol.lba0_12", "raw.partition.1.prefix"]
    );
    let json: serde_json::Value = serde_json::from_slice(&artifact.data).unwrap();
    assert!(json["entries"].is_null());
    assert_eq!(json["status"], "locked");
}

#[test]
fn missing_and_short_prefixes_are_explicit_and_never_zero_filled() {
    let p = partition();
    let missing = assess_partition(&p, None);
    assert_eq!(missing.status, AnalysisStatus::NotCaptured);
    let artifact = missing.into_artifact().unwrap();
    assert_eq!(artifact.source_extent_ids, ["extent.protocol.lba0_12"]);
    assert_eq!(
        assess_partition(&p, Some(&[0; 511])).status,
        AnalysisStatus::ParseFailed
    );
}

#[test]
fn plaintext_metadata_does_not_claim_a_complete_inventory() {
    let mut p = partition();
    p.need_encrypt = 0;
    p.need_disturb = 0;
    p.partition_type = 4;
    let mut raw = [0; 512];
    raw[3..11].copy_from_slice(b"NTFS    ");
    let report = assess_partition(&p, Some(&raw));
    assert_eq!(report.status, AnalysisStatus::Unsupported);
    assert!(report.entries.is_none());
    p.need_disturb = 1;
    assert_eq!(
        assess_partition(&p, Some(&raw)).status,
        AnalysisStatus::Unsupported
    );
}

use edpcli::backup_deep::{analyze_partition, PartitionReader};
use std::{collections::BTreeMap, io};

struct SparseReader {
    sectors: BTreeMap<u64, Vec<u8>>,
    reads: Vec<u64>,
}
impl PartitionReader for SparseReader {
    fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        self.reads.push(lba);
        self.sectors
            .get(&lba)
            .cloned()
            .ok_or_else(|| io::Error::other("uncaptured sector"))
    }
}
fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(b: &mut [u8], at: usize, v: u64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
fn entry(name: &[u8; 11], cluster: u16, size: u32, dir: bool) -> [u8; 32] {
    let mut e = [0; 32];
    e[..11].copy_from_slice(name);
    e[11] = if dir { 16 } else { 32 };
    e[12] = 24;
    put16(&mut e, 26, cluster);
    put32(&mut e, 28, size);
    e
}
fn fat_fixture(fat32: bool) -> (PartitionGeometry, SparseReader, u64) {
    let (clusters, fat_sectors, reserved, root_sectors) = if fat32 {
        (65525u32, 512u32, 32u32, 0u32)
    } else {
        (4085, 16, 1, 1)
    };
    let total = reserved + fat_sectors + root_sectors + clusters;
    let mut p = partition();
    p.need_encrypt = 0;
    p.need_disturb = 0;
    p.sector_count = total as u64;
    p.partition_size = p.sector_count * 512;
    let mut boot = vec![0; 512];
    boot[0] = 0xeb;
    put16(&mut boot, 11, 512);
    boot[13] = 1;
    put16(&mut boot, 14, reserved as u16);
    boot[16] = 1;
    boot[21] = 0xf8;
    put32(&mut boot, 32, total);
    boot[510] = 0x55;
    boot[511] = 0xaa;
    if fat32 {
        put32(&mut boot, 36, fat_sectors);
        put32(&mut boot, 44, 2);
        boot[82..90].copy_from_slice(b"FAT32   ");
    } else {
        put16(&mut boot, 17, 16);
        put16(&mut boot, 22, fat_sectors as u16);
        boot[54..62].copy_from_slice(b"FAT16   ");
    }
    let mut r = SparseReader {
        sectors: BTreeMap::new(),
        reads: vec![],
    };
    r.sectors.insert(0, boot);
    let mut fat = vec![0; fat_sectors as usize * 512];
    for c in 0..=5 {
        if fat32 {
            put32(&mut fat, c * 4, 0x0fffffff);
        } else {
            put16(&mut fat, c * 2, 0xffff);
        }
    }
    for (i, s) in fat.chunks(512).enumerate() {
        r.sectors.insert(reserved as u64 + i as u64, s.to_vec());
    }
    let data = (reserved + fat_sectors + root_sectors) as u64;
    let root = if fat32 { data } else { data - 1 };
    let mut dir = vec![0; 512];
    dir[..32].copy_from_slice(&entry(b"FOO     TXT", 3, 5, false));
    dir[32..64].copy_from_slice(&entry(b"DIR        ", 4, 0, true));
    r.sectors.insert(root, dir);
    let mut sub = vec![0; 512];
    sub[..32].copy_from_slice(&entry(b"BAR     BIN", 5, 7, false));
    r.sectors.insert(data + 2, sub);
    // File data clusters 3 and 5 deliberately absent: parser must not read them.
    (p, r, data)
}

fn exfat_set_checksum(bytes: &[u8]) -> u16 {
    bytes
        .iter()
        .enumerate()
        .filter(|(index, _)| !matches!(index, 2 | 3))
        .fold(0u16, |sum, (_, &byte)| {
            sum.rotate_right(1).wrapping_add(byte as u16)
        })
}

fn exfat_file_set(name: &str, cluster: u32, size: u64, directory: bool) -> Vec<u8> {
    let units = name.encode_utf16().collect::<Vec<_>>();
    let name_entries = units.len().div_ceil(15);
    let mut set = vec![0u8; (2 + name_entries) * 32];
    set[0] = 0x85;
    set[1] = (1 + name_entries) as u8;
    put16(&mut set, 4, if directory { 0x10 } else { 0x20 });
    let stream = &mut set[32..64];
    stream[0] = 0xc0;
    stream[1] = 0x03; // allocation possible + no FAT chain
    stream[3] = units.len() as u8;
    put64(stream, 8, size);
    put32(stream, 20, cluster);
    put64(stream, 24, size);
    for (entry_index, chunk) in units.chunks(15).enumerate() {
        let entry = &mut set[(2 + entry_index) * 32..(3 + entry_index) * 32];
        entry[0] = 0xc1;
        for (index, value) in chunk.iter().enumerate() {
            put16(entry, 2 + index * 2, *value);
        }
    }
    let checksum = exfat_set_checksum(&set);
    put16(&mut set, 2, checksum);
    set
}

fn exfat_boot_checksum(sectors: &[Vec<u8>]) -> u32 {
    let mut checksum = 0u32;
    for (sector_index, sector) in sectors.iter().take(11).enumerate() {
        for (offset, &byte) in sector.iter().enumerate() {
            if sector_index == 0 && matches!(offset, 106 | 107 | 112) {
                continue;
            }
            checksum = checksum.rotate_right(1).wrapping_add(byte as u32);
        }
    }
    checksum
}

fn exfat_fixture() -> (PartitionGeometry, SparseReader, u64, [u64; 2]) {
    let volume_sectors = 96u64;
    let fat_offset = 24u32;
    let heap_offset = 32u32;
    let clusters = 64u32;
    let root_cluster = 2u32;
    let mut p = partition();
    p.need_encrypt = 0;
    p.need_disturb = 0;
    p.sector_count = volume_sectors;
    p.partition_size = volume_sectors * 512;

    let mut sectors = BTreeMap::new();
    let mut boot = vec![0u8; 512];
    boot[0..3].copy_from_slice(&[0xeb, 0x76, 0x90]);
    boot[3..11].copy_from_slice(b"EXFAT   ");
    put64(&mut boot, 72, volume_sectors);
    put32(&mut boot, 80, fat_offset);
    put32(&mut boot, 84, 1);
    put32(&mut boot, 88, heap_offset);
    put32(&mut boot, 92, clusters);
    put32(&mut boot, 96, root_cluster);
    put16(&mut boot, 104, 0x0100);
    boot[108] = 9;
    boot[109] = 0;
    boot[110] = 1;
    boot[111] = 0x80;
    boot[112] = 8;
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    let mut boot_region = vec![boot.clone()];
    for _ in 1..=10 {
        boot_region.push(vec![0; 512]);
    }
    let checksum = exfat_boot_checksum(&boot_region);
    let mut checksum_sector = vec![0u8; 512];
    for chunk in checksum_sector.as_chunks_mut::<4>().0 {
        chunk.copy_from_slice(&checksum.to_le_bytes());
    }
    sectors.insert(0, boot);
    for sector in 1..=10 {
        sectors.insert(sector, vec![0; 512]);
    }
    sectors.insert(11, checksum_sector);

    let mut fat = vec![0u8; 512];
    put32(&mut fat, 0, 0xfffffff8);
    put32(&mut fat, 4, 0xffffffff);
    put32(&mut fat, 2 * 4, 0xffffffff); // root
    put32(&mut fat, 3 * 4, 0xffffffff); // allocation bitmap
    sectors.insert(fat_offset as u64, fat);

    let root_lba = heap_offset as u64;
    let bitmap_lba = root_lba + 1;
    let file_lba = root_lba + 2;
    let dir_lba = root_lba + 3;
    let nested_file_lba = root_lba + 4;
    let mut root = vec![0u8; 512];
    root[0] = 0x81;
    put32(&mut root, 20, 3);
    put64(&mut root, 24, 8);
    let root_file = exfat_file_set("foo.txt", 4, 5, false);
    root[32..32 + root_file.len()].copy_from_slice(&root_file);
    let dir = exfat_file_set("dir", 5, 512, true);
    let dir_offset = 32 + root_file.len();
    root[dir_offset..dir_offset + dir.len()].copy_from_slice(&dir);
    sectors.insert(root_lba, root);

    let mut bitmap = vec![0u8; 512];
    bitmap[0] = 0x1f; // clusters 2..6 are allocated
    sectors.insert(bitmap_lba, bitmap);

    let mut subdir = vec![0u8; 512];
    let bar = exfat_file_set("bar.bin", 6, 7, false);
    subdir[..bar.len()].copy_from_slice(&bar);
    sectors.insert(dir_lba, subdir);
    // file_lba and nested_file_lba are deliberately not captured.

    (
        p,
        SparseReader {
            sectors,
            reads: vec![],
        },
        dir_lba,
        [file_lba, nested_file_lba],
    )
}

#[test]
fn fat16_and_fat32_inventory_reads_no_file_contents() {
    for fat32 in [false, true] {
        let (p, mut r, data) = fat_fixture(fat32);
        let report = analyze_partition(&p, &mut r);
        assert_eq!(report.status, AnalysisStatus::Parsed, "{}", report.reason);
        assert_eq!(
            report.filesystem.as_deref(),
            Some(if fat32 { "fat32" } else { "fat16" })
        );
        assert_eq!(report.total_bytes, Some(p.partition_size));
        assert_eq!(
            report.used_bytes.unwrap() + report.free_bytes.unwrap(),
            p.partition_size
        );
        assert_eq!(
            report.free_bytes,
            Some((if fat32 { 65525 - 4 } else { 4085 - 4 }) * 512)
        );
        assert_eq!(report.file_count, Some(2));
        assert_eq!(report.directory_count, Some(1));
        let entries = report.entries.unwrap();
        assert_eq!(
            entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
            ["/", "/dir/", "/dir/bar.bin", "/foo.txt"]
        );
        assert_eq!(entries[3].logical_size, 5);
        assert_eq!(entries[3].allocated_size, Some(512));
        assert!(!r.reads.contains(&(data + 1)));
        assert!(!r.reads.contains(&(data + 3)));
    }
}

#[test]
fn exfat_inventory_reads_metadata_but_never_ordinary_file_payloads() {
    let (p, mut reader, dir_lba, forbidden) = exfat_fixture();
    let report = analyze_partition(&p, &mut reader);
    assert_eq!(report.status, AnalysisStatus::Parsed, "{}", report.reason);
    assert_eq!(report.filesystem.as_deref(), Some("exfat"));
    assert_eq!(report.total_bytes, Some(p.partition_size));
    assert_eq!(report.file_count, Some(2));
    assert_eq!(report.directory_count, Some(1));
    assert_eq!(report.free_bytes, Some((64 - 5) * 512));
    assert_eq!(
        report
            .entries
            .as_ref()
            .unwrap()
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        ["/", "/dir/", "/dir/bar.bin", "/foo.txt"]
    );
    assert_eq!(
        report
            .entries
            .as_ref()
            .unwrap()
            .last()
            .unwrap()
            .allocated_size,
        Some(512)
    );
    assert!(reader.reads.contains(&dir_lba));
    for lba in forbidden {
        assert!(
            !reader.reads.contains(&lba),
            "ordinary file payload LBA {lba} was read"
        );
    }
}

#[test]
fn corrupt_exfat_metadata_fails_closed() {
    for mutation in 0..4 {
        let (p, mut reader, _, _) = exfat_fixture();
        match mutation {
            0 => reader.sectors.get_mut(&0).unwrap()[64] ^= 1, // boot checksum
            1 => reader.sectors.get_mut(&33).unwrap()[0] &= !1, // root marked free
            2 => reader.sectors.get_mut(&32).unwrap()[34] ^= 1, // entry-set checksum
            _ => put32(reader.sectors.get_mut(&24).unwrap(), 2 * 4, 2), // root FAT loop
        }
        let report = analyze_partition(&p, &mut reader);
        assert_eq!(
            report.status,
            AnalysisStatus::ParseFailed,
            "mutation {mutation}: {}",
            report.reason
        );
        assert_eq!(report.filesystem.as_deref(), Some("exfat"));
        assert!(report.entries.is_none());
        assert!(report.used_bytes.is_none());
    }
}

#[test]
fn corrupt_fat_metadata_fails_closed() {
    for mutation in 0..6 {
        let (p, mut r, data) = fat_fixture(false);
        match mutation {
            0 => r.sectors.get_mut(&0).unwrap()[13] = 255,
            1 => put32(r.sectors.get_mut(&0).unwrap(), 32, u32::MAX),
            2 => put16(r.sectors.get_mut(&1).unwrap(), 8, 4), // circular directory chain
            3 => r.sectors.get_mut(&(data - 1)).unwrap()[0] = b'/',
            4 => {
                r.sectors.get_mut(&1).unwrap().truncate(511);
            }
            _ => put16(r.sectors.get_mut(&(data - 1)).unwrap(), 26, 60000),
        }
        let report = analyze_partition(&p, &mut r);
        assert_eq!(
            report.status,
            AnalysisStatus::ParseFailed,
            "mutation {mutation}: {}",
            report.reason
        );
        assert!(report.entries.is_none());
        assert!(report.used_bytes.is_none());
    }
}

#[test]
fn unknown_and_locked_readers_have_explicit_states() {
    let mut p = partition();
    let mut r = SparseReader {
        sectors: BTreeMap::new(),
        reads: vec![],
    };
    assert_eq!(analyze_partition(&p, &mut r).status, AnalysisStatus::Locked);
    assert!(r.reads.is_empty());
    p.need_encrypt = 0;
    p.need_disturb = 0;
    r.sectors.insert(0, vec![0; 512]);
    assert_eq!(
        analyze_partition(&p, &mut r).status,
        AnalysisStatus::Unsupported
    );
    r.sectors.get_mut(&0).unwrap()[3..11].copy_from_slice(b"NTFS    ");
    let report = analyze_partition(&p, &mut r);
    assert_eq!(report.status, AnalysisStatus::Unsupported);
    assert!(report.file_count.is_none());
    r.sectors.get_mut(&0).unwrap()[3..11].copy_from_slice(b"EXFAT   ");
    let report = analyze_partition(&p, &mut r);
    assert_eq!(report.status, AnalysisStatus::ParseFailed);
    assert!(report.file_count.is_none());
}

#[test]
fn raw_partition_reader_rejects_out_of_range_without_device_io() {
    struct Dev(usize);
    impl edpcli::diskio::SectorDev for Dev {
        fn read_sector(&mut self, _: u32) -> io::Result<Vec<u8>> {
            self.0 += 1;
            Ok(vec![0; 512])
        }
        fn write_sector(&mut self, _: u32, _: &[u8]) -> io::Result<()> {
            panic!("unexpected write")
        }
        fn reopen_rdwr(&mut self, _: std::time::Duration) -> io::Result<()> {
            panic!("unexpected reopen")
        }
    }
    let p = partition();
    let mut d = Dev(0);
    let mut r = edpcli::backup_deep::RawPartitionReader::new(&mut d, &p);
    assert!(r.read_sector(p.sector_count).is_err());
    assert!(r.read_sector(u64::MAX).is_err());
    assert_eq!(d.0, 0);
}

#[test]
fn fat_long_names_and_local_timestamps_are_preserved() {
    let (p, mut r, data) = fat_fixture(false);
    let mut short = entry(b"LONGNA~1TXT", 3, 5, false);
    put16(&mut short, 24, ((2026 - 1980) << 9) | (9 << 5) | 22);
    put16(&mut short, 22, (17 << 11) | (30 << 5) | 15);
    let check = short[..11]
        .iter()
        .fold(0u8, |sum, &c| sum.rotate_right(1).wrapping_add(c));
    let mut long = [0xff; 32];
    long[0] = 0x41;
    long[11] = 15;
    long[12] = 0;
    long[13] = check;
    put16(&mut long, 26, 0);
    let units: Vec<u16> = "长文件.txt".encode_utf16().chain([0]).collect();
    for (&offset, value) in [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30]
        .iter()
        .zip(units.iter().copied().chain(std::iter::repeat(0xffff)))
    {
        put16(&mut long, offset, value);
    }
    let root = r.sectors.get_mut(&(data - 1)).unwrap();
    root.fill(0);
    root[..32].copy_from_slice(&long);
    root[32..64].copy_from_slice(&short);
    let report = analyze_partition(&p, &mut r);
    assert_eq!(report.status, AnalysisStatus::Parsed, "{}", report.reason);
    let e = &report.entries.unwrap()[1];
    assert_eq!(e.path, "/长文件.txt");
    assert_eq!(e.mtime.as_deref(), Some("2026-09-22T17:30:30.00"));
    r.sectors.get_mut(&(data - 1)).unwrap()[13] ^= 1;
    assert_eq!(
        analyze_partition(&p, &mut r).status,
        AnalysisStatus::ParseFailed
    );
}

#[test]
fn deep_container_retains_metadata_and_never_makes_analysis_restorable() {
    use edpcli::{backup_deep::acquire_deep, backup_metadata::acquire_metadata, edpb};
    struct Dev;
    impl edpcli::diskio::SectorDev for Dev {
        fn read_sector(&mut self, _: u32) -> io::Result<Vec<u8>> {
            Ok(vec![0; 512])
        }
        fn write_sector(&mut self, _: u32, _: &[u8]) -> io::Result<()> {
            panic!("unexpected write")
        }
        fn reopen_rdwr(&mut self, _: std::time::Duration) -> io::Result<()> {
            panic!("unexpected reopen")
        }
    }
    let image=include_bytes!("fixtures/protocol/disk4_243625984_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid3164177653_20260827_221910.bin");
    let did = "disk&ven_lexar&prod_usb_flash_drive";
    let metadata = acquire_metadata(&mut Dev, image, did, 243625984).unwrap();
    let deep = acquire_deep(&mut Dev, image, did, 243625984).unwrap();
    for original in &metadata.artifacts {
        let retained = deep.artifacts.iter().find(|a| a.id == original.id).unwrap();
        assert_eq!(retained.data, original.data);
        assert_eq!(retained.restore_policy, original.restore_policy);
    }
    assert!(deep
        .artifacts
        .iter()
        .any(|a| a.id == "raw.lba7_compatibility"));
    let decoded = deep
        .artifacts
        .iter()
        .find(|a| a.id == "decoded.partition.1.sector.0")
        .unwrap();
    assert_eq!(decoded.restore_policy, RestorePolicy::DerivedOnly);
    assert_eq!(decoded.data.len(), 512);
    assert!(decoded
        .derivation
        .as_ref()
        .unwrap()
        .source_artifact_ids
        .iter()
        .any(|id| id.starts_with("raw.partition.1.deep.")));

    for index in [1, 2] {
        for kind in ["filesystem_summary", "file_list"] {
            let a = deep
                .artifacts
                .iter()
                .find(|a| a.id == format!("derived.partition.{index}.{kind}"))
                .unwrap();
            assert_eq!(a.restore_policy, RestorePolicy::DerivedOnly);
            let v: serde_json::Value = serde_json::from_slice(&a.data).unwrap();
            assert_eq!(v["status"], "unsupported");
        }
    }
    let path = std::env::temp_dir().join(format!(
        "deep-{}-{}.edpb",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let capture = edpb::MetadataCapture {
        core: edpb::CoreCapture {
            snapshot_id: "deep-test".into(),
            created_epoch: 1_790_000_000,
            disk_number: Some(5),
            vid: "21c4".into(),
            pid: "0cd1".into(),
            device_id: did.into(),
            onlyid: Some("3164177653".into()),
            total_sectors: Some(243625984),
            logical_sector_size: 512,
            edpcli_version: "test".into(),
            device_state: "encrypted".into(),
            lba0_12: image,
        },
        regions: deep.regions,
        extents: deep.extents,
        artifacts: deep.artifacts,
        notes: deep.notes,
    };
    edpb::write_deep_backup(&path, &capture).unwrap();
    let verified = edpb::verify_file(&path).unwrap();
    assert_eq!(
        verified.manifest.snapshot.capture_level,
        edpb::CaptureLevel::Deep
    );
    assert_eq!(
        verified
            .manifest
            .artifacts
            .iter()
            .filter(|a| a.restore_policy == RestorePolicy::Restorable)
            .count(),
        1
    );
    assert_eq!(edpb::read_raw_protocol(&path).unwrap(), image);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn deep_source_has_no_mutation_or_mount_operations() {
    for source in [
        include_str!("../src/backup_deep.rs"),
        include_str!("../src/backup_deep/fat.rs"),
        include_str!("../src/backup_deep/exfat.rs"),
    ] {
        for forbidden in [
            "write_sector(",
            "prepare_write(",
            "reopen_rdwr(",
            "diskutil",
            "Command::new",
        ] {
            assert!(!source.contains(forbidden), "{forbidden}");
        }
    }
}

#[test]
fn parsed_deep_inventory_cites_captured_raw_metadata() {
    use edpcli::crypto::{a6b0_full, a7f0_full, crc32_bare};
    let (p, fs, data) = fat_fixture(true);
    let did = "disk&ven_lexar&prod_usb_flash_drive";
    let mut image=include_bytes!("fixtures/protocol/disk4_243625984_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid3164177653_20260827_221910.bin").to_vec();
    let key = crc32_bare(did.as_bytes()).to_le_bytes();
    let mut table = a6b0_full(&image[12 * 512..13 * 512], &key, 0);
    put32(&mut table, 96 + 0x14, 0);
    table[96 + 0x28..96 + 0x30].copy_from_slice(&p.partition_size.to_le_bytes());
    image[12 * 512..13 * 512].copy_from_slice(&a7f0_full(&table, &key, 0));
    struct Dev {
        start: u64,
        fs: SparseReader,
        forbidden: [u64; 2],
    }
    impl edpcli::diskio::SectorDev for Dev {
        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            assert!(
                !self.forbidden.contains(&(lba as u64)),
                "read ordinary file data"
            );
            Ok((lba as u64)
                .checked_sub(self.start)
                .and_then(|rel| self.fs.sectors.get(&rel))
                .cloned()
                .unwrap_or(vec![0; 512]))
        }
        fn write_sector(&mut self, _: u32, _: &[u8]) -> io::Result<()> {
            panic!("unexpected write")
        }
    }
    let mut dev = Dev {
        start: p.start_sector,
        fs,
        forbidden: [p.start_sector + data + 1, p.start_sector + data + 3],
    };
    let deep = edpcli::backup_deep::acquire_deep(&mut dev, &image, did, 243625984).unwrap();
    let summary = deep
        .artifacts
        .iter()
        .find(|a| a.id == "derived.partition.1.filesystem_summary")
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&summary.data).unwrap();
    assert_eq!(v["status"], "parsed");
    assert_eq!(v["file_count"], 2);
    for id in &summary.derivation.as_ref().unwrap().source_artifact_ids {
        if id == "raw.protocol.lba0_12" {
            continue;
        }
        let evidence = deep
            .artifacts
            .iter()
            .find(|a| &a.id == id)
            .expect("raw source exists");
        assert_eq!(evidence.restore_policy, RestorePolicy::EvidenceOnly);
    }
    let list = deep
        .artifacts
        .iter()
        .find(|a| a.id == "derived.partition.1.file_list")
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&list.data).unwrap();
    assert_eq!(v["entries"].as_array().unwrap().len(), 4);
}

#[test]
fn default_mode2_key_unwrap_requires_valid_crc() {
    use edpcli::backup_deep::keys::{
        default_file_key, default_file_key_checked, DefaultFileKeyError,
    };
    use edpcli::crypto::{a6b0_full, a7f0_full, crc32_bare};
    let mut image=include_bytes!("fixtures/protocol/disk4_243625984_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid3164177653_20260827_221910.bin").to_vec();
    let did = "disk&ven_lexar&prod_usb_flash_drive";
    let key = default_file_key(&image, did, 1).unwrap();
    assert_eq!(key, default_file_key(&image, did, 2).unwrap());
    let crc = crc32_bare(did.as_bytes()).to_le_bytes();
    let mut plain = a6b0_full(&image[6144..], &crc, 0);
    plain[96 + 0x38] ^= 1;
    image[6144..].copy_from_slice(&a7f0_full(&plain, &crc, 0));
    assert!(default_file_key(&image, did, 1)
        .unwrap_err()
        .contains("FileKeyCRC"));
    assert!(matches!(
        default_file_key_checked(&image, did, 1),
        Err(DefaultFileKeyError::FileKeyCrcMismatch)
    ));
}

#[test]
fn default_mode2_key_unwrap_replays_all_committed_default_password_fixtures() {
    use edpcli::backup_deep::keys::default_file_key;
    use edpcli::crypto::{a6b0_full, crc32_bare};
    use std::fs;

    const DEFAULT_USER_KEY_CRC: u32 = 0x0429_735d;
    let mut expected_entries = 0usize;
    let mut verified_entries = 0usize;

    for entry in fs::read_dir(common::FIXTURE_DIR).expect("protocol fixtures") {
        let path = entry.expect("fixture entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        let Some(meta) = gold_name::parse_gold_name(name) else {
            continue;
        };
        let image = fs::read(&path).expect("fixture bytes");
        if image.len() != 13 * 512 {
            continue;
        }
        let device_crc = crc32_bare(meta.device_id.as_bytes()).to_le_bytes();
        let plain = a6b0_full(&image[12 * 512..13 * 512], &device_crc, 0);

        for index in 0..3 {
            let base = index * 0x60;
            if &plain[base..base + 4] != b"EDPF"
                || u32::from_le_bytes(plain[base + 0x30..base + 0x34].try_into().unwrap())
                    != DEFAULT_USER_KEY_CRC
                || plain[base + 0x58] != 2
            {
                continue;
            }
            expected_entries += 1;
            default_file_key(&image, &meta.device_id, index).unwrap_or_else(|error| {
                panic!("default mode2 key unwrap failed: {name} entry {index}: {error}")
            });
            verified_entries += 1;
        }
    }

    assert_eq!(verified_entries, expected_entries);
    assert!(
        verified_entries >= 12,
        "protocol fixture set lost default-password mode2 coverage"
    );
}

#[test]
fn default_mode2_key_rejects_unverified_profiles() {
    use edpcli::backup_deep::keys::default_file_key;
    use edpcli::crypto::{a6b0_full, a7f0_full, crc32_bare};

    let original = include_bytes!("fixtures/protocol/disk4_243625984_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid3164177653_20260827_221910.bin");
    let did = "disk&ven_lexar&prod_usb_flash_drive";
    let device_crc = crc32_bare(did.as_bytes()).to_le_bytes();

    for (offset, value, expected) in [
        (0x120, 0, "PassInfo"),
        (96 + 0x30, 0, "default password"),
        (96 + 0x58, 3, "mode2"),
    ] {
        let mut image = original.to_vec();
        let mut plain = a6b0_full(&image[6144..], &device_crc, 0);
        plain[offset] = value;
        image[6144..].copy_from_slice(&a7f0_full(&plain, &device_crc, 0));
        assert!(
            default_file_key(&image, did, 1)
                .unwrap_err()
                .contains(expected),
            "mutation at {offset:#x} must be rejected"
        );
    }
}

#[test]
fn mode2_sm4_matches_published_block_vector() {
    use edpcli::backup_deep::keys::sm4_decrypt_block;

    let key = [
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32,
        0x10,
    ];
    let cipher = [
        0x68, 0x1e, 0xdf, 0x34, 0xd2, 0x06, 0x96, 0x5e, 0x86, 0xb3, 0xe9, 0x4f, 0x53, 0x6e, 0x42,
        0x46,
    ];
    assert_eq!(sm4_decrypt_block(&cipher, &key), key);
}
