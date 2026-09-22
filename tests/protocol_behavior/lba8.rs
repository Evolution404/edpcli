use edpcli::{
    crypto::{a6b0_full, a7f0_full, crc32_bare},
    protocol::{
        lba4::{parse_lba4, Lba4Context},
        lba8::{parse_lba8, Lba8Context, UsbOnlyInfo},
        profile::{HostHardinfoSource, Lba8UsbOnlyInfo},
    },
};

fn device_id_for(profile: &str, sample: &str) -> String {
    if profile == "authentic-nopwd" {
        "disk&ven_sandisk&prod_ultra&rev_1.00".into()
    } else {
        crate::gold_name::parse_gold_name(sample).unwrap().device_id
    }
}

#[test]
pub fn lba8_dynamic_prefix_and_identity_profiles_preserve_backing() {
    let mut current = 0usize;
    let mut strict_legacy = 0usize;
    let mut current_fixture = None;

    for row in include_str!("../../audit/protocol/gold_samples.tsv")
        .lines()
        .skip(1)
    {
        let c: Vec<_> = row.split('\t').collect();
        let image =
            std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(c[4])).unwrap();
        let raw4: &[u8; 512] = image[4 * 512..5 * 512].try_into().unwrap();
        let raw8: &[u8; 512] = image[8 * 512..9 * 512].try_into().unwrap();
        let onlyid = parse_lba4(raw4, Lba4Context::default()).unwrap().onlyid;
        let device_id = device_id_for(c[0], c[3]);
        let crc = crc32_bare(device_id.as_bytes());
        let head = a6b0_full(&raw8[..0x40], &crc.to_le_bytes(), 0);
        let host_hardinfo = u32::from_le_bytes(head[0x14..0x18].try_into().unwrap());
        let context = if host_hardinfo == 0 {
            current += 1;
            Lba8Context {
                usb_only_info: Lba8UsbOnlyInfo::Current,
                host_hardinfo_source: HostHardinfoSource::CurrentZero,
                main_onlyid: Some(onlyid),
            }
        } else {
            strict_legacy += 1;
            Lba8Context {
                usb_only_info: Lba8UsbOnlyInfo::StrictLegacyAbsent,
                host_hardinfo_source: HostHardinfoSource::LegacyHostIdentity,
                main_onlyid: Some(onlyid),
            }
        };
        let view = parse_lba8(raw8, crc, context).unwrap();
        assert_eq!(view.reconstruct(), *raw8);
        assert_eq!(view.reencode(), *raw8);
        assert_eq!(view.logical_length as usize, 0x80 + view.elabel_body.len());
        assert_eq!(
            view.encrypted_len(),
            (view.logical_length as usize / 16 + 1) * 16
        );
        assert!(view.elabel_body.starts_with(b"<ELABEL>"));
        assert!(view
            .elabel_body
            .windows(b"||VOLC2=".len())
            .any(|window| window == b"||VOLC2="));
        assert_eq!(view.tool_version, [1, 0, 0, 1]);
        assert_eq!(view.lab_version, 0x222);
        assert_eq!(view.mac_info, [0; 6]);
        assert_eq!(view.usb_only_suffix, [0; 16]);
        assert_eq!(view.elab_offset, 0x80);
        assert_eq!(view.reserved_header, [0; 64]);
        match (&view.usb_only_info, host_hardinfo) {
            (UsbOnlyInfo::Current(_), 0) => {}
            (UsbOnlyInfo::StrictLegacyAbsent, value) if value != 0 => {}
            other => panic!("unexpected LBA8 identity profile: {other:?}"),
        }
        if host_hardinfo == 0 && current_fixture.is_none() {
            current_fixture = Some((*raw8, crc, onlyid, context, view));
        }
    }
    assert!(current > 0);
    assert!(strict_legacy > 0);

    let (raw, crc, onlyid, context, view) = current_fixture.expect("current LBA8 physical fixture");
    let mut plain = *view.mixed_plain();
    let logical = view.logical_length as usize;
    if logical + 1 < view.encrypted_len() {
        plain[logical + 1] = 0xa5;
    }
    let mut synthetic = raw;
    synthetic[..view.encrypted_len()].copy_from_slice(&a7f0_full(
        &plain[..view.encrypted_len()],
        &crc.to_le_bytes(),
        0,
    ));
    if view.encrypted_len() < 512 {
        synthetic[view.encrypted_len()] = 0x5a;
    }
    let preserved = parse_lba8(&synthetic, crc, context).unwrap();
    if logical + 1 < preserved.encrypted_len() {
        assert_eq!(preserved.encrypted_backing[0], 0xa5);
    }
    if preserved.encrypted_len() < 512 {
        assert_eq!(preserved.tail_backing[0], 0x5a);
    }
    assert_eq!(preserved.reencode(), synthetic);

    let host = 0x1234_5678u32;
    let mut transition_plain = *view.mixed_plain();
    transition_plain[0x14..0x18].copy_from_slice(&host.to_le_bytes());
    transition_plain[0x1e..0x2e].copy_from_slice(format!("{onlyid:08x}{host:08x}").as_bytes());
    let mut transition_wire = raw;
    transition_wire[..view.encrypted_len()].copy_from_slice(&a7f0_full(
        &transition_plain[..view.encrypted_len()],
        &crc.to_le_bytes(),
        0,
    ));
    let transitional = parse_lba8(
        &transition_wire,
        crc,
        Lba8Context {
            usb_only_info: Lba8UsbOnlyInfo::Transitional2019,
            host_hardinfo_source: HostHardinfoSource::LegacyHostIdentity,
            main_onlyid: Some(onlyid),
        },
    )
    .unwrap();
    assert_eq!(transitional.host_hardinfo, host);
    assert!(matches!(
        transitional.usb_only_info,
        UsbOnlyInfo::Transitional(_)
    ));
    assert_eq!(transitional.reencode(), transition_wire);
}

#[test]
pub fn lba8_profile_axes_fail_closed_without_coupling() {
    let raw = [0u8; 512];
    assert!(parse_lba8(&raw, 0, Lba8Context::default()).is_err());
}
