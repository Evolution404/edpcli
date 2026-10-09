//! P6: disposable *ordinary-file* native-block virtual disk only.
//! No physical disk access, no mount, no transaction writer activation.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};

use edpcli::application::filesystem::{
    FilesystemGeometry, FilesystemKind, FormatRequest, NativeFormatPlan, NativeVirtualDiskPlan,
    EXFAT_DRIVER, FAT12_DRIVER, FAT16_DRIVER, FAT32_DRIVER,
};
use sha2::{Digest, Sha256};

const BPS: u64 = 4096;
const TOTAL: u64 = 130_000;
const PARTITIONS: [(u64, u64, FilesystemKind); 4] = [
    (63, 2497, FilesystemKind::Fat12),
    (2560, 16_384, FilesystemKind::Fat16),
    (20_000, 70_000, FilesystemKind::Fat32),
    (92_000, 32_768, FilesystemKind::ExFat),
];

fn plans_with_sector_bytes(sector_bytes: u32) -> Vec<NativeFormatPlan> {
    PARTITIONS
        .iter()
        .enumerate()
        .map(|(i, (start, count, kind))| {
            let geometry = FilesystemGeometry::new(*start, *count, sector_bytes);
            let request = FormatRequest {
                filesystem: *kind,
                volume_label: Some(format!("DISK{}", i + 1)),
                volume_serial: Some(0x1234_5600 + i as u32),
            };
            match kind {
                FilesystemKind::Fat12 => FAT12_DRIVER.build_native_format_plan(geometry, &request),
                FilesystemKind::Fat16 => FAT16_DRIVER.build_native_format_plan(geometry, &request),
                FilesystemKind::Fat32 => FAT32_DRIVER.build_native_format_plan(geometry, &request),
                FilesystemKind::ExFat => EXFAT_DRIVER.build_native_format_plan(geometry, &request),
                _ => unreachable!(),
            }
            .unwrap_or_else(|error| panic!("{kind:?}: {error}"))
        })
        .collect()
}

fn plans() -> Vec<NativeFormatPlan> {
    plans_with_sector_bytes(4096)
}

fn seek(file: &mut File, lba: u64) -> std::io::Result<()> {
    file.seek(SeekFrom::Start(lba.checked_mul(BPS).unwrap()))?;
    Ok(())
}

fn read_block(file: &mut File, lba: u64) -> Vec<u8> {
    let mut block = vec![0u8; BPS as usize];
    seek(file, lba).unwrap();
    file.read_exact(&mut block).unwrap();
    block
}

fn write_block(file: &mut File, lba: u64, block: &[u8]) {
    assert_eq!(block.len(), BPS as usize);
    seek(file, lba).unwrap();
    file.write_all(block).unwrap();
}

fn u16le(data: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes(data[pos..pos + 2].try_into().unwrap())
}

fn u32le(data: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap())
}

