//! macOS-only guarded 4096-byte *OS block device* HIL. Script creates an
//! ephemeral disk image, verifies diskutil identity and exports its raw path.
//! No USB media ever qualifies for ci_prepare_virtual_write.
#![cfg(all(feature = "ci-virtual-disk", target_os = "macos"))]

// Reuse established macOS HIL entrypoint for independent CLI crypto verification.
#[path = "hil/native_cli_crypto_hil.rs"]
mod native_cli_crypto_hil;

use edpcli::application::filesystem::{FilesystemKind, NativeVirtualDiskPlan};
use edpcli::application::provision::native_image::{
    plan_native_edp_image, plan_native_plain_image,
};
use edpcli::application::provision::{FormatOptions, PlainPartitionRequest, PlainPartitionSize};
use edpcli::diskio::{
    inspect_native_journal, NativeBlockDevice, NativeJournalState, NativeRawBlockDevice,
};
use edpcli::platform::{self, HardwareProbe, InquiryInfo, NativeTransport};
use edpcli::protocol::crypto::a6b0_full;
use edpcli::protocol::image::NativeProtocolImage;
use edpcli::protocol::lba7_compat::Lba7CompatibilityExtentLayout;
use edpcli::provision::{
    parse_existing_provision_native, wrap_file_key, wrap_legacy_lba7_file_key, DiskProvisionKind,
    FileKeyWrapMode, OfficialPartitionMode, OfficialPartitionSizes, OfficialProvisionPlan, OnlyId,
    ProvisionEntropy, ProvisionMetadata, ProvisionProfile, ProvisionSpec, TargetIdentity,
};
use std::path::PathBuf;

const BLOCK: u32 = 4096;
const SECRET: [u8; 16] = [0x42; 16];

fn mode(s: &str) -> DiskProvisionKind {
    match s {
        "Plain" => DiskProvisionKind::Plain,
        "Mode0" => DiskProvisionKind::Mode0,
        "Mode1" => DiskProvisionKind::Mode1,
        "Mode2" => DiskProvisionKind::Mode2,
        "Mode3" => DiskProvisionKind::Mode3,
        _ => panic!("unrecognized 4Kn HIL target mode {s}"),
    }
}

