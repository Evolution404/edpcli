#![cfg(all(
    feature = "ci-virtual-disk",
    any(target_os = "linux", target_os = "windows")
))]

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use edpcli::backup_metadata::PartitionGeometry;
use edpcli::common::{METADATA_SECTOR_COUNT, SECTOR};
use edpcli::diskio::{
    atomic_write_sectors, execute_write_transaction, FileDev, SectorDev, SectorWriteStage,
    WriteTransactionPlan,
};
use edpcli::filesystem::analysis::{
    analyze_partition, stream_file_payload, AnalysisStatus, PartitionReader,
};
use edpcli::filesystem::FilesystemKind;
use edpcli::platform::{HardwareProbe, InquiryInfo, NativeTransport};
use edpcli::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity;
use edpcli::provision::{
    build_migrated_filesystem, generate_official_image, parse_existing_provision,
    prefill_for_target_mode, unwrap_legacy_lba7_file_key, wrap_file_key, wrap_legacy_lba7_file_key,
    FileKeyWrapMode, KeyDomainRole, KeyDomainSecretPair, KeyDomainSecrets, MigrationStagedEntry,
    MigrationTransform, OfficialPartitionMode, OfficialPartitionSizes, OfficialProvisionPlan,
    OnlyId, PartitionAction, PartitionRole, ProvisionEntropy, ProvisionImage, ProvisionMetadata,
    ProvisionProfile, ProvisionSpec, RegionDisposition, SourcePasswordKnowledge, TargetIdentity,
    TargetProvisionPlan,
};

static HIL_LOCK: Mutex<()> = Mutex::new(());

const SHARE_PASSWORD: &[u8] = b"SharePass1!";
const ENCRYPT_PASSWORD: &[u8] = b"EncryptPass1!";
const NEW_ENCRYPT_PASSWORD: &[u8] = b"EncryptPass2!";

fn read_metadata(dev: &mut dyn SectorDev) -> BTreeMap<u32, Vec<u8>> {
    (0u32..METADATA_SECTOR_COUNT as u32)
        .map(|lba| {
            let data = dev
                .read_sector(lba)
                .unwrap_or_else(|error| panic!("read LBA{lba}: {error}"));
            (lba, data)
        })
        .collect()
}

fn deterministic_patch() -> BTreeMap<u32, Vec<u8>> {
    (0u32..METADATA_SECTOR_COUNT as u32)
        .map(|lba| {
            let mut data = vec![0u8; SECTOR];
            for (index, byte) in data.iter_mut().enumerate() {
                *byte = ((lba as usize * 37 + index * 13 + 0x5a) % 251) as u8;
            }
            data[..12].copy_from_slice(b"EDPCLI-VHIL\0");
            data[12..16].copy_from_slice(&lba.to_le_bytes());
            (lba, data)
        })
        .collect()
}

fn image_patch(image: &ProvisionImage) -> BTreeMap<u32, Vec<u8>> {
    (0u32..METADATA_SECTOR_COUNT as u32)
        .map(|lba| {
            let start = lba as usize * SECTOR;
            (lba, image.as_bytes()[start..start + SECTOR].to_vec())
        })
        .collect()
}

fn read_provision_image(dev: &mut dyn SectorDev) -> ProvisionImage {
    let mut bytes = Vec::with_capacity(METADATA_SECTOR_COUNT * SECTOR);
    for lba in 0u32..METADATA_SECTOR_COUNT as u32 {
        bytes.extend_from_slice(
            &dev.read_sector(lba)
                .unwrap_or_else(|error| panic!("read LBA{lba}: {error}")),
        );
    }
    ProvisionImage::from_bytes(bytes).expect("virtual disk metadata image")
}

