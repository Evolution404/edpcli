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