/// Place a small 8.3 file into an already-formatted FAT16/FAT32 virtual
/// partition, using only independent on-disk byte offsets.
fn write_fat_file(file: &mut File, index: usize) -> (u64, u64, Vec<u8>, u32) {
    let (start, _, kind) = PARTITIONS[index];
    assert!(matches!(
        kind,
        FilesystemKind::Fat16 | FilesystemKind::Fat32
    ));
    let fat32 = kind == FilesystemKind::Fat32;
    let boot = read_block(file, start);
    let reserved = u64::from(u16le(&boot, 14));
    let fat_count = u64::from(boot[16]);
    let fat_len = if fat32 {
        u64::from(u32le(&boot, 36))
    } else {
        u64::from(u16le(&boot, 22))
    };
    let spc = u64::from(boot[13]);
    let root_start = start + reserved + fat_count * fat_len;
    let root_sectors = (u64::from(u16le(&boot, 17)) * 32).div_ceil(BPS);
    let data_start = root_start + root_sectors;
    let cluster = if fat32 { 3u32 } else { 2u32 };
    let root_lba = if fat32 { data_start } else { root_start };
    let payload_lba = if fat32 { data_start + spc } else { data_start };
    let payload = format!(
        "4Kn FULL DISK FAT{} FILE ROUNDTRIP\n",
        if fat32 { 32 } else { 16 }
    )
    .repeat(24)
    .into_bytes();
    assert!(payload.len() <= BPS as usize);
    for copy in 0..fat_count {
        let lba = start + reserved + copy * fat_len;
        let mut fat = read_block(file, lba);
        if fat32 {
            let at = cluster as usize * 4;
            fat[at..at + 4].copy_from_slice(&0x0fff_ffffu32.to_le_bytes());
        } else {
            let at = cluster as usize * 2;
            fat[at..at + 2].copy_from_slice(&0xffffu16.to_le_bytes());
        }
        write_block(file, lba, &fat);
    }
    if fat32 {
        // Both primary and backup FSInfo must reflect allocation of cluster 3.
        for offset in [1, 7] {
            let mut info = read_block(file, start + offset);
            let free = u32le(&info, 488);
            info[488..492].copy_from_slice(&(free - 1).to_le_bytes());
            info[492..496].copy_from_slice(&4u32.to_le_bytes());
            write_block(file, start + offset, &info);
        }
    }
    let mut root = read_block(file, root_lba);
    root[32..43].copy_from_slice(b"HELLO   TXT");
    root[32 + 11] = 0x20;
    root[32 + 26..32 + 28].copy_from_slice(&(cluster as u16).to_le_bytes());
    root[32 + 28..32 + 32].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    write_block(file, root_lba, &root);
    let mut data = vec![0u8; BPS as usize];
    data[..payload.len()].copy_from_slice(&payload);
    write_block(file, payload_lba, &data);
    (root_lba, payload_lba, payload, cluster)
}

/// exFAT independent virtual-only file authoring. Exercises the primary
/// directory entry, stream extension, UTF16 filename, FAT chain and bitmap.
/// This is test fixture I/O to a *newly created regular file*, not a
/// filesystem writer, host mount, or production disk transaction.
struct ExfatSample {
    root_lba: u64,
    bitmap_lba: u64,
    fat_lba: u64,
    first_cluster: u32,
    sectors_per_cluster: u64,
    heap_lba: u64,
    payload: Vec<u8>,
}

fn exfat_set_checksum(entries: &[u8]) -> u16 {
    entries
        .iter()
        .enumerate()
        .filter(|(index, _)| !matches!(index, 2 | 3))
        .fold(0u16, |sum, (_, byte)| {
            sum.rotate_right(1).wrapping_add(u16::from(*byte))
        })
}

fn exfat_filename_hash(name: &[u16]) -> u16 {
    // The fixture uses already-uppercase ASCII, which maps to itself in
    // the on-disk exFAT Upcase table.
    name.iter()
        .flat_map(|c| c.to_le_bytes())
        .fold(0u16, |sum, byte| {
            sum.rotate_right(1).wrapping_add(u16::from(byte))
        })
}

