use std::io;

use edpcli::{
    backup_deep::{analyze_partition, AnalysisStatus, PartitionReader},
    backup_metadata::PartitionGeometry,
    crypto::{a6b0_full, crc32_bare, xor_rolling},
    platform::{HardwareProbe, InquiryInfo, NativeTransport},
    protocol::lba7_compat::locate_lba7_compatibility_extent_from_geometry,
    provision::{
        build_passwordless_conversion, generate_official_image, wrap_file_key,
        wrap_legacy_lba7_file_key, FileKeyWrapMode, OfficialPartitionMode, OfficialPartitionSizes,
        OfficialProvisionPlan, OnlyId, ProvisionEntropy, ProvisionImage, ProvisionMetadata,
        ProvisionProfile, ProvisionSpec, SparseFilesystemImage, TargetIdentity,
    },
};

const SECTOR: usize = 512;
const E7: usize = 0x40;
const E12: usize = 0x60;
const FILE_KEY: [u8; 16] = [
    0x14, 0x71, 0x96, 0xf5, 0xa2, 0xec, 0x79, 0x12, 0xed, 0xf1, 0x3f, 0x75, 0xd7, 0x66, 0xcb, 0x42,
];

struct SparseReader<'a>(&'a SparseFilesystemImage);

impl PartitionReader for SparseReader<'_> {
    fn read_sector(&mut self, relative_lba: u64) -> io::Result<Vec<u8>> {
        self.0
            .sector_or_zero(relative_lba)
            .map(|sector| sector.to_vec())
            .ok_or_else(|| io::Error::other("sector out of range"))
    }
}

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
    let target = TargetIdentity::from_probe(&probe, 16_777_216).unwrap();
    let metadata = ProvisionMetadata::new(
        OnlyId::parse("1402259934").unwrap(),
        "USER06",
        "江苏省电力有限公司",
        "江苏电力!SAFE6",
    )
    .unwrap();
    ProvisionSpec::new(target, metadata, ProvisionProfile::canonical_v1()).unwrap()
}

fn plan(mode: OfficialPartitionMode) -> OfficialProvisionPlan {
    let compat = locate_lba7_compatibility_extent_from_geometry(1024, 255, 63, 512).unwrap();
    OfficialProvisionPlan::new(
        mode,
        OfficialPartitionSizes::new(32, 64, 128),
        compat,
        wrap_legacy_lba7_file_key(
            b"0000aaaa",
            [0x7d, 0x9e, 0xe4, 0xe8, 0x75, 0x4a, 0xd4, 0x38],
        ),
        wrap_file_key(b"ProofPass1!", FILE_KEY, FileKeyWrapMode::Sm4),
    )
    .unwrap()
}

fn source(mode: OfficialPartitionMode) -> (ProvisionSpec, ProvisionImage) {
    let spec = spec();
    let image =
        generate_official_image(&spec, &ProvisionEntropy::new([0x5a; 252]), &plan(mode)).unwrap();
    (spec, image)
}

fn u32le(raw: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap())
}

fn u64le(raw: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(raw[offset..offset + 8].try_into().unwrap())
}

