use edpcli::{
    crypto::{a6b0_full, a7f0_full, crc32_bare, lba6_decode},
    protocol::{
        image::{parse_protocol_image, ProtocolImageContext, PROTOCOL_IMAGE_BYTES},
        lba6::encode_safe6,
        profile::*,
        types::ProtocolError,
    },
};

const NETAC_GOLD: &[u8; PROTOCOL_IMAGE_BYTES] = include_bytes!(
    "../audit/protocol/gold/strict-encrypted/disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin"
);
const DEVICE_ID: &str = "disk&ven_netac&prod_onlydisk";
const VID: &str = "0dd8";
const PID: &str = "2005";
const DISK_SIZE_BYTES: u64 = 122_880_000 * 512;

fn physical_profile() -> ProtocolProfile {
    ProtocolProfile {
        lba0_bootstrap: Lba0Bootstrap::UsbMainBsec,
        lba0_sector_size_overlay: Lba0SectorSizeOverlay::Absent,
        gpt_layout: GptLayout::Absent,
        lba3_metadata: Lba3Metadata::Zero,
        lba4_encoding: Lba4Encoding::OrdinaryRolling,
        dept_layout: DeptLayout::Short,
        lba6_mbr_underlay: Lba6MbrUnderlay::ZeroUnderlay,
        lba7_entry_count: Lba7EntryCount::ThreeEntry,
        lba7_passinfo_version: Lba7PassinfoVersion::LegacyV0064,
        lba8_usb_only_info: Lba8UsbOnlyInfo::StrictLegacyAbsent,
        lba9_eetu: Lba9Eetu::Eetu,
        lba9_overlay: Lba9Overlay::Sapf,
        lba10_eesi: Lba10Eesi::AbsentZero,
        lba11_capacity: Lba11Capacity::DiskSize,
        lba12_mode: Lba12Mode::Mode2,
        lba4_second_key_source: Lba4SecondKeySource::LegacyGuidCrc,
        lba4_hserial_source: Lba4HserialSource::LegacyCallerVector,
        host_hardinfo_source: HostHardinfoSource::LegacyHostIdentity,
    }
}

fn context() -> ProtocolImageContext<'static> {
    ProtocolImageContext {
        profile: physical_profile(),
        device_id: DEVICE_ID,
        vid: VID,
        pid: PID,
        disk_size_bytes: DISK_SIZE_BYTES,
    }
}

fn replace_sector(image: &mut [u8; PROTOCOL_IMAGE_BYTES], lba: usize, sector: &[u8; 512]) {
    image[lba * 512..(lba + 1) * 512].copy_from_slice(sector);
}

#[test]
fn protocol_image_replays_committed_physical_gold_sector_for_sector() {
    let view = parse_protocol_image(NETAC_GOLD, context()).unwrap();

    let replay = [
        view.lba0.reconstruct(),
        view.lba1.reconstruct(),
        view.lba2.reconstruct(),
        view.lba3.reconstruct(),
        view.lba4.reconstruct(),
        view.lba5.reconstruct(),
        view.lba6.reconstruct(),
        view.lba7.reconstruct(),
        view.lba8.reconstruct(),
        view.lba9.reconstruct(),
        view.lba10.reconstruct(),
        view.lba11.reconstruct(),
        view.lba12.reconstruct(),
    ];

    for (lba, sector) in replay.iter().enumerate() {
        assert_eq!(
            sector.as_slice(),
            &NETAC_GOLD[lba * 512..(lba + 1) * 512],
            "LBA{lba} replay diverged from committed physical gold"
        );
    }
    let reencoded = [
        (4usize, view.lba4.encode().unwrap()),
        (6, view.lba6.reencode()),
        (7, view.lba7.reencode()),
        (8, view.lba8.reencode()),
        (9, view.lba9.reencode()),
        (10, view.lba10.reencode()),
        (11, view.lba11.reencode()),
        (12, view.lba12.reencode()),
    ];
    for (lba, sector) in reencoded {
        assert_eq!(
            sector.as_slice(),
            &NETAC_GOLD[lba * 512..(lba + 1) * 512],
            "LBA{lba} typed serializer diverged from committed physical gold"
        );
    }
}