fn write_exfat_file(file: &mut File) -> ExfatSample {
    let (start, count, kind) = PARTITIONS[3];
    assert_eq!(kind, FilesystemKind::ExFat);
    let boot = read_block(file, start);
    assert_eq!(&boot[3..11], b"EXFAT   ");
    assert_eq!(boot[108], 12);
    let spc = 1u64 << boot[109];
    let cluster_bytes = spc * BPS;
    let fat_lba = start + u64::from(u32le(&boot, 80));
    let heap_lba = start + u64::from(u32le(&boot, 88));
    let cluster_count = u32le(&boot, 92);
    assert_eq!(u64::from(u32le(&boot, 96)), 2);
    let root_lba = heap_lba;
    let root = read_block(file, root_lba);
    assert_eq!(root[0], 0x81);
    assert_eq!(root[32], 0x82);
    assert_eq!(root[64], 0x83);
    let bitmap_cluster = u32le(&root, 20);
    let upcase_cluster = u32le(&root, 52);
    let upcase_length = u64::from_le_bytes(root[56..64].try_into().unwrap());
    let upcase_clusters = upcase_length.div_ceil(cluster_bytes);
    let first_cluster = upcase_cluster + upcase_clusters as u32;
    assert_eq!(bitmap_cluster, 3);
    assert!(u64::from(first_cluster) + 1 < u64::from(cluster_count) + 2);
    let bitmap_lba = heap_lba + u64::from(bitmap_cluster - 2) * spc;
    let payload = b"EDPCLI: EXFAT NATIVE 4096B FILE AND FAT CHAIN HASH CHECK\n"
        .repeat((2 * cluster_bytes as usize) / 50)
        .into_iter()
        .take(2 * cluster_bytes as usize - 71)
        .collect::<Vec<u8>>();
    assert!(payload.len() as u64 > cluster_bytes);
    assert!((payload.len() as u64) < 2 * cluster_bytes);

    // Allocate two clusters and mark their chained values in the single FAT.
    let fat_entries_per_block = BPS / 4;
    for (index, next) in [(0u32, first_cluster + 1), (1, 0xffff_ffff)] {
        let cluster = first_cluster + index;
        let fat_sector = fat_lba + u64::from(cluster) / fat_entries_per_block;
        let mut fat = read_block(file, fat_sector);
        let offset = (u64::from(cluster) % fat_entries_per_block) as usize * 4;
        assert_eq!(u32le(&fat, offset), 0);
        fat[offset..offset + 4].copy_from_slice(&next.to_le_bytes());
        write_block(file, fat_sector, &fat);
    }
    let mut bitmap = read_block(file, bitmap_lba);
    for cluster in [first_cluster, first_cluster + 1] {
        let bit = cluster - 2;
        let index = (bit / 8) as usize;
        let mask = 1u8 << (bit % 8);
        assert_eq!(bitmap[index] & mask, 0);
        bitmap[index] |= mask;
    }
    write_block(file, bitmap_lba, &bitmap);

    // Add exactly three directory entries following the volume metadata.
    let mut directory = read_block(file, root_lba);
    let start_entry = 96usize;
    assert_eq!(directory[start_entry], 0);
    let records = &mut directory[start_entry..start_entry + 96];
    records[0] = 0x85;
    records[1] = 2;
    records[4..6].copy_from_slice(&0x20u16.to_le_bytes()); // archive
    records[32] = 0xC0;
    records[33] = 0; // FAT chain in use (no NoFatChain flag)
    let name: Vec<u16> = "HELLO.TXT".encode_utf16().collect();
    records[35] = name.len() as u8;
    records[36..38].copy_from_slice(&exfat_filename_hash(&name).to_le_bytes());
    records[40..48].copy_from_slice(&(payload.len() as u64).to_le_bytes()); // valid length
    records[52..56].copy_from_slice(&first_cluster.to_le_bytes());
    records[56..64].copy_from_slice(&(payload.len() as u64).to_le_bytes());
    records[64] = 0xC1;
    for (i, ch) in name.iter().enumerate() {
        let index = 66 + 2 * i;
        records[index..index + 2].copy_from_slice(&ch.to_le_bytes());
    }
    let checksum = exfat_set_checksum(records);
    records[2..4].copy_from_slice(&checksum.to_le_bytes());
    write_block(file, root_lba, &directory);

    for (index, chunk) in payload.chunks(BPS as usize).enumerate() {
        let cluster_index = index as u64 / spc;
        assert!(cluster_index < 2);
        let sector_offset = index as u64 % spc;
        let lba = heap_lba + (u64::from(first_cluster - 2) + cluster_index) * spc + sector_offset;
        assert!(lba < start + count);
        let mut sector = vec![0u8; BPS as usize];
        sector[..chunk.len()].copy_from_slice(chunk);
        write_block(file, lba, &sector);
    }
    ExfatSample {
        root_lba,
        bitmap_lba,
        fat_lba,
        first_cluster,
        sectors_per_cluster: spc,
        heap_lba,
        payload,
    }
}