#[test]
fn strict_conversion_preserves_type4_and_rebuilds_only_the_front_filesystem() {
    let (spec, source) = source(OfficialPartitionMode::DefaultThreePartition);
    let did = spec.target().device_id();
    let converted = build_passwordless_conversion(&source, did, 0x1234_5678, "SAFE6").unwrap();
    let crc = crc32_bare(did.as_bytes());

    let source12 = a6b0_full(
        &source.as_bytes()[12 * SECTOR..13 * SECTOR],
        &crc.to_le_bytes(),
        0,
    );
    let target12 = a6b0_full(&converted.lba12, &crc.to_le_bytes(), 0);
    let source_type4_12 = 2 * E12;
    let target_type4_12 = E12;

    assert_eq!(u32le(&target12, 0x08), 2);
    assert_eq!(u32le(&target12, 0x0c), 2);
    assert_eq!(u32le(&target12, E12 + 0x0c), 4);
    assert_eq!(u64le(&target12, 0x18), 63);
    assert_eq!(u64le(&target12, 0x28), converted.plan.front_sectors * 512);
    assert_eq!(
        &target12[0x30..0x60],
        &source12[source_type4_12 + 0x30..source_type4_12 + 0x60],
        "front type2 must reuse the source type4 key material"
    );
    assert_eq!(
        &target12[target_type4_12 + 0x18..target_type4_12 + 0x60],
        &source12[source_type4_12 + 0x18..source_type4_12 + 0x60],
        "type4 geometry and key material must be byte-for-byte preserved"
    );
    assert!(target12[2 * E12..3 * E12].iter().all(|byte| *byte == 0));
    assert_eq!(&target12[0x120..], &source12[0x120..]);

    let k0 = (crc & 0xffff) ^ (crc >> 16);
    let source7 = xor_rolling(&source.as_bytes()[7 * SECTOR..8 * SECTOR], k0);
    let target7 = xor_rolling(&converted.lba7, k0);
    let source_type4_7 = 2 * E7;
    let target_type4_7 = E7;
    assert_eq!(u32le(&target7, 0x08), 2);
    assert_eq!(u32le(&target7, 0x0c), 2);
    assert_eq!(u32le(&target7, E7 + 0x0c), 4);
    assert_eq!(
        &target7[0x30..0x40],
        &source7[source_type4_7 + 0x30..source_type4_7 + 0x40]
    );
    assert_eq!(
        &target7[target_type4_7 + 0x18..target_type4_7 + 0x40],
        &source7[source_type4_7 + 0x18..source_type4_7 + 0x40]
    );
    assert!(target7[2 * E7..3 * E7].iter().all(|byte| *byte == 0));
    assert_eq!(&target7[0xc0..], &source7[0xc0..]);

    assert_eq!(converted.lba0[0x1be + 4], 0x07);
    assert_eq!(u32le(&converted.lba0, 0x1be + 8), 63);
    assert_eq!(
        u32le(&converted.lba0, 0x1be + 12) as u64,
        converted.plan.front_sectors
    );
    assert_eq!(
        converted.plan.encrypt_start_lba,
        u64le(&source12, source_type4_12 + 0x18)
    );
    assert_eq!(
        converted.plan.encrypt_size_bytes,
        u64le(&source12, source_type4_12 + 0x28)
    );

    let partition = PartitionGeometry {
        index: 0,
        partition_type: 2,
        partition_count: 2,
        need_disturb: 1,
        need_encrypt: 0,
        start_sector: converted.plan.front_start_lba,
        sector_size: 512,
        partition_size: converted.plan.front_sectors * 512,
        sector_count: converted.plan.front_sectors,
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    };
    let mut reader = SparseReader(&converted.front_filesystem);
    let report = analyze_partition(&partition, &mut reader);
    assert_eq!(report.status, AnalysisStatus::Parsed, "{}", report.reason);
    assert_eq!(report.filesystem.as_deref(), Some("exfat"));
    assert_eq!(report.file_count, Some(0));
}

#[test]
fn strict_conversion_rejects_wrong_device_id_missing_type4_and_already_target_layout() {
    let (spec, mode0) = source(OfficialPartitionMode::DefaultThreePartition);
    let err =
        build_passwordless_conversion(&mode0, "disk&ven_wrong&prod_wrong", 1, "SAFE6").unwrap_err();
    assert!(err.contains("EDPF") || err.contains("type4"), "{err}");

    let (_, mode3) = source(OfficialPartitionMode::IntranetExtranetDualPartition);
    let err =
        build_passwordless_conversion(&mode3, spec.target().device_id(), 1, "SAFE6").unwrap_err();
    assert!(err.contains("type4"), "{err}");

    let (_, mode1) = source(OfficialPartitionMode::BootShareCombined);
    let err =
        build_passwordless_conversion(&mode1, spec.target().device_id(), 1, "SAFE6").unwrap_err();
    assert!(err.contains("already"), "{err}");
}