#[test]
fn protocol_image_rejects_lba6_device_crc_that_disagrees_with_context() {
    let mut image = *NETAC_GOLD;
    let raw6: &[u8; 512] = image[6 * 512..7 * 512].try_into().unwrap();
    let mut plain6: [u8; 512] = lba6_decode(raw6).try_into().unwrap();
    let wrong_crc = crc32_bare(DEVICE_ID.as_bytes()).wrapping_add(1);
    plain6[0x100..0x104].copy_from_slice(&wrong_crc.to_le_bytes());
    plain6[0x104..0x108].copy_from_slice(&wrong_crc.wrapping_mul(2).to_le_bytes());
    replace_sector(&mut image, 6, &encode_safe6(&plain6));

    assert!(matches!(
        parse_protocol_image(&image, context()),
        Err(ProtocolError::InvalidField {
            lba: 6,
            field: "device_crc_cross_lba"
        })
    ));
}

#[test]
fn protocol_image_rejects_lba6_snapshot_geometry_that_disagrees_with_lba12() {
    let mut image = *NETAC_GOLD;
    let crc = crc32_bare(DEVICE_ID.as_bytes());

    let raw12: &[u8; 512] = image[12 * 512..13 * 512].try_into().unwrap();
    let plain12: [u8; 512] = a6b0_full(raw12, &crc.to_le_bytes(), 0).try_into().unwrap();
    let count = u32::from_le_bytes(plain12[8..12].try_into().unwrap()) as usize;
    let type4 = (0..count)
        .map(|index| &plain12[index * 0x60..(index + 1) * 0x60])
        .find(|entry| u32::from_le_bytes(entry[0x0c..0x10].try_into().unwrap()) == 4)
        .unwrap();
    let type4_start = u64::from_le_bytes(type4[0x18..0x20].try_into().unwrap());
    let type4_sectors = u64::from_le_bytes(type4[0x28..0x30].try_into().unwrap()) / 512;

    let raw6: &[u8; 512] = image[6 * 512..7 * 512].try_into().unwrap();
    let mut plain6: [u8; 512] = lba6_decode(raw6).try_into().unwrap();
    plain6[0x1e0..0x1e6].copy_from_slice(&[0xc1, 0xff, 0x07, 0xef, 0xff, 0xff]);
    plain6[0x1e6..0x1ea].copy_from_slice(&(type4_start as u32 + 1).to_le_bytes());
    plain6[0x1ea..0x1ee].copy_from_slice(&(type4_sectors as u32).to_le_bytes());
    replace_sector(&mut image, 6, &encode_safe6(&plain6));

    let mut context = context();
    context.profile.lba6_mbr_underlay = Lba6MbrUnderlay::LegacyMbrSnapshot;
    assert!(matches!(
        parse_protocol_image(&image, context),
        Err(ProtocolError::InvalidField {
            lba: 12,
            field: "legacy_mbr_snapshot_cross_lba"
        })
    ));
}

#[test]
fn protocol_image_rejects_lba12_partition_type_that_disagrees_with_lba7() {
    let mut image = *NETAC_GOLD;
    let crc = crc32_bare(DEVICE_ID.as_bytes());
    let raw12: &[u8; 512] = image[12 * 512..13 * 512].try_into().unwrap();
    let mut plain12: [u8; 512] = a6b0_full(raw12, &crc.to_le_bytes(), 0).try_into().unwrap();
    let current_type = u32::from_le_bytes(plain12[0x0c..0x10].try_into().unwrap());
    let wrong_type = if current_type == 1 { 2u32 } else { 1u32 };
    plain12[0x0c..0x10].copy_from_slice(&wrong_type.to_le_bytes());
    let wire12: [u8; 512] = a7f0_full(&plain12, &crc.to_le_bytes(), 0)
        .try_into()
        .unwrap();
    replace_sector(&mut image, 12, &wire12);

    assert!(matches!(
        parse_protocol_image(&image, context()),
        Err(ProtocolError::InvalidField {
            lba: 12,
            field: "edpf_type_cross_lba"
        })
    ));
}
