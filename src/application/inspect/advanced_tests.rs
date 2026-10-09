use super::*;
use std::io;

use crate::common::METADATA_IMAGE_LEN;

struct MemoryReader {
    sectors: Vec<Vec<u8>>,
}

impl SectorReader for MemoryReader {
    fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        self.sectors
            .get(lba as usize)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "missing sector"))
    }
}

#[test]
fn sector_reader_range_is_checked_and_lossless() {
    let mut reader = MemoryReader {
        sectors: vec![vec![0x11; SECTOR], vec![0x22; SECTOR], vec![0x33; SECTOR]],
    };
    let range = reader.read_range(1, 2).unwrap();
    assert_eq!(range.len(), 2 * SECTOR);
    assert_eq!(&range[..SECTOR], vec![0x22; SECTOR].as_slice());
    assert_eq!(&range[SECTOR..], vec![0x33; SECTOR].as_slice());

    reader.sectors[2].truncate(SECTOR - 1);
    let error = reader.read_range(2, 1).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    assert!(error.to_string().contains("511B"));
}

#[test]
fn native_sector_range_preserves_full_blocks_for_parameterized_geometry() {
    struct NativeRangeReader {
        width: u32,
        sectors: Vec<Vec<u8>>,
        reads: Vec<u64>,
    }
    impl SectorReader for NativeRangeReader {
        fn logical_sector_bytes(&self) -> u32 {
            self.width
        }
        fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
            Ok(self
                .sectors
                .get(lba as usize)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no sector"))?[..512]
                .to_vec())
        }
        fn read_native_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
            self.reads.push(lba);
            self.sectors
                .get(lba as usize)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no sector"))
        }
    }

    for width in [512u32, 1024, 2048, 4096, 8192] {
        let mut reader = NativeRangeReader {
            width,
            sectors: (0..3)
                .map(|index| vec![index + 13; width as usize])
                .collect(),
            reads: Vec::new(),
        };
        let native = reader.read_native_range(1, 2).unwrap();
        assert_eq!(native.len(), 2 * width as usize);
        assert_eq!(native[..width as usize], reader.sectors[1]);
        assert_eq!(native[width as usize..], reader.sectors[2]);
        assert_eq!(reader.reads, vec![1, 2]);
        reader.reads.clear();
        let legacy = reader.read_range(1, 2).unwrap();
        assert_eq!(
            legacy.len(),
            1024,
            "512B wire projection must stay independent"
        );
        assert_eq!(reader.reads, Vec::<u64>::new());

        reader.sectors[2].pop();
        let failure = reader.read_native_range(1, 2).unwrap_err();
        assert_eq!(failure.kind(), io::ErrorKind::UnexpectedEof);
        assert!(failure.to_string().contains("完整原生扇区"));
    }

    // The application reads the full 13-block native protocol envelope in
    // one bounded operation, retaining every unknown byte past the 512B
    // protocol projection. It must never accept a truncated final 4Kn block.
    let mut reader = NativeRangeReader {
        width: 4096,
        sectors: (0..13).map(|lba| vec![lba + 1; 4096]).collect(),
        reads: Vec::new(),
    };
    let protocol = reader
        .read_native_range(0, crate::common::METADATA_SECTOR_COUNT)
        .unwrap();
    assert_eq!(protocol.len(), 13 * 4096);
    assert_eq!(reader.reads, (0u64..13).collect::<Vec<_>>());
    assert_eq!(&protocol[12 * 4096..], vec![13; 4096].as_slice());
    reader.reads.clear();
    reader.sectors[12].truncate(4095);
    let error = reader
        .read_native_range(0, crate::common::METADATA_SECTOR_COUNT)
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    assert_eq!(reader.reads, (0u64..13).collect::<Vec<_>>());
    let mut reader = NativeRangeReader {
        width: 4096,
        sectors: vec![vec![0; 4096]],
        reads: Vec::new(),
    };
    for (first, count) in [(u64::MAX, 2), (0, usize::MAX), (0, 2049)] {
        assert_eq!(
            reader.read_native_range(first, count).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(reader.reads.is_empty(), "invalid request must not read");
    }
    for invalid_width in [0u32, 256, 1000, 65_537] {
        reader.width = invalid_width;
        assert_eq!(
            reader.read_native_range(0, 1).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(reader.reads.is_empty());
    }
}

#[test]
fn decoder_registry_fails_closed_outside_registered_regions() {
    let context = crate::inspect_target::InspectDiskContext::new_with_partition_table(
        vec![0; METADATA_IMAGE_LEN],
        Some("disk&ven_test&prod_test".into()),
        4096,
        Some(crate::provision::DiskProvisionKind::Mode0),
        None,
        None,
    );
    let meta = InspectMeta::default();
    assert_eq!(DECODER_REGISTRY[0], InspectDecoderKind::Protocol);
    let mut lba0 = [0u8; SECTOR];
    lba0[510..512].copy_from_slice(&[0x55, 0xaa]);
    let (_, method, ranges) = decode_sector(&context, &meta, 0, &lba0, None).unwrap();
    assert!(!method.is_empty());
    assert!(
        ranges.is_empty(),
        "LBA0 is parsed but does not pass through a sector decoder"
    );

    let error = decode_sector(&context, &meta, 100, &[0; SECTOR], None).unwrap_err();
    assert_eq!(error.kind(), InspectErrorKind::Decode);
    assert!(error.message().contains("不属于已注册 decoder"), "{error}");
}

#[test]
fn field_range_is_absolute_and_cross_sector_capable() {
    let range = AbsoluteByteRange {
        start: 100 * SECTOR as u64 + 0x1f0,
        end_exclusive: 101 * SECTOR as u64 + 0x30,
    };
    assert_eq!(range.start_lba(), 100);
    assert_eq!(range.end_lba(), 101);
    assert!(range.spans_sectors());
    assert_eq!(range.len(), 64);
}

#[test]
fn protocol_fields_preserve_raw_decoded_and_absolute_range() {
    let lba = 4u64;
    let mut raw = vec![0u8; SECTOR];
    raw[8..12].copy_from_slice(&[1, 2, 3, 4]);
    let mut decoded = raw.clone();
    decoded[8..12].copy_from_slice(&[5, 6, 7, 8]);
    let fields = vec![crate::inspect_adapter::SectorField {
        start: 8,
        end: 12,
        label: "test".into(),
        value: "value".into(),
        style: crate::inspect_adapter::FieldStyle::Identity,
        group: Some("group".into()),
        children: Vec::new(),
        status: crate::inspect_adapter::SectorFieldStatus::Known,
        transform: None,
    }];
    let materialized = materialize_protocol_fields(lba, &raw, &decoded, &fields).unwrap();
    assert_eq!(materialized.len(), 1);
    let field = &materialized[0];
    assert_eq!(field.range.start, 4 * SECTOR as u64 + 8);
    assert_eq!(field.range.end_exclusive, 4 * SECTOR as u64 + 12);
    assert_eq!(field.range.len(), 4);
    assert_eq!(field.raw, [1, 2, 3, 4]);
    assert_eq!(field.decoded, [5, 6, 7, 8]);
    assert_eq!(field.field_type, InspectFieldType::Identity);
    assert_eq!(field.status, InspectFieldStatus::Known);
    assert_eq!(field.group.as_deref(), Some("group"));
}

#[test]
fn pass_info_field_keeps_all_four_byte_and_value_layers() {
    let raw = vec![0u8; SECTOR];
    let mut sector_decoded = raw.clone();
    sector_decoded[0x126] = 0x77;
    let fields = vec![crate::inspect_adapter::SectorField {
        start: 0x126,
        end: 0x127,
        label: "保密区最大错误次数".into(),
        value: "255".into(),
        style: crate::inspect_adapter::FieldStyle::Flag,
        group: Some("PassInfo".into()),
        children: Vec::new(),
        status: crate::inspect_adapter::SectorFieldStatus::Known,
        transform: Some(FieldTransform::XorByte {
            offset: 0,
            mask: 0x88,
        }),
    }];
    let materialized = materialize_protocol_fields(12, &raw, &sector_decoded, &fields)
        .expect("valid one-byte field");
    let field = &materialized[0];
    assert_eq!(field.key, InspectFieldKey::Lba12MaxEncryptPasswordErrors);
    assert_eq!(field.raw, [0x00]);
    assert_eq!(field.decoded, [0x77]);
    assert_eq!(field.field_logical.as_deref(), Some(&[0xff][..]));
    assert_eq!(field.value, "255");
}

#[test]
fn sector_inspector_decode_failure_preserves_raw_without_weakening_strict_decode() {
    let context =
        crate::inspect_target::InspectDiskContext::new(vec![0; METADATA_IMAGE_LEN], None, 4096);
    let meta = InspectMeta::default();
    let mut sectors = vec![vec![0; SECTOR]; 101];
    sectors[100] = vec![0x5a; SECTOR];
    let mut reader = MemoryReader {
        sectors: sectors.clone(),
    };
    let fail_soft = AdvancedInspectRequest {
        mode: AdvancedInspectMode::Decode,
        lbas: vec![100],
        export_dir: None,
        device_id_override: None,
        fail_soft_decode: true,
    };
    let workspace = run_advanced_source(
        "memory".into(),
        meta.clone(),
        context.clone(),
        &fail_soft,
        &mut reader,
    )
    .unwrap();
    let item = &workspace.items[0];
    assert_eq!(item.raw, vec![0x5a; SECTOR]);
    assert!(item.decoded.is_none());
    assert!(item
        .decode_error
        .as_deref()
        .is_some_and(|error| { error.contains("不属于已注册 decoder") }));

    let strict = AdvancedInspectRequest {
        fail_soft_decode: false,
        ..fail_soft
    };
    let mut reader = MemoryReader { sectors };
    let error =
        run_advanced_source("memory".into(), meta, context, &strict, &mut reader).unwrap_err();
    assert_eq!(error.kind(), InspectErrorKind::Decode);
    assert!(error.message().contains("不属于已注册 decoder"), "{error}");
}

#[test]
fn inspect_reader_boundary_is_read_only_and_reads_only_requested_raw_sectors() {
    struct AuditReader {
        reads: Vec<u64>,
    }

    impl SectorReader for AuditReader {
        fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
            self.reads.push(lba);
            Ok(vec![(lba & 0xff) as u8; SECTOR])
        }
    }

    let context =
        crate::inspect_target::InspectDiskContext::new(vec![0; METADATA_IMAGE_LEN], None, 4096);
    let request = AdvancedInspectRequest {
        mode: AdvancedInspectMode::Raw,
        lbas: vec![100, 3_000],
        export_dir: None,
        device_id_override: None,
        fail_soft_decode: false,
    };
    let mut reader = AuditReader { reads: Vec::new() };
    let workspace = run_advanced_source(
        "audit-reader".into(),
        InspectMeta::default(),
        context,
        &request,
        &mut reader,
    )
    .unwrap();

    assert_eq!(reader.reads, vec![100, 3_000]);
    assert_eq!(
        workspace
            .items
            .iter()
            .map(|item| item.lba)
            .collect::<Vec<_>>(),
        vec![100, 3_000]
    );
    assert_eq!(workspace.items[0].raw[0], 100);
    assert_eq!(workspace.items[1].raw[0], (3_000 & 0xff) as u8);
}