fn verify_exfat_file(file: &mut File, sample: &ExfatSample) {
    let directory = read_block(file, sample.root_lba);
    let records = &directory[96..192];
    assert_eq!(&[records[0], records[32], records[64]], &[0x85, 0xC0, 0xC1]);
    assert_eq!(records[1], 2);
    assert_eq!(u16le(records, 2), exfat_set_checksum(records));
    assert_eq!(records[33] & 0x02, 0); // data stream is FAT chained
    let name_len = records[35] as usize;
    let name = (0..name_len)
        .map(|i| u16le(records, 66 + 2 * i))
        .collect::<Vec<_>>();
    assert_eq!(String::from_utf16(&name).unwrap(), "HELLO.TXT");
    assert_eq!(u16le(records, 36), exfat_filename_hash(&name));
    let length = u64::from_le_bytes(records[56..64].try_into().unwrap());
    let valid_length = u64::from_le_bytes(records[40..48].try_into().unwrap());
    assert_eq!(length, sample.payload.len() as u64);
    assert_eq!(valid_length, length);
    let first_cluster = u32le(records, 52);
    assert_eq!(first_cluster, sample.first_cluster);

    let bitmap = read_block(file, sample.bitmap_lba);
    // Upcase metadata must survive file creation unchanged, including the
    // separate root metadata checksum (not the file entry-set checksum).
    let root_meta = read_block(file, sample.root_lba);
    let upcase_first = u32le(&root_meta, 52);
    let upcase_size = u64::from_le_bytes(root_meta[56..64].try_into().unwrap());
    assert!(upcase_size > 0);
    let mut upcase = Vec::new();
    for lba_offset in 0..upcase_size.div_ceil(BPS) {
        upcase.extend_from_slice(&read_block(
            file,
            sample.heap_lba + u64::from(upcase_first - 2) * sample.sectors_per_cluster + lba_offset,
        ));
    }
    upcase.truncate(upcase_size as usize);
    let checksum = upcase.iter().fold(0u32, |sum, byte| {
        sum.rotate_right(1).wrapping_add(u32::from(*byte))
    });
    assert_eq!(u32le(&root_meta, 36), checksum);
    let mut data = Vec::new();
    let mut cluster = first_cluster;
    for hop in 0..2u32 {
        let bit = cluster - 2;
        assert_ne!(bitmap[(bit / 8) as usize] & (1 << (bit % 8)), 0);
        for sector in 0..sample.sectors_per_cluster {
            let lba =
                sample.heap_lba + u64::from(cluster - 2) * sample.sectors_per_cluster + sector;
            data.extend_from_slice(&read_block(file, lba));
        }
        let fat_sector = sample.fat_lba + u64::from(cluster) / (BPS / 4);
        let fat = read_block(file, fat_sector);
        let next = u32le(&fat, (u64::from(cluster) % (BPS / 4)) as usize * 4);
        if hop == 0 {
            assert_eq!(next, cluster + 1);
            cluster = next;
        } else {
            assert!(next >= 0xffff_fff8);
        }
    }
    data.truncate(length as usize);
    assert_eq!(
        Sha256::digest(&data)[..],
        Sha256::digest(&sample.payload)[..]
    );
    // Deliberate corruption probes: a validator must reject damaged entry
    // metadata and user data, without modifying the saved source file.
    let mut tampered_records = records.to_vec();
    tampered_records[66] ^= 1;
    assert_ne!(
        u16le(&tampered_records, 2),
        exfat_set_checksum(&tampered_records)
    );
    let mut tampered_payload = data;
    tampered_payload[0] ^= 1;
    assert_ne!(
        Sha256::digest(&tampered_payload)[..],
        Sha256::digest(&sample.payload)[..]
    );
}

