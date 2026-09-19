use edpcli::crypto::{a7f0_full, crc32_bare};
use edpcli::inspect::{analyze_sector, InspectMeta};
use edpcli::metainfo::ownership_from_lba8;
use edpcli::platform::{HardwareProbe, InquiryInfo, NativeTransport};
use edpcli::provision::{
    generate_image, OnlyId, ProvisionEntropy, ProvisionMetadata, ProvisionProfile, ProvisionSpec,
    TargetIdentity,
};
use edpcli::sectors::looks_nopwd;

fn spec(onlyid: &str) -> ProvisionSpec {
    let probe = HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: NativeTransport::Uas,
        inquiry: Some(InquiryInfo {
            vendor: "Netac".into(),
            product: "OnlyDisk".into(),
            revision: "1.00".into(),
        }),
    };
    let target = TargetIdentity::from_probe(&probe, 122_880_000).unwrap();
    let metadata = ProvisionMetadata::new(
        OnlyId::parse(onlyid).unwrap(),
        "宋旭琳",
        "江苏省电力有限公司/泰州供电公司/输电运检中心",
        "江苏电力!SAFE6",
    )
    .unwrap();
    ProvisionSpec::new(target, metadata, ProvisionProfile::canonical_v1()).unwrap()
}

fn entropy() -> ProvisionEntropy {
    let mut random = [0u8; 256];
    for (index, byte) in random.iter_mut().enumerate() {
        *byte = index as u8;
    }
    ProvisionEntropy::new([1, 2, 3, 4, 5, 6, 7, 8], random)
}

fn sector(image: &[u8], lba: usize) -> &[u8] {
    &image[lba * 512..(lba + 1) * 512]
}

#[test]
fn generated_image_is_structurally_complete_nopwd_metadata() {
    let spec = spec("1402259934");
    let image = generate_image(&spec, &entropy()).unwrap();
    let bytes = image.as_bytes();

    for lba in [1usize, 2, 3, 5, 9, 10] {
        assert!(sector(bytes, lba).iter().all(|byte| *byte == 0), "LBA{lba}");
    }

    let mbr = sector(bytes, 0);
    assert_eq!(&mbr[0x1fe..0x200], &[0x55, 0xaa]);
    assert_eq!(mbr[0x1be + 4], 0x07);
    assert_eq!(
        u32::from_le_bytes(mbr[0x1be + 8..0x1be + 12].try_into().unwrap()),
        63
    );

    let meta = InspectMeta {
        device_id: Some(spec.target().device_id().into()),
        vid: Some(spec.target().vid_hex()),
        pid: Some(spec.target().pid_hex()),
        size_bytes: Some(spec.target().total_sectors() * 512),
        onlyid: Some(spec.metadata().onlyid().text().into()),
    };
    let lba4 = analyze_sector(4, sector(bytes, 4), &meta);
    assert!(lba4.method.contains("labelOnlyId=1402259934"));
    assert_eq!(&lba4.decoded[0x39..0x3d], b"LLGB");

    let lba6 = analyze_sector(6, sector(bytes, 6), &meta);
    assert_eq!(&lba6.decoded[0x1c0..0x1c8], b"322CA28A");
    assert_eq!(lba6.decoded[0x1c8], 0);
    assert!(lba6.decoded[0x1c9..0x1d0].iter().all(|byte| *byte == 0));
    assert!(lba6.decoded[0x1d0..0x1f0].iter().all(|byte| *byte == 0));
    assert_eq!(
        u32::from_le_bytes(lba6.decoded[0x1f0..0x1f4].try_into().unwrap()),
        1
    );
    assert!(lba6
        .fields
        .iter()
        .any(|field| { field.label == "device_id CRC32" && field.value.contains('✓') }));
    assert!(lba6
        .fields
        .iter()
        .any(|field| field.label == "校验和" && field.value.contains('✓')));

    let lba7 = analyze_sector(7, sector(bytes, 7), &meta);
    assert_eq!(&lba7.decoded[0..4], b"EDPF");
    assert_eq!(
        u32::from_le_bytes(lba7.decoded[0x0c..0x10].try_into().unwrap()),
        2
    );
    assert_eq!(
        u32::from_le_bytes(lba7.decoded[0x4c..0x50].try_into().unwrap()),
        4
    );
    assert!(lba7.decoded[0x80..0xc0].iter().all(|byte| *byte == 0));

    let ownership = ownership_from_lba8(sector(bytes, 8), &meta).unwrap();
    assert_eq!(ownership.user.as_deref(), Some("宋旭琳"));
    assert_eq!(
        ownership.dept.as_deref(),
        Some("江苏省电力有限公司/泰州供电公司/输电运检中心")
    );

    let lba11 = analyze_sector(11, sector(bytes, 11), &meta);
    assert_eq!(&lba11.decoded[0x100..0x104], b"PDKB");
    assert!(lba11
        .fields
        .iter()
        .any(|field| field.label == "PDKB device_id" && field.value == spec.target().device_id()));

    let lba12 = analyze_sector(12, sector(bytes, 12), &meta);
    assert_eq!(
        u32::from_le_bytes(lba12.decoded[0x0c..0x10].try_into().unwrap()),
        2
    );
    assert_eq!(
        u32::from_le_bytes(lba12.decoded[0x6c..0x70].try_into().unwrap()),
        4
    );
    assert!(lba12.decoded[0xc0..0x120].iter().all(|byte| *byte == 0));
    let crc = crc32_bare(spec.target().device_id().as_bytes());
    let expected_tail = a7f0_full(&[0u8; 144], &crc.to_le_bytes(), 0x170);
    assert_eq!(&sector(bytes, 12)[0x170..], expected_tail.as_slice());

    let read = |lba: u32| -> edpcli::common::EdpCliResult<Vec<u8>> {
        Ok(sector(bytes, lba as usize).to_vec())
    };
    assert!(looks_nopwd(&read, spec.target().device_id()).unwrap());
}

#[test]
fn signed_onlyid_round_trips_through_generated_lba4() {
    let spec = spec("-1833210541");
    let image = generate_image(&spec, &entropy()).unwrap();
    let meta = InspectMeta {
        device_id: Some(spec.target().device_id().into()),
        vid: Some(spec.target().vid_hex()),
        pid: Some(spec.target().pid_hex()),
        size_bytes: Some(spec.target().total_sectors() * 512),
        onlyid: Some(spec.metadata().onlyid().text().into()),
    };
    let lba4 = analyze_sector(4, sector(image.as_bytes(), 4), &meta);
    assert!(lba4.method.contains("labelOnlyId=-1833210541"));
    assert_eq!(&lba4.decoded[0x39..0x3d], b"LLGB");
}