fn hil_mode0_fixture(total_sectors: u64) -> (ProvisionSpec, ProvisionImage, String) {
    let probe = HardwareProbe {
        vid: Some(0x0dd8),
        pid: Some(0x2005),
        transport: NativeTransport::Uas,
        windows_pnp_instance_id: None,
        inquiry: Some(InquiryInfo {
            vendor: "EDPCLI".into(),
            product: "Chapter12VHIL".into(),
            revision: "1.00".into(),
        }),
    };
    let target =
        TargetIdentity::from_probe(&probe, total_sectors).expect("virtual target identity");
    let device_id = target.device_id().to_string();
    let metadata = ProvisionMetadata::new(
        OnlyId::parse("1402259934").unwrap(),
        "VHIL01",
        "江苏省电力有限公司",
        "江苏电力!SAFE6",
    )
    .unwrap();
    let spec = ProvisionSpec::new(target, metadata, ProvisionProfile::canonical_v1()).unwrap();
    let compatibility =
        locate_lba7_compatibility_extent_from_verified_usb_capacity(total_sectors, SECTOR as u32)
            .expect("128MiB HIL disk must have verified 255x63 compatibility geometry");
    let sizes = OfficialPartitionSizes::new(8, 16, 32);
    let plan = OfficialProvisionPlan::new(
        OfficialPartitionMode::DefaultThreePartition,
        sizes,
        compatibility,
        wrap_legacy_lba7_file_key(b"0000aaaa", [0x11; 8]),
        wrap_file_key(b"0000aaaa", [0x21; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap()
    .with_partition_key_material(
        1,
        wrap_legacy_lba7_file_key(SHARE_PASSWORD, [0x31; 8]),
        wrap_file_key(SHARE_PASSWORD, [0x41; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap()
    .with_partition_key_material(
        2,
        wrap_legacy_lba7_file_key(ENCRYPT_PASSWORD, [0x51; 8]),
        wrap_file_key(ENCRYPT_PASSWORD, [0x61; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap();
    let image = generate_official_image(&spec, &ProvisionEntropy::new([0x5a; 252]), &plan).unwrap();
    (spec, image, device_id)
}

fn domain_secrets(
    share_source: Option<&[u8]>,
    share_target: Option<&[u8]>,
    encrypt_source: Option<&[u8]>,
    encrypt_target: Option<&[u8]>,
) -> KeyDomainSecrets {
    KeyDomainSecrets::new(
        KeyDomainSecretPair::new(share_source, share_target),
        KeyDomainSecretPair::new(encrypt_source, encrypt_target),
    )
}

fn part<'a>(
    plan: &'a TargetProvisionPlan,
    role: PartitionRole,
) -> &'a edpcli::provision::TargetPartitionPlan {
    plan.partitions
        .iter()
        .find(|part| part.geometry.role == role)
        .unwrap_or_else(|| panic!("missing {role:?} target"))
}

struct HilPartitionReader<'a> {
    dev: &'a mut dyn SectorDev,
    start_lba: u64,
    sector_count: u64,
}

impl PartitionReader for HilPartitionReader<'_> {
    fn read_sector(&mut self, relative_lba: u64) -> std::io::Result<Vec<u8>> {
        if relative_lba >= self.sector_count {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "K6 HIL relative LBA exceeds partition",
            ));
        }
        let absolute = self
            .start_lba
            .checked_add(relative_lba)
            .and_then(|lba| u32::try_from(lba).ok())
            .ok_or_else(|| std::io::Error::other("K6 HIL absolute LBA overflow"))?;
        self.dev.read_sector(absolute)
    }
}

#[test]
#[ignore = "requires a disposable OS virtual disk created by the HIL workflow"]
fn raw_virtual_disk_atomic_roundtrip_and_restore() {
    let _serial = HIL_LOCK.lock().expect("serialize destructive virtual HIL");
    let path = std::env::var("EDPCLI_VIRTUAL_DISK_PATH")
        .expect("EDPCLI_VIRTUAL_DISK_PATH must point to the disposable loop/VHD");
    assert!(
        edpcli::platform::is_raw_device_path(&path),
        "HIL path must be a raw device: {path}"
    );

    // 先走产品平台层的卸载/锁卷逻辑。ci_prepare_virtual_write 内部还会二次确认：
    // Linux 必须是 /dev/loopN；Windows 必须是 Virtual/FileBackedVirtual VHD。
    let _guard = edpcli::platform::ci_prepare_virtual_write(&path)
        .unwrap_or_else(|error| panic!("prepare virtual write {path}: {error}"));

    let mut dev = FileDev::open_rdwr(&path, Duration::from_secs(5))
        .unwrap_or_else(|error| panic!("open virtual raw device {path}: {error}"));
    let original = read_metadata(&mut dev);
    let patch = deterministic_patch();
    assert_ne!(
        original, patch,
        "virtual disk unexpectedly matches HIL pattern"
    );

    atomic_write_sectors(&mut dev, &patch).expect("atomic write HIL pattern");
    assert_eq!(
        read_metadata(&mut dev),
        patch,
        "HIL write read-back mismatch"
    );

    atomic_write_sectors(&mut dev, &original).expect("restore original virtual disk metadata");
    assert_eq!(
        read_metadata(&mut dev),
        original,
        "virtual disk was not restored bit-for-bit"
    );

    // Chapter 12 semantic HIL: install a real mode0 metadata image on the disposable
    // loop/VHD, read it back through FileDev, then exercise the per-domain planner
    // against bytes that actually crossed the raw-device boundary.
    let total_sectors: u64 = std::env::var("EDPCLI_VIRTUAL_DISK_SECTORS")
        .expect("EDPCLI_VIRTUAL_DISK_SECTORS must describe the disposable virtual disk")
        .parse()
        .expect("virtual-disk sector count must be an integer");
    let compatibility =
        locate_lba7_compatibility_extent_from_verified_usb_capacity(total_sectors, SECTOR as u32)
            .expect("virtual disk must expose verified 255x63 geometry");
    let (spec, source_image, device_id) = hil_mode0_fixture(total_sectors);
    atomic_write_sectors(&mut dev, &image_patch(&source_image))
        .expect("install Chapter 12 mode0 source metadata");

    let installed = read_provision_image(&mut dev);
    let mut source = parse_existing_provision(&installed, &device_id, total_sectors)
        .expect("parse virtual mode0 source")
        .expect("virtual source must be registered EDP");
    assert_eq!(
        source.source_password_knowledge(KeyDomainRole::Share, Some(SHARE_PASSWORD)),
        SourcePasswordKnowledge::UserVerified
    );
    assert_eq!(
        source.source_password_knowledge(KeyDomainRole::Encrypt, Some(ENCRYPT_PASSWORD)),
        SourcePasswordKnowledge::UserVerified
    );
    assert_eq!(
        source.source_password_knowledge(KeyDomainRole::Encrypt, Some(SHARE_PASSWORD)),
        SourcePasswordKnowledge::Unknown,
        "Share password must never verify the independent Encrypt domain"
    );

    source
        .confirm_filesystem(PartitionRole::Boot, FilesystemKind::Fat16)
        .unwrap();
    source
        .confirm_filesystem(PartitionRole::Share, FilesystemKind::ExFat)
        .unwrap();
    source
        .confirm_filesystem(PartitionRole::Encrypt, FilesystemKind::ExFat)
        .unwrap();

    let exact_prefill = prefill_for_target_mode(
        Some(&source.profile),
        OfficialPartitionMode::DefaultThreePartition,
        compatibility.start_lba,
        SECTOR as u64,
    )
    .unwrap();
    let exact_targets = exact_prefill.target_partitions(SECTOR as u64).unwrap();

    // Exact preserve with two different passwords.
    let exact = TargetProvisionPlan::build(
        Some(&source),
        OfficialPartitionMode::DefaultThreePartition,
        &exact_targets,
        compatibility.start_lba,
        &domain_secrets(
            Some(SHARE_PASSWORD),
            Some(SHARE_PASSWORD),
            Some(ENCRYPT_PASSWORD),
            Some(ENCRYPT_PASSWORD),
        ),
    )
    .unwrap();
    assert!(exact
        .partitions
        .iter()
        .all(|part| part.action == PartitionAction::PreserveExact));
    assert_eq!(
        part(&exact, PartitionRole::Share).disposition,
        RegionDisposition::PreserveVerified
    );
    assert_eq!(
        part(&exact, PartitionRole::Encrypt).disposition,
        RegionDisposition::PreserveVerified
    );

    // Unknown-password exact geometry must remain opaque-preservable and must
    // retain the original key record rather than generating a new FileKey.
    let opaque = TargetProvisionPlan::build(
        Some(&source),
        OfficialPartitionMode::DefaultThreePartition,
        &exact_targets,
        compatibility.start_lba,
        &domain_secrets(Some(SHARE_PASSWORD), Some(SHARE_PASSWORD), None, None),
    )
    .unwrap();
    let opaque_encrypt = part(&opaque, PartitionRole::Encrypt);
    assert_eq!(
        opaque_encrypt.disposition,
        RegionDisposition::PreserveOpaque
    );
    assert_eq!(
        opaque_encrypt.preserved_record,
        source.record(PartitionRole::Encrypt).copied()
    );

    // Password-only change is RewrapVerified. Then actually write the rewrapped
    // protocol image to the loop/VHD and prove the data extent stayed bit-for-bit
    // unchanged while the new password replaces the old password.
    let rewrap = TargetProvisionPlan::build(
        Some(&source),
        OfficialPartitionMode::DefaultThreePartition,
        &exact_targets,
        compatibility.start_lba,
        &domain_secrets(
            Some(SHARE_PASSWORD),
            Some(SHARE_PASSWORD),
            Some(ENCRYPT_PASSWORD),
            Some(NEW_ENCRYPT_PASSWORD),
        ),
    )
    .unwrap();
    assert_eq!(
        part(&rewrap, PartitionRole::Encrypt).disposition,
        RegionDisposition::RewrapVerified
    );

    let share_record = *source.record(PartitionRole::Share).unwrap();
    let encrypt_record = *source.record(PartitionRole::Encrypt).unwrap();
    let raw_legacy =
        unwrap_legacy_lba7_file_key(ENCRYPT_PASSWORD, encrypt_record.lba7_key_material()).unwrap();
    let raw_file_key = encrypt_record
        .verified_sm4_file_key(ENCRYPT_PASSWORD)
        .unwrap();
    let encrypt_start = part(&rewrap, PartitionRole::Encrypt).geometry.start_lba as u32;
    let data_before = dev
        .read_sector(encrypt_start)
        .expect("read preserved Encrypt data sector before rewrap");

    let rewrap_plan = OfficialProvisionPlan::new(
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionSizes::new(8, 16, 32),
        compatibility,
        wrap_legacy_lba7_file_key(b"0000aaaa", [0x11; 8]),
        wrap_file_key(b"0000aaaa", [0x21; 16], FileKeyWrapMode::Sm4),
    )
    .unwrap()
    .with_target_geometry(&exact_targets, SECTOR as u64)
    .unwrap()
    .with_partition_key_material(
        1,
        share_record.lba7_key_material(),
        share_record.lba12_key_material().unwrap(),
    )
    .unwrap()
    .with_partition_key_material(
        2,
        wrap_legacy_lba7_file_key(NEW_ENCRYPT_PASSWORD, raw_legacy),
        wrap_file_key(NEW_ENCRYPT_PASSWORD, raw_file_key, FileKeyWrapMode::Sm4),
    )
    .unwrap();
    let rewrapped_image =
        generate_official_image(&spec, &ProvisionEntropy::new([0x5a; 252]), &rewrap_plan).unwrap();
    atomic_write_sectors(&mut dev, &image_patch(&rewrapped_image))
        .expect("write rewrapped Chapter 12 metadata");
    assert_eq!(
        dev.read_sector(encrypt_start).unwrap(),
        data_before,
        "password-only rewrap must not write the Encrypt data extent"
    );
    let reparsed =
        parse_existing_provision(&read_provision_image(&mut dev), &device_id, total_sectors)
            .unwrap()
            .unwrap();
    assert_eq!(
        reparsed.source_password_knowledge(KeyDomainRole::Encrypt, Some(NEW_ENCRYPT_PASSWORD)),
        SourcePasswordKnowledge::UserVerified
    );
    assert_eq!(
        reparsed.source_password_knowledge(KeyDomainRole::Encrypt, Some(ENCRYPT_PASSWORD)),
        SourcePasswordKnowledge::Unknown,
        "old password must not unwrap the rewrapped key record"
    );
    assert_eq!(
        reparsed
            .record(PartitionRole::Encrypt)
            .unwrap()
            .verified_sm4_file_key(NEW_ENCRYPT_PASSWORD)
            .unwrap(),
        raw_file_key,
        "rewrap must preserve K_old"
    );

    // Geometry change removes Preserve eligibility and forces rebuild.
    let mut changed_targets = exact_targets.clone();
    let changed_encrypt = changed_targets
        .iter_mut()
        .find(|part| part.role == PartitionRole::Encrypt)
        .unwrap();
    changed_encrypt.sector_count -= 1;
    let forced = TargetProvisionPlan::build(
        Some(&source),
        OfficialPartitionMode::DefaultThreePartition,
        &changed_targets,
        compatibility.start_lba,
        &domain_secrets(
            Some(SHARE_PASSWORD),
            Some(SHARE_PASSWORD),
            Some(ENCRYPT_PASSWORD),
            Some(ENCRYPT_PASSWORD),
        ),
    )
    .unwrap();
    assert_eq!(
        part(&forced, PartitionRole::Encrypt).disposition,
        RegionDisposition::Rebuild
    );

    // mode1 Combined is physically plaintext even though it owns the Share key
    // domain. This guards against NeedEncrypt/key-record metadata accidentally
    // turning the data extent into ciphertext.
    let mode1 = prefill_for_target_mode(
        Some(&source.profile),
        OfficialPartitionMode::BootShareCombined,
        compatibility.start_lba,
        SECTOR as u64,
    )
    .unwrap()
    .target_partitions(SECTOR as u64)
    .unwrap();
    let combined = mode1
        .iter()
        .find(|part| part.role == PartitionRole::BootShareCombined)
        .unwrap();
    assert!(!combined.physically_encrypted);
    assert_eq!(combined.filesystem, Some(FilesystemKind::ExFat));
    assert_eq!(
        KeyDomainRole::from_partition_role(combined.role),
        Some(KeyDomainRole::Share)
    );

    // K6 migration HIL: build a populated first-party filesystem, write the exact
    // sparse write-set to the real loop/VHD, parse it back through raw-device I/O,
    // and verify the migrated payload byte-for-byte before restoring every touched
    // sector. Application K6 uses the same atomic write engine for this write-set.
    let share_target = exact_targets
        .iter()
        .find(|part| part.role == PartitionRole::Share)
        .expect("mode0 Share target for K6 HIL");
    let payload = b"k6-virtual-hil-payload-".repeat(64);
    let staged = vec![MigrationStagedEntry {
        source_index: 0,
        transform: MigrationTransform::ShareToBootShareCombined,
        path: "/k6-vhil.bin".into(),
        is_directory: false,
        data: payload.clone(),
        attributes: 0x20,
        mtime: None,
        ctime: None,
    }];
    let migrated = build_migrated_filesystem(
        FilesystemKind::ExFat,
        share_target.start_lba,
        share_target.sector_count,
        0x4b36_4849,
        "K6VHIL",
        &staged,
    )
    .expect("build populated K6 exFAT image");
    let mut k6_patch = BTreeMap::new();
    let mut k6_before = BTreeMap::new();
    for (&relative_lba, sector) in migrated.sectors() {
        let absolute = share_target
            .start_lba
            .checked_add(relative_lba)
            .and_then(|lba| u32::try_from(lba).ok())
            .expect("K6 HIL target LBA");
        k6_before.insert(
            absolute,
            dev.read_sector(absolute)
                .unwrap_or_else(|error| panic!("snapshot K6 HIL LBA{absolute}: {error}")),
        );
        k6_patch.insert(absolute, sector.to_vec());
    }
    let mut k6_transaction = WriteTransactionPlan::new(total_sectors);
    for (&lba, data) in &k6_patch {
        k6_transaction
            .insert(
                lba,
                data.clone(),
                SectorWriteStage::Data,
                "K6 migration HIL",
            )
            .expect("plan K6 data write");
    }
    execute_write_transaction(&mut dev, &k6_transaction)
        .expect("write K6 populated filesystem to virtual disk");
    for (&lba, expected) in &k6_patch {
        assert_eq!(
            dev.read_sector(lba).unwrap(),
            *expected,
            "K6 HIL raw readback mismatch at LBA{lba}"
        );
    }
    let geometry = PartitionGeometry {
        index: 1,
        partition_type: 2,
        partition_count: 3,
        need_disturb: 0,
        need_encrypt: 0,
        start_sector: share_target.start_lba,
        sector_size: SECTOR as u64,
        partition_size: share_target.sector_count * SECTOR as u64,
        sector_count: share_target.sector_count,
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: 0,
    };
    let entry = {
        let mut reader = HilPartitionReader {
            dev: &mut dev,
            start_lba: share_target.start_lba,
            sector_count: share_target.sector_count,
        };
        let report = analyze_partition(&geometry, &mut reader);
        assert_eq!(report.status, AnalysisStatus::Parsed, "{}", report.reason);
        report
            .entries
            .unwrap()
            .into_iter()
            .find(|entry| entry.path == "/k6-vhil.bin")
            .expect("K6 HIL migrated file")
    };
    let mut readback = Vec::new();
    let mut reader = HilPartitionReader {
        dev: &mut dev,
        start_lba: share_target.start_lba,
        sector_count: share_target.sector_count,
    };
    stream_file_payload(&mut reader, &entry, payload.len() as u64, &mut readback)
        .expect("K6 HIL stream migrated payload");
    assert_eq!(readback, payload, "K6 HIL logical payload mismatch");
    let mut k6_restore = WriteTransactionPlan::new(total_sectors);
    for (&lba, data) in &k6_before {
        k6_restore
            .insert(
                lba,
                data.clone(),
                SectorWriteStage::Data,
                "K6 migration HIL restore",
            )
            .expect("plan K6 restore write");
    }
    execute_write_transaction(&mut dev, &k6_restore).expect("restore K6 HIL touched sectors");
    for (&lba, expected) in &k6_before {
        assert_eq!(
            dev.read_sector(lba).unwrap(),
            *expected,
            "K6 HIL sector restore mismatch at LBA{lba}"
        );
    }

    // mode2 keeps the canonical 0x7E00-byte / 63-sector compatibility reserve:
    // no user filesystem and no physical encryption.
    let mode2 = prefill_for_target_mode(
        Some(&source.profile),
        OfficialPartitionMode::WholeDiskEncrypted,
        compatibility.start_lba,
        SECTOR as u64,
    )
    .unwrap()
    .target_partitions(SECTOR as u64)
    .unwrap();
    let reserve = mode2
        .iter()
        .find(|part| part.role == PartitionRole::CompatibilityReserve)
        .unwrap();
    assert_eq!(reserve.sector_count, 63);
    assert!(!reserve.physically_encrypted);
    assert_eq!(reserve.filesystem, None);

    atomic_write_sectors(&mut dev, &original)
        .expect("restore original virtual disk metadata after Chapter 12 HIL");
    assert_eq!(
        read_metadata(&mut dev),
        original,
        "Chapter 12 HIL must restore LBA0-12 bit-for-bit"
    );
}