#[test]
fn advanced_mode_cycles_without_hidden_state() {
    assert_eq!(AdvancedInspectMode::Raw.next(), AdvancedInspectMode::Decode);
    assert_eq!(
        AdvancedInspectMode::Decode.next(),
        AdvancedInspectMode::Meta
    );
    assert_eq!(AdvancedInspectMode::Meta.next(), AdvancedInspectMode::Raw);
    assert_eq!(
        AdvancedInspectMode::Raw.previous(),
        AdvancedInspectMode::Meta
    );
    assert_eq!(
        AdvancedInspectMode::Meta.previous(),
        AdvancedInspectMode::Decode
    );
}

#[test]
fn native_four_kn_aes_cross_inspect_end_to_end_never_reads_or_decodes_partial_sector() {
    use crate::protocol::crypto::{a6b0_full, a7f0_full, aes128_ecb_encrypt_block, crc32_bare};
    use crate::provision::{default_file_key, wrap_file_key, FileKeyWrapMode};

    const DEVICE_ID: &str = "disk&ven_lexar&prod_usb_flash_drive";
    const IMAGE: &[u8; METADATA_IMAGE_LEN] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/protocol/disk4_243625984_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid3164177653_20260827_221910.bin"
    ));

    struct NativeReader {
        start: u64,
        encrypted_boot: Vec<u8>,
        encrypted_data: Vec<u8>,
        reads: Vec<u64>,
    }
    impl SectorReader for NativeReader {
        fn logical_sector_bytes(&self) -> u32 {
            4096
        }
        fn read_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
            self.read_native_sector(lba)
                .map(|sector| sector[..SECTOR].to_vec())
        }
        fn read_native_sector(&mut self, lba: u64) -> io::Result<Vec<u8>> {
            self.reads.push(lba);
            match lba.checked_sub(self.start) {
                Some(0) => Ok(self.encrypted_boot.clone()),
                Some(1) => Ok(self.encrypted_data.clone()),
                _ => Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "only two synthetic native sectors",
                )),
            }
        }
    }

    let mut image = IMAGE.to_vec();
    let mut context = crate::inspect_target::InspectDiskContext::new(
        image.clone(),
        Some(DEVICE_ID.into()),
        243_625_984,
    );
    let index = context
        .partitions
        .iter()
        .position(|p| p.partition_type == 4)
        .unwrap();
    let part = context.partitions[index].clone();
    let key = default_file_key(&image, DEVICE_ID, part.index).unwrap();
    let wrap = wrap_file_key(b"0000aaaa", key, FileKeyWrapMode::Aes128Ecb);
    let crc = crc32_bare(DEVICE_ID.as_bytes());
    let mut decoded_metadata = a6b0_full(&image[12 * SECTOR..13 * SECTOR], &crc.to_le_bytes(), 0);
    decoded_metadata[part.index * 0x60 + 0x30..part.index * 0x60 + 0x48]
        .copy_from_slice(&wrap.packed24());
    decoded_metadata[part.index * 0x60 + 0x58] = 3;
    image[12 * SECTOR..13 * SECTOR].copy_from_slice(&a7f0_full(
        &decoded_metadata,
        &crc.to_le_bytes(),
        0,
    ));
    context.protocol_image = image;
    context.partitions[index].encrypt_mode = 3;
    context.logical_sector_bytes = 4096;

    let mut boot = vec![0u8; 4096];
    boot[0..3].copy_from_slice(&[0xeb, 0x76, 0x90]);
    boot[3..11].copy_from_slice(b"EXFAT   ");
    boot[64..72].copy_from_slice(&part.start_sector.to_le_bytes());
    boot[72..80].copy_from_slice(&part.sector_count.to_le_bytes());
    boot[80..84].copy_from_slice(&24u32.to_le_bytes());
    boot[84..88].copy_from_slice(&1024u32.to_le_bytes());
    boot[88..92].copy_from_slice(&1048u32.to_le_bytes());
    let clusters = ((part.sector_count - 1048) / 8) as u32;
    boot[92..96].copy_from_slice(&clusters.to_le_bytes());
    boot[96..100].copy_from_slice(&2u32.to_le_bytes());
    boot[108..111].copy_from_slice(&[12, 3, 1]);
    boot[112] = 0xff;
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    let second = (0..4096).map(|v| (v * 37 + 13) as u8).collect::<Vec<_>>();
    let encrypt = |plain: &[u8]| {
        plain
            .as_chunks::<16>()
            .0
            .iter()
            .flat_map(|block| aes128_ecb_encrypt_block(block, &key))
            .collect::<Vec<_>>()
    };
    let encrypted_boot = encrypt(&boot);
    let encrypted_data = encrypt(&second);

    let mut reader = NativeReader {
        start: part.start_sector,
        encrypted_boot,
        encrypted_data,
        reads: Vec::new(),
    };
    let request = AdvancedInspectRequest {
        mode: AdvancedInspectMode::Decode,
        lbas: vec![part.start_sector, part.start_sector + 1],
        export_dir: None,
        device_id_override: None,
        fail_soft_decode: false,
    };
    let result = run_advanced_source(
        "isolated-4kn-mode3".into(),
        InspectMeta::default(),
        context.clone(),
        &request,
        &mut reader,
    )
    .unwrap();
    assert_eq!(
        reader.reads,
        vec![part.start_sector, part.start_sector + 1, part.start_sector]
    );
    assert_eq!(result.items.len(), 2);
    assert_eq!(result.items[0].decoded.as_deref(), Some(boot.as_slice()));
    assert_eq!(result.items[1].decoded.as_deref(), Some(second.as_slice()));
    assert_eq!(
        result.items[1].decoded_sha256.as_deref(),
        Some(crate::sha256::sha256_hex(&second).as_str())
    );
    assert!(result.items.iter().all(|item| {
        item.decode_ranges
            .iter()
            .any(|range| range.start == 0 && range.end == 4096)
    }));

    let mut no_key_context = context;
    no_key_context.device_id = None;
    let mut fail_soft_reader = reader;
    let fail_soft_request = AdvancedInspectRequest {
        fail_soft_decode: true,
        lbas: vec![part.start_sector],
        ..request
    };
    let result = run_advanced_source(
        "isolated-4kn-no-key".into(),
        InspectMeta::default(),
        no_key_context,
        &fail_soft_request,
        &mut fail_soft_reader,
    )
    .unwrap();
    assert!(result.items[0].decoded.is_none());
    assert!(result.items[0]
        .decode_error
        .as_deref()
        .is_some_and(|error| error.contains("device_id")));
}

