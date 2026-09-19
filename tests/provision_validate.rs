use edpcli::platform::{HardwareProbe, InquiryInfo, NativeTransport};
use edpcli::provision::{
    generate_image, OnlyId, ProvisionEntropy, ProvisionMetadata, ProvisionProfile, ProvisionSpec,
    ProvisionValidator, TargetIdentity, PROVISION_IMAGE_LEN,
};

fn spec() -> ProvisionSpec {
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
        OnlyId::parse("1402259934").unwrap(),
        "宋旭琳",
        "江苏省电力有限公司/泰州供电公司/输电运检中心",
        "江苏电力!SAFE6",
    )
    .unwrap();
    ProvisionSpec::new(target, metadata, ProvisionProfile::canonical_v1()).unwrap()
}

fn entropy() -> ProvisionEntropy {
    ProvisionEntropy::new([0x5a; 256])
}

#[test]
fn validator_accepts_generated_image_and_reports_identity() {
    let spec = spec();
    let image = generate_image(&spec, &entropy()).unwrap();
    let report = ProvisionValidator::validate(&spec, &image).unwrap();
    assert_eq!(report.profile_id(), "jiangsu-safe6-nopwd");
    assert_eq!(report.device_id(), spec.target().device_id());
    assert_eq!(report.onlyid(), "1402259934");
    assert!(report.is_nopwd());
}

#[test]
fn validator_rejects_wrong_length_before_protocol_parsing() {
    let spec = spec();
    let err = ProvisionValidator::validate_bytes(&spec, &[0u8; 12 * 512]).unwrap_err();
    assert!(err.contains(&PROVISION_IMAGE_LEN.to_string()), "{err}");
}

#[test]
fn validator_rejects_reserved_sector_tamper() {
    let spec = spec();
    let image = generate_image(&spec, &entropy()).unwrap();
    let mut bytes = image.as_bytes().to_vec();
    bytes[3 * 512 + 17] = 1;
    let err = ProvisionValidator::validate_bytes(&spec, &bytes).unwrap_err();
    assert!(err.contains("LBA3"), "{err}");
}

#[test]
fn validator_rejects_mbr_and_lba12_tail_tamper() {
    let spec = spec();
    let image = generate_image(&spec, &entropy()).unwrap();

    let mut bad_mbr = image.as_bytes().to_vec();
    bad_mbr[0x1be + 4] = 0x0b;
    let err = ProvisionValidator::validate_bytes(&spec, &bad_mbr).unwrap_err();
    assert!(err.contains("MBR"), "{err}");

    let mut bad_tail = image.as_bytes().to_vec();
    bad_tail[12 * 512 + 0x170] ^= 1;
    let err = ProvisionValidator::validate_bytes(&spec, &bad_tail).unwrap_err();
    assert!(err.contains("LBA12 decoded tail"), "{err}");
}

#[test]
fn validator_rejects_image_for_different_hardware_identity() {
    let spec = spec();
    let image = generate_image(&spec, &entropy()).unwrap();

    let mut other_probe = HardwareProbe {
        vid: Some(0x21c4),
        pid: Some(0x0cd1),
        transport: NativeTransport::Uas,
        inquiry: Some(InquiryInfo {
            vendor: "Lexar".into(),
            product: "USB Flash Drive".into(),
            revision: "1.00".into(),
        }),
    };
    let other_target = TargetIdentity::from_probe(&other_probe, 122_880_000).unwrap();
    let other_metadata = spec.metadata().clone();
    let other_spec = ProvisionSpec::new(
        other_target,
        other_metadata,
        ProvisionProfile::canonical_v1(),
    )
    .unwrap();
    let err = ProvisionValidator::validate(&other_spec, &image).unwrap_err();
    assert!(
        err.contains("device_id")
            || err.contains("LBA6")
            || err.contains("LBA7")
            || err.contains("LBA12"),
        "{err}"
    );

    other_probe.inquiry.as_mut().unwrap().product = "Other".into();
}