struct Handoff {
    raw: PathBuf,
    total: u64,
    mode: DiskProvisionKind,
    action: String,
    wal: PathBuf,
}
impl Handoff {
    fn from_env() -> Self {
        let raw = PathBuf::from(std::env::var("EDPCLI_4KN_HIL_RAW").unwrap());
        let bytes: u64 = std::env::var("EDPCLI_4KN_HIL_BYTES")
            .unwrap()
            .parse()
            .unwrap();
        assert!(bytes >= 256 * 1024 * 1024 && bytes.is_multiple_of(u64::from(BLOCK)));
        let path = raw.to_string_lossy();
        assert!(
            path.starts_with("/dev/rdisk")
                && path["/dev/rdisk".len()..]
                    .chars()
                    .all(|ch| ch.is_ascii_digit())
        );
        Self {
            raw,
            total: bytes / u64::from(BLOCK),
            mode: mode(&std::env::var("EDPCLI_4KN_HIL_MODE").unwrap()),
            action: std::env::var("EDPCLI_4KN_HIL_ACTION").unwrap(),
            wal: PathBuf::from(std::env::var("EDPCLI_4KN_HIL_WAL").unwrap()),
        }
    }
}
fn build_plan(
    h: &Handoff,
) -> (
    NativeVirtualDiskPlan,
    Option<(
        OfficialProvisionModeInfo,
        Vec<(u64, u64, FilesystemKind, bool)>,
    )>,
) {
    if h.mode == DiskProvisionKind::Plain {
        let plan = plan_native_plain_image(
            h.total,
            BLOCK,
            &[PlainPartitionRequest {
                start_lba: 2048,
                size: PlainPartitionSize::Fill,
                filesystem: FilesystemKind::ExFat,
                volume_label: "EDPCLI4KN".into(),
            }],
        )
        .unwrap();
        return (plan, None);
    }
    let probe = HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: NativeTransport::Uas,
        windows_pnp_instance_id: None,
        inquiry: Some(InquiryInfo {
            vendor: "Netac".into(),
            product: "OnlyDisk".into(),
            revision: "1.00".into(),
        }),
    };
    let target = TargetIdentity::from_probe(&probe, h.total).unwrap();
    let device_id = target.device_id().to_owned();
    let spec = ProvisionSpec::new(
        target,
        ProvisionMetadata::new(
            OnlyId::parse("1402259934").unwrap(),
            "HIL",
            "TEST",
            "4KN-VIRTUAL",
        )
        .unwrap(),
        ProvisionProfile::canonical_v1(),
    )
    .unwrap();
    // Native candidate LCE: leave 128 native blocks at end, no guessed 512B
    // CHS translator or real-USB LCE claim.
    let lce_lba = h.total - 128;
    let extent = Lba7CompatibilityExtentLayout {
        chs_bytes: lce_lba * u64::from(BLOCK) + 0xe0000,
        start_byte_offset: lce_lba * u64::from(BLOCK),
        start_lba: lce_lba,
        size_bytes: u64::from(BLOCK),
        size_sectors: 1,
    };
    let options = FormatOptions {
        boot: matches!(h.mode, DiskProvisionKind::Mode0 | DiskProvisionKind::Mode3),
        share: matches!(
            h.mode,
            DiskProvisionKind::Mode0 | DiskProvisionKind::Mode1 | DiskProvisionKind::Mode3
        ),
        encrypt: matches!(
            h.mode,
            DiskProvisionKind::Mode0 | DiskProvisionKind::Mode1 | DiskProvisionKind::Mode2
        ),
        ..FormatOptions::default()
    };
    let official_mode = h.mode.official_mode().unwrap();
    let official = OfficialProvisionPlan::new(
        official_mode,
        OfficialPartitionSizes::new(32, 64, 128),
        extent,
        wrap_legacy_lba7_file_key(b"0000aaaa", [0; 8]),
        wrap_file_key(b"0000aaaa", SECRET, FileKeyWrapMode::Sm4),
    )
    .unwrap()
    .with_filesystems(options.filesystems());
    let targets = official.format_targets_native(BLOCK).unwrap();
    let mut formats = Vec::new();
    for part in &targets {
        let formatted = match part.role {
            edpcli::provision::PartitionRole::Boot => options.boot,
            edpcli::provision::PartitionRole::Share
            | edpcli::provision::PartitionRole::BootShareCombined => options.share,
            edpcli::provision::PartitionRole::Encrypt => options.encrypt,
            edpcli::provision::PartitionRole::CompatibilityReserve => false,
        };
        if formatted {
            formats.push((
                part.geometry.start_sector,
                part.geometry.sector_count(),
                part.filesystem.expect("formatted partition filesystem"),
                part.physically_encrypted,
            ));
        }
    }
    let plan = plan_native_edp_image(
        &spec,
        &ProvisionEntropy::new([0x5a; 252]),
        &official,
        &options,
        &vec![0x1234_5678; targets.len()],
        &vec![SECRET; targets.len()],
        BLOCK,
    )
    .unwrap();
    (
        plan,
        Some((
            OfficialProvisionModeInfo {
                mode: official_mode,
                device_id,
                lce_lba,
            },
            formats,
        )),
    )
}
struct OfficialProvisionModeInfo {
    mode: OfficialPartitionMode,
    device_id: String,
    lce_lba: u64,
}