#[test]
fn four_kn_full_disk_four_filesystem_file_roundtrips() {
    let plans = plans();
    let composite = NativeVirtualDiskPlan::assemble(TOTAL, 4096, &plans).unwrap();
    assert_eq!(composite.total_sectors, TOTAL);
    assert_eq!(composite.sector_bytes, 4096);
    assert_eq!(composite.writes.last().unwrap().relative_lba, 0);
    assert_eq!(
        &composite.writes.last().unwrap().data[510..512],
        &[0x55, 0xaa]
    );
    let external = std::env::var_os("EDPCLI_NATIVE_DISK_IMAGE_PATH");
    let path = external
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("edpcli-p6-full-4kn-{}.img", std::process::id()))
        });
    // create_new prevents clobbering pre-existing raw targets and symlinks.
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    assert!(file.metadata().unwrap().file_type().is_file());
    file.set_len(TOTAL * BPS).unwrap();

    // Failure injection before final MBR commit. All component writes stay in
    // complete native blocks. A failed build must not show a valid MBR.
    for block in composite.writes.iter().take(17) {
        assert_ne!(block.relative_lba, 0);
        write_block(&mut file, block.relative_lba, &block.data);
    }
    file.sync_all().unwrap();
    assert!(read_block(&mut file, 0).iter().all(|byte| *byte == 0));

    // Commit rest of metadata, and MBR only last.
    for block in composite.writes.iter().skip(17) {
        write_block(&mut file, block.relative_lba, &block.data);
    }
    file.sync_all().unwrap();
    drop(file);

    // Fresh file descriptor and independent byte-level MBR/partition probe.
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let mbr = read_block(&mut file, 0);
    assert_eq!(&mbr[510..512], &[0x55, 0xaa]);
    assert!(mbr[512..].iter().all(|b| *b == 0));
    for (i, ((start, count, kind), plan)) in PARTITIONS.iter().zip(&plans).enumerate() {
        let entry = 446 + 16 * i;
        let type_id = match kind {
            FilesystemKind::Fat12 => 0x01,
            FilesystemKind::Fat16 => 0x06,
            FilesystemKind::Fat32 => 0x0c,
            FilesystemKind::ExFat => 0x07,
            _ => unreachable!(),
        };
        assert_eq!(mbr[entry + 4], type_id);
        assert_eq!(u32le(&mbr, entry + 8) as u64, *start);
        assert_eq!(u32le(&mbr, entry + 12) as u64, *count);
        for metadata in &plan.writes {
            let raw = read_block(&mut file, *start + metadata.relative_lba);
            assert_eq!(Sha256::digest(&raw)[..], Sha256::digest(&metadata.data)[..]);
        }
        let boot = read_block(&mut file, *start);
        match kind {
            FilesystemKind::ExFat => {
                assert_eq!(&boot[3..11], b"EXFAT   ");
                assert_eq!(boot[108], 12);
                assert_eq!(u32le(&boot, 80), 24);
            }
            FilesystemKind::Fat12 | FilesystemKind::Fat16 | FilesystemKind::Fat32 => {
                assert_eq!(u16le(&boot, 11), 4096);
                assert_eq!(&boot[510..512], &[0x55, 0xaa]);
            }
            _ => unreachable!(),
        }
    }

    // Exercise a real FAT12 file entry and payload without calling edpcli's
    // filesystem implementation for verification.
    let (start, count, _) = PARTITIONS[0];
    let boot = read_block(&mut file, start);
    let reserved = u16le(&boot, 14) as u64;
    let fat_sectors = u16le(&boot, 22) as u64;
    let fats = u64::from(boot[16]);
    let root_sectors = (u64::from(u16le(&boot, 17)) * 32).div_ceil(BPS);
    let root_lba = start + reserved + fats * fat_sectors;
    let data_lba = root_lba + root_sectors;
    assert_eq!(data_lba, start + 7);
    assert!(data_lba < start + count);
    let content = b"edpcli independent native 4096B FAT12 file readback regression\n".repeat(16);
    assert!(content.len() < BPS as usize);
    for fat_copy in 0..fats {
        let lba = start + reserved + fat_copy * fat_sectors;
        let mut fat = read_block(&mut file, lba);
        // FAT12 cluster #2 at packed offset3: end-of-chain 0xFFF.
        fat[3] = 0xff;
        fat[4] = (fat[4] & 0xf0) | 0x0f;
        write_block(&mut file, lba, &fat);
    }
    let mut root = read_block(&mut file, root_lba);
    let entry = 32usize; // preceding slot is the existing volume label
    root[entry..entry + 11].copy_from_slice(b"HELLO   TXT");
    root[entry + 11] = 0x20;
    root[entry + 26..entry + 28].copy_from_slice(&2u16.to_le_bytes());
    root[entry + 28..entry + 32].copy_from_slice(&(content.len() as u32).to_le_bytes());
    write_block(&mut file, root_lba, &root);
    let mut data = vec![0u8; BPS as usize];
    data[..content.len()].copy_from_slice(&content);
    write_block(&mut file, data_lba, &data);
    let fat16_sample = write_fat_file(&mut file, 1);
    let fat32_sample = write_fat_file(&mut file, 2);
    let exfat_sample = write_exfat_file(&mut file);
    file.sync_all().unwrap();
    drop(file);

    let mut file = OpenOptions::new().read(true).open(&path).unwrap();
    let root = read_block(&mut file, root_lba);
    let file_entry = &root[32..64];
    assert_eq!(&file_entry[..11], b"HELLO   TXT");
    assert_eq!(u16le(file_entry, 26), 2);
    let read_len = u32le(file_entry, 28) as usize;
    let mut file_bytes = read_block(&mut file, data_lba);
    file_bytes.truncate(read_len);
    assert_eq!(
        Sha256::digest(&file_bytes)[..],
        Sha256::digest(&content)[..]
    );
    let fat_a = read_block(&mut file, start + reserved);
    let fat_b = read_block(&mut file, start + reserved + fat_sectors);
    assert_eq!(fat_a, fat_b);
    assert_eq!(
        u16::from(fat_a[3]) | (u16::from(fat_a[4] & 0x0f) << 8),
        0x0fff
    );

    for (root_lba, payload_lba, expected, cluster) in [&fat16_sample, &fat32_sample] {
        let entry = read_block(&mut file, *root_lba);
        assert_eq!(&entry[32..43], b"HELLO   TXT");
        assert_eq!(u16le(&entry, 32 + 26), *cluster as u16);
        let size = u32le(&entry, 32 + 28) as usize;
        let mut data = read_block(&mut file, *payload_lba);
        data.truncate(size);
        assert_eq!(Sha256::digest(&data)[..], Sha256::digest(expected)[..]);
    }
    verify_exfat_file(&mut file, &exfat_sample);
    if external.is_none() {
        drop(file);
        std::fs::remove_file(&path).unwrap();
    }
}

