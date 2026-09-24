use edpcli::platform::{HardwareProbe, InquiryInfo, NativeTransport};
use edpcli::provision::{
    OnlyId, ProvisionImage, ProvisionMetadata, ProvisionProfile, ProvisionSpec, TargetIdentity,
    PROVISION_IMAGE_LEN,
};

fn probe(transport: NativeTransport) -> HardwareProbe {
    HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport,
        inquiry: Some(InquiryInfo {
            vendor: "Netac  ".into(),
            product: "OnlyDisk".into(),
            revision: "1.00".into(),
        }),
    }
}

#[test]
fn onlyid_accepts_real_signed_and_unsigned_32_bit_text_forms() {
    let unsigned = OnlyId::parse("3164177653").unwrap();
    assert_eq!(unsigned.text(), "3164177653");
    assert_eq!(unsigned.bits(), 3_164_177_653u32);

    let signed = OnlyId::parse("-1833210541").unwrap();
    assert_eq!(signed.text(), "-1833210541");
    assert_eq!(signed.bits(), (-1_833_210_541i32) as u32);

    assert!(OnlyId::parse("4294967296").is_err());
    assert!(OnlyId::parse("-2147483649").is_err());
    assert!(OnlyId::parse("+1").is_err());
    assert!(OnlyId::parse("abc").is_err());
}

#[test]
fn generated_onlyid_candidate_is_nonzero_and_round_trips_through_the_existing_parser() {
    let candidate = OnlyId::random_candidate().expect("onlyid candidate");
    assert_ne!(candidate.bits(), 0);
    assert_eq!(
        OnlyId::parse(candidate.text()).unwrap().bits(),
        candidate.bits()
    );
}

#[test]
fn target_identity_is_derived_from_complete_hardware_probe() {
    let uas = TargetIdentity::from_probe(&probe(NativeTransport::Uas), 122_880_000).unwrap();
    assert_eq!(uas.device_id(), "disk&ven_netac&prod_onlydisk");
    assert_eq!(uas.vid_hex(), "0dd8");
    assert_eq!(uas.pid_hex(), "2005");
    assert_eq!(uas.total_sectors(), 122_880_000);

    let bot = TargetIdentity::from_probe(&probe(NativeTransport::Bot), 122_880_000).unwrap();
    assert_eq!(bot.device_id(), "disk&ven_netac&prod_onlydisk&rev_1.00");
}

#[test]
fn target_identity_fails_closed_when_probe_is_ambiguous() {
    let mut missing_vid = probe(NativeTransport::Uas);
    missing_vid.vid = None;
    assert!(TargetIdentity::from_probe(&missing_vid, 122_880_000).is_err());

    let mut missing_inquiry = probe(NativeTransport::Uas);
    missing_inquiry.inquiry = None;
    assert!(TargetIdentity::from_probe(&missing_inquiry, 122_880_000).is_err());

    assert!(TargetIdentity::from_probe(&probe(NativeTransport::Unknown), 122_880_000).is_err());

    let mut bot_without_revision = probe(NativeTransport::Bot);
    bot_without_revision
        .inquiry
        .as_mut()
        .unwrap()
        .revision
        .clear();
    assert!(TargetIdentity::from_probe(&bot_without_revision, 122_880_000).is_err());
}

#[test]
fn spec_has_explicit_metadata_and_versioned_profile() {
    let target = TargetIdentity::from_probe(&probe(NativeTransport::Uas), 122_880_000).unwrap();
    let metadata = ProvisionMetadata::new(
        OnlyId::parse("1402259934").unwrap(),
        "宋旭琳",
        "江苏省电力有限公司/泰州供电公司/输电运检中心",
        "江苏电力!SAFE6",
    )
    .unwrap();
    let profile = ProvisionProfile::canonical_v1();
    assert!(!profile.id().is_empty());
    assert_eq!(profile.version(), 1);

    let spec = ProvisionSpec::new(target, metadata, profile).unwrap();
    assert_eq!(spec.profile().version(), 1);
    assert_eq!(spec.metadata().onlyid().text(), "1402259934");
}

#[test]
fn metadata_rejects_empty_or_unrepresentable_text() {
    let onlyid = OnlyId::parse("1").unwrap();
    assert!(ProvisionMetadata::new(onlyid.clone(), "", "部门", "标签").is_err());
    assert!(ProvisionMetadata::new(onlyid.clone(), "用户", "", "标签").is_err());
    assert!(ProvisionMetadata::new(onlyid.clone(), "用户", "部门", "").is_err());
    assert!(
        ProvisionMetadata::new(onlyid, "🙂", "部门", "标签").is_err(),
        "metadata must be losslessly representable in GBK"
    );
}

#[test]
fn provision_image_is_always_exactly_thirteen_sectors() {
    assert_eq!(PROVISION_IMAGE_LEN, 13 * 512);
    assert!(ProvisionImage::from_bytes(vec![0u8; PROVISION_IMAGE_LEN]).is_ok());
    assert!(ProvisionImage::from_bytes(vec![0u8; PROVISION_IMAGE_LEN - 1]).is_err());
    assert!(ProvisionImage::from_bytes(vec![0u8; PROVISION_IMAGE_LEN + 1]).is_err());
}