#[test]
fn native_4kn_raw_inspect_preserves_unowned_tail_and_rejects_unknown_decode() {
    struct NativeReader(Vec<u8>);
    impl SectorReader for NativeReader {
        fn logical_sector_bytes(&self) -> u32 {
            4096
        }
        fn read_sector(&mut self, _lba: u64) -> io::Result<Vec<u8>> {
            Ok(self.0[..SECTOR].to_vec())
        }
        fn read_native_sector(&mut self, _lba: u64) -> io::Result<Vec<u8>> {
            Ok(self.0.clone())
        }
    }
    let mut full = vec![0u8; 4096];
    full[..4].copy_from_slice(b"DRKB");
    full[SECTOR..].fill(0xa7);
    let context =
        crate::inspect_target::InspectDiskContext::new_with_partition_table_and_sector_bytes(
            vec![0; METADATA_IMAGE_LEN],
            None,
            100,
            None,
            None,
            None,
            4096,
        );
    let request = AdvancedInspectRequest {
        mode: AdvancedInspectMode::Raw,
        lbas: vec![11],
        export_dir: None,
        device_id_override: None,
        fail_soft_decode: false,
    };
    let workspace = run_advanced_source(
        "virtual-4kn".into(),
        InspectMeta::default(),
        context.clone(),
        &request,
        &mut NativeReader(full.clone()),
    )
    .expect("native block read");
    let item = &workspace.items[0];
    assert_eq!(item.raw, full);
    assert_eq!(item.raw_nonzero, 4 + 3584);
    assert_eq!(item.raw_sha256, crate::sha256::sha256_hex(&full));
    assert!(item.notes.iter().any(|note| note.contains("3584B")));
    let decode = AdvancedInspectRequest {
        mode: AdvancedInspectMode::Decode,
        ..request
    };
    let error = run_advanced_source(
        "virtual-4kn".into(),
        InspectMeta::default(),
        context,
        &decode,
        &mut NativeReader(full),
    )
    .expect_err("unknown 4kn region must fail closed");
    assert_eq!(error.kind(), InspectErrorKind::Decode);
    assert!(error.message().contains("4Kn"));
}