#[test]
fn native_full_disk_assembler_rejects_mismatches_overlap_and_bad_writes() {
    let original = plans();
    assert!(NativeVirtualDiskPlan::assemble(TOTAL, 512, &original).is_err());
    assert!(NativeVirtualDiskPlan::assemble(TOTAL, 2048, &original).is_err());
    assert!(NativeVirtualDiskPlan::assemble(0, 4096, &original).is_err());
    assert!(NativeVirtualDiskPlan::assemble(TOTAL, 4096, &[]).is_err());
    let mut duplicates = original.clone();
    duplicates[1].geometry.partition_offset = duplicates[0].geometry.partition_offset;
    assert!(NativeVirtualDiskPlan::assemble(TOTAL, 4096, &duplicates).is_err());
    let mut bad = original.clone();
    bad[1].writes[0].data.resize(512, 0);
    assert!(NativeVirtualDiskPlan::assemble(TOTAL, 4096, &bad).is_err());
    let mut no_boot = original.clone();
    no_boot[0].writes.retain(|write| write.relative_lba != 0);
    assert!(NativeVirtualDiskPlan::assemble(TOTAL, 4096, &no_boot).is_err());
    let mut mismatched_kind = original.clone();
    mismatched_kind[0].expected_metadata.kind = FilesystemKind::ExFat;
    assert!(NativeVirtualDiskPlan::assemble(TOTAL, 4096, &mismatched_kind).is_err());
    let mut bad = original.clone();
    bad[2].writes[0].relative_lba = bad[2].geometry.sector_count;
    assert!(NativeVirtualDiskPlan::assemble(TOTAL, 4096, &bad).is_err());
    let mut duplicate = original.clone();
    let repeated_block = duplicate[0].writes[0].clone();
    duplicate[0].writes.push(repeated_block);
    assert!(NativeVirtualDiskPlan::assemble(TOTAL, 4096, &duplicate).is_err());
    let mut beyond = original.clone();
    beyond[3].geometry.partition_offset = TOTAL - 1;
    assert!(NativeVirtualDiskPlan::assemble(TOTAL, 4096, &beyond).is_err());
    let mut zero = original;
    zero[0].geometry.partition_offset = 0;
    assert!(NativeVirtualDiskPlan::assemble(TOTAL, 4096, &zero).is_err());
}

#[test]
fn native_full_disk_512b_compatibility_and_4kn_independent_block_sizes() {
    let plans_512 = plans_with_sector_bytes(512);
    let plans_4096 = plans_with_sector_bytes(4096);
    let legacy = NativeVirtualDiskPlan::assemble(TOTAL, 512, &plans_512).unwrap();
    let native = NativeVirtualDiskPlan::assemble(TOTAL, 4096, &plans_4096).unwrap();
    assert_eq!(legacy.sector_bytes, 512);
    assert_eq!(native.sector_bytes, 4096);
    assert_eq!(legacy.total_sectors, native.total_sectors);
    let legacy_mbr = &legacy.writes.last().unwrap().data;
    let native_mbr = &native.writes.last().unwrap().data;
    assert_eq!(&native_mbr[..512], legacy_mbr);
    assert!(native_mbr[512..].iter().all(|byte| *byte == 0));
    for write in &legacy.writes {
        assert_eq!(write.data.len(), 512);
    }
    for write in &native.writes {
        assert_eq!(write.data.len(), 4096);
    }
    assert!(legacy.writes.len() > 10);
    assert!(native.writes.len() > 10);
}