/// Raw full-native-block port available only within this feature-gated,
/// ignored HIL test. Production's physical lease and write gates are unchanged.
fn verify(
    h: &Handoff,
    plan: &NativeVirtualDiskPlan,
    profile: Option<(
        OfficialProvisionModeInfo,
        Vec<(u64, u64, FilesystemKind, bool)>,
    )>,
) {
    let geometry = edpcli::platform::ObservedDeviceGeometry {
        capacity_bytes: h.total * u64::from(BLOCK),
        logical_sector_bytes: Some(BLOCK),
        physical_sector_bytes: None,
    }
    .native_read_geometry()
    .unwrap();
    let mut disk = NativeRawBlockDevice::open_readonly(h.raw.to_str().unwrap(), geometry).unwrap();
    assert_eq!(disk.sector_bytes(), BLOCK);
    // Native sector re-open and full authored-write equality, not 512B-projection.
    for write in &plan.writes {
        assert_eq!(
            disk.read_block_fresh(write.relative_lba).unwrap(),
            write.data,
            "raw LBA {} verification",
            write.relative_lba
        );
    }
    if let Some((info, formats)) = profile {
        let mut native = Vec::with_capacity(13 * BLOCK as usize);
        for lba in 0..13 {
            native.extend(disk.read_block(lba).unwrap());
        }
        let parsed = NativeProtocolImage::from_native_bytes(BLOCK, native).unwrap();
        let profile = parse_existing_provision_native(&parsed, &info.device_id, h.total)
            .unwrap()
            .unwrap();
        assert_eq!(profile.profile.source_mode, info.mode);
        assert_eq!(
            profile.profile.partitions.len(),
            formats.len() + usize::from(info.mode == OfficialPartitionMode::WholeDiskEncrypted)
        );
        let encrypted = disk.read_block_fresh(info.lce_lba).unwrap();
        let decrypted = a6b0_full(&encrypted, &[0; 8], info.lce_lba * u64::from(BLOCK));
        assert_eq!(&decrypted[..3072], edpcli::provision::lce_plaintext());
        assert!(decrypted[3072..].iter().all(|value| *value == 0));
        for (start, count, filesystem, physically_encrypted) in formats {
            let raw = disk.read_block_fresh(start).unwrap();
            // Encrypted native bytes are verified byte-for-byte against the
            // complete authored image. Inspect nonencrypted FS signatures.
            if !physically_encrypted {
                assert_eq!(
                    edpcli::application::filesystem::detect_native_boot_sector(&raw, count, BLOCK)
                        .unwrap(),
                    Some(filesystem),
                    "native filesystem at LBA {start}"
                );
            }
        }
    } else {
        let mbr = disk.read_block_fresh(0).unwrap();
        assert_eq!(&mbr[510..512], &[0x55, 0xaa]);
        assert_eq!(mbr[446 + 4], 0x07);
        assert_eq!(
            u32::from_le_bytes(mbr[446 + 8..446 + 12].try_into().unwrap()),
            2048
        );
    }
}

#[test]
#[ignore = "requires guarded disposable macOS hdiutil -blocksize 4096 raw disk image"]
fn macos_4kn_native_full_block_hil() {
    edpcli::platform::set_include_virtual(true);
    let h = Handoff::from_env();
    let (plan, profile) = build_plan(&h);
    match h.action.as_str() {
        "verify" => {
            verify(&h, &plan, profile);
            println!("HIL READBACK OK: {:?} on {} native blocks", h.mode, h.total);
        }
        "write" => {
            edpcli::application::provision::native_commit::commit_native_plan_on_disk(
                &edpcli::platform::system::SysRunner,
                platform::parse_disk_selector(h.raw.to_str().unwrap()).unwrap(),
                &plan,
                &h.wal,
            )
            .expect("unified native write lease + WAL + readback");
            assert_eq!(
                inspect_native_journal(&h.wal).unwrap().state,
                NativeJournalState::Committed
            );
            verify(&h, &plan, profile);
            println!("HIL WRITE OK: {:?} on {} native blocks", h.mode, h.total);
        }
        _ => panic!("unrecognized HIL stage {}", h.action),
    }
}