/// AES_CROSS data crypto in a sparse *ordinary file*: this is deliberately
/// independent of physical USB 4Kn provisioning or FileKey/password records.
/// The AES block algorithm is already pinned to the official Windows driver.
#[test]
fn aes_cross_native_four_kn_virtual_exfat_file_roundtrip() {
    use edpcli::application::filesystem::detect_native_boot_sector;
    use edpcli::application::inspect::decrypt_mode3_native;
    use edpcli::protocol::crypto::aes128_ecb_encrypt_block;

    let key = std::array::from_fn(|index| index as u8);
    let (start, count, fs) = PARTITIONS[3];
    assert_eq!(fs, FilesystemKind::ExFat);
    let composite = NativeVirtualDiskPlan::assemble(TOTAL, 4096, &plans()).unwrap();
    let mut written_encrypted = 0;
    let scratch = std::env::temp_dir().join(format!(
        "edpcli-p6-aes-cross-native-4kn-{}-{}.img",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // create_new rejects existing files/symlinks; no raw device path is ever
    // accepted or opened, and only this private scratch pathname is removed.
    let mut file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&scratch)
        .unwrap();
    assert!(file.metadata().unwrap().is_file());
    file.set_len(TOTAL * BPS).unwrap();
    let encrypt = |plain: &[u8]| {
        assert_eq!(plain.len(), BPS as usize);
        plain
            .as_chunks::<16>()
            .0
            .iter()
            .flat_map(|block| aes128_ecb_encrypt_block(block, &key))
            .collect::<Vec<u8>>()
    };
    // The generator emits the MBR last, after every encrypted partition block.
    assert_eq!(composite.writes.last().unwrap().relative_lba, 0);
    for write in &composite.writes {
        let inside = (start..start + count).contains(&write.relative_lba);
        let payload = if inside {
            written_encrypted += 1;
            encrypt(&write.data)
        } else {
            write.data.clone()
        };
        write_block(&mut file, write.relative_lba, &payload);
    }
    assert!(written_encrypted >= 2);
    file.sync_all().unwrap();
    drop(file);

    let mut verify = OpenOptions::new().read(true).open(&scratch).unwrap();
    assert_eq!(&read_block(&mut verify, 0)[510..512], &[0x55, 0xaa]);
    let boot_raw = read_block(&mut verify, start);
    assert_ne!(&boot_raw[3..11], b"EXFAT   ");
    let boot = decrypt_mode3_native(&boot_raw, &key).unwrap();
    assert_eq!(&boot[3..11], b"EXFAT   ");
    assert_eq!(boot[108], 12);
    assert!(detect_native_boot_sector(&boot_raw, count, 4096)
        .unwrap()
        .is_none());
    let recognized = detect_native_boot_sector(&boot, count, 4096).unwrap();
    assert!(
        recognized.is_some(),
        "decrypted native exFAT must pass strict BPB"
    );
    let mut wrong_key = key;
    wrong_key[0] ^= 1;
    let wrong_boot = decrypt_mode3_native(&boot_raw, &wrong_key).unwrap();
    assert!(detect_native_boot_sector(&wrong_boot, count, 4096)
        .unwrap()
        .is_none());

    let formatted = &plans()[3];
    for write in &formatted.writes {
        let full_lba = start + write.relative_lba;
        let cipher = read_block(&mut verify, full_lba);
        let recovered = decrypt_mode3_native(&cipher, &key).unwrap();
        assert_eq!(recovered, write.data, "AES_CROSS exFAT LBA{full_lba}");
        assert_eq!(cipher.len(), 4096);
        assert_ne!(
            cipher, recovered,
            "no ciphertext block may remain plaintext"
        );
    }
    assert!(decrypt_mode3_native(&boot_raw[..512], &key).is_ok());
    assert!(decrypt_mode3_native(&boot_raw[..4095], &key).is_err());
    drop(verify);
    std::fs::remove_file(&scratch).unwrap();
}
