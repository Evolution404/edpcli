//! Shared backup/restore application service.
//!
//! CLI and TUI must enter raw-disk mutation through this module. The safety chain remains single-source:
//! system-disk/USB whole-disk guard → selector pinning by callers →
//! unmount/lock → reopen identity recheck → atomic write → sync/readback/rollback.

#[path = "write/restore_plan.rs"]
mod restore_plan;
use restore_plan::build_metadata_restore_plan;
#[path = "write/backup.rs"]
mod backup;
pub use backup::{backup_create_flow, backup_create_on_disk, backup_create_on_disk_with_pin};
#[path = "write/restore.rs"]
mod restore;
pub use restore::{
    resolve_restore_backup_path, restore_flow, restore_flow_typed, restore_metadata_on_device,
    restore_on_disk, restore_on_disk_typed, restore_on_disk_typed_with_pin,
    restore_on_disk_with_pin, RestoreMetadataRequest,
};

use std::path::PathBuf;

use super::media_identity::{
    match_media_identity, BackupAffinity, BackupAffinityPolicy, MediaIdentityResumePin,
    MediaIdentitySnapshot, RestoreAuthorizationDecision, RestoreAuthorizationPolicy,
    RestoreGeometryRequirements,
};
use super::media_identity_observer::observe_media_identity_readonly;
use super::target_session::{ReadOnly, ReopenAndVerifyError, TargetSession};
use crate::common::*;
use crate::diskio::{self, raw_path, Clock, DiskFacts, SectorDev, SystemClock};
use crate::identify::{generate_candidates, identify};
use crate::selectors::{BackupSelector, DeviceSelector};
use crate::sysinfo::{self, CmdRunner};

use super::device::open_readonly_usb_disk;
pub use super::device::{guard_system_disk, guard_usb_disk};
pub use super::Prompter;

const OPEN_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// UI-neutral typed progress events emitted by backup/restore application flows.
/// Frontends own text, ANSI and TUI presentation; interactive confirmation remains on `Prompter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteEvent {
    BackupCreated {
        path: PathBuf,
    },
    RestoreMatchesHeader {
        disk: u32,
        onlyid: String,
        count: usize,
    },
    RestoreMatchRow {
        index: usize,
        time: String,
        file_name: String,
    },
    RestoreSelectionRetry {
        message: String,
    },
    BackupShaVerified {
        digest: String,
    },
    RestoreDryRunNotice {
        path: PathBuf,
        disk: u32,
    },
    RestoreTargetHeader {
        path: PathBuf,
    },
    RestoreWriteCompleted,
    PostRestoreAssessment {
        assessment: super::post_restore::PostRestoreAssessment,
    },
}

pub type BackupReport = super::post_restore::MetadataBackupReport;

pub struct Ctx<'a> {
    pub runner: &'a dyn CmdRunner,
    pub clock: &'a dyn Clock,
    pub prompt: &'a mut dyn Prompter,
    pub backup_dir: PathBuf,
}

fn err(code: i32, msg: impl Into<String>) -> EdpCliError {
    EdpCliError::new(code, msg)
}

fn backup_identity(
    manifest: &crate::edpb::Manifest,
) -> EdpCliResult<super::media_identity::MediaIdentitySnapshot> {
    crate::edpb::canonical_media_identity(manifest)
        .map_err(|message| err(EXIT_BACKUP, format!("错误: EDPB 身份证据无效: {message}")))
}

fn authorize_restore(
    backup: &MediaIdentitySnapshot,
    target: &MediaIdentitySnapshot,
    backup_tag16: Option<[u8; 16]>,
    target_tag16: &[u8],
    target_lba4_nonzero: bool,
    geometry: RestoreGeometryRequirements,
) -> EdpCliResult<()> {
    let backup_is_plain = matches!(
        backup.protocol.provision_kind,
        Some(crate::provision::DiskProvisionKind::Plain)
    );
    if target_lba4_nonzero
        && !backup_is_plain
        && (target_tag16.iter().all(|byte| *byte == 0)
            || target.protocol.device_id.is_none()
            || target.protocol.provision_kind.is_none())
    {
        return Err(err(
            EXIT_BACKUP,
            "错误: 当前盘 LBA4 非零但 EDP 协议身份损坏，拒绝 fallback Plain 或写入",
        ));
    }
    let identity_match = match_media_identity(backup, target, None);
    match RestoreAuthorizationPolicy::evaluate(backup, target, &identity_match, geometry, None) {
        RestoreAuthorizationDecision::Authorized => {}
        RestoreAuthorizationDecision::Reject(reason) => {
            return Err(err(
                EXIT_BACKUP,
                format!(
                    "错误: restore authorization 拒绝写入: relationship={:?}, conflict={reason:?}",
                    identity_match.relationship
                ),
            ));
        }
    }
    if target_tag16.iter().any(|byte| *byte != 0) && !backup_is_plain {
        if target.protocol.device_id.is_none() || target.protocol.provision_kind.is_none() {
            return Err(err(
                EXIT_BACKUP,
                "错误: 当前盘 LBA4 非零但 EDP 协议身份损坏，拒绝 fallback Plain 或写入",
            ));
        }
        // Plain media has no EDP LBA4 identity tag/onlyid. Restoring a Plain backup
        // to the same physical device after it has been provisioned as EDP is a valid
        // rollback path. The strong physical-media + exact-geometry policy above is
        // the write grant; do not compare a Plain backup's LBA4 bytes as an EDP tag.
        if !backup_is_plain
            && backup_tag16
                .map(|backup_tag16| backup_tag16.as_slice() != target_tag16)
                .unwrap_or(true)
        {
            return Err(err(
                EXIT_BACKUP,
                format!(
                    "错误: EDP 备份身份标签与当前盘不一致(current onlyid={}, backup onlyid={})，拒绝还原",
                    target.protocol.onlyid.as_deref().unwrap_or("未知"),
                    backup.protocol.onlyid.as_deref().unwrap_or("未知")
                ),
            ));
        }
    }
    Ok(())
}

fn backup_protocol_image(
    reader: &crate::edpb::VerifiedBackupReader,
) -> EdpCliResult<Option<Vec<u8>>> {
    let verified = reader.verified();
    let Some(artifact) = verified
        .manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.id == crate::edpb::RAW_PROTOCOL_ARTIFACT_ID)
    else {
        let plain_v3 = verified.manifest.schema == "edpb.manifest.v3"
            && verified
                .manifest
                .snapshot
                .device_state
                .eq_ignore_ascii_case("plain");
        return if plain_v3 {
            Ok(None)
        } else {
            Err(err(EXIT_BACKUP, "错误: EDPB 缺少 LBA0-12 原始 Artifact"))
        };
    };
    if artifact.restore_policy != crate::edpb::RestorePolicy::Restorable {
        return Err(err(
            EXIT_BACKUP,
            "错误: EDPB 的 LBA0-12 Artifact 未标记为可恢复，拒绝写盘",
        ));
    }
    let data = reader.read_raw_protocol().map_err(|message| {
        err(
            EXIT_BACKUP,
            format!("错误: 读取 EDPB LBA0-12 失败: {message}"),
        )
    })?;
    if data.len() != METADATA_IMAGE_LEN {
        return Err(err(
            EXIT_BACKUP,
            format!(
                "错误: EDPB LBA0-12 大小 {} ≠ {}",
                data.len(),
                METADATA_IMAGE_LEN
            ),
        ));
    }
    Ok(Some(data.to_vec()))
}

pub(crate) fn read_image(dev: &mut dyn SectorDev) -> EdpCliResult<Vec<u8>> {
    super::media_identity_observer::read_protocol_image_readonly(dev)
}

/// `reopen_rdwr` 会重新打开平台裸盘设备。确认期间既可能换盘，也可能有别的程序
/// 改动同一块盘的元数据。自动备份保存的是确认前 LBA0-12，因此第一笔写入前必须
/// 再读一次并逐扇区比对，保证“当前状态 == 刚刚备份的状态”。
pub(crate) fn verify_reopened_snapshot(
    dev: &mut dyn SectorDev,
    expected: &[u8],
) -> EdpCliResult<()> {
    if expected.len() != METADATA_IMAGE_LEN {
        return Err(err(EXIT_IO, "错误: 内部预写快照长度异常"));
    }
    // 先核身份，再核其余元数据；换盘时不要被 LBA0 的差异抢先掩盖诊断。
    for lba in
        std::iter::once(4u32).chain((0..METADATA_SECTOR_COUNT as u32).filter(|&lba| lba != 4))
    {
        let actual = dev
            .read_sector(lba)
            .map_err(|e| err(EXIT_IO, format!("错误: 重开后读取 LBA{} 失败: {}", lba, e)))?;
        if actual.len() != SECTOR {
            return Err(err(
                EXIT_IO,
                format!(
                    "错误: 重开后 LBA{} 读取 {}B，预期完整扇区 {}B",
                    lba,
                    actual.len(),
                    SECTOR
                ),
            ));
        }
        let start = lba as usize * SECTOR;
        let before = &expected[start..start + SECTOR];
        if actual != before {
            if lba == 4 {
                let expected_id =
                    diskio::lba4_label_id_from(before).unwrap_or_else(|| "未知".into());
                let actual_id =
                    diskio::lba4_label_id_from(&actual).unwrap_or_else(|| "未知".into());
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: 设备身份在确认/卸载期间发生变化(expected onlyid={}, actual onlyid={})，疑似换盘或重枚举，拒绝写入",
                        expected_id, actual_id
                    ),
                ));
            }
            return Err(err(
                EXIT_TARGET,
                format!(
                    "错误: 设备元数据在备份/确认期间发生变化(LBA{})，拒绝基于过期快照写入",
                    lba
                ),
            ));
        }
    }
    Ok(())
}

/// Revalidate the identity shown by the TUI immediately before starting the
/// selected operation. The write flow still performs its existing fresh
/// snapshot and post-reopen checks; this closes the earlier selection/confirmation
/// window where another USB device could take the same platform disk number.
fn protocol_image_confirms_plain(protocol_image: &[u8], total_sectors: u64) -> bool {
    if protocol_image.len() != METADATA_IMAGE_LEN {
        return false;
    }
    let lba4 = &protocol_image[4 * SECTOR..5 * SECTOR];
    diskio::lba4_label_id_from(lba4).is_none()
        && crate::partition_table::confirmed_plain_protocol_prefix(protocol_image, total_sectors)
}

pub fn verify_expected_identity(
    runner: &dyn CmdRunner,
    disk: u32,
    expected_onlyid: Option<&str>,
    expected_device_id: Option<&str>,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<()> {
    if let Some(expected) = expected_onlyid {
        let raw = dev
            .read_sector(4)
            .map_err(|error| err(EXIT_IO, format!("错误: 身份复核读取 LBA4 失败: {error}")))?;
        if raw.len() != SECTOR {
            return Err(err(
                EXIT_IO,
                format!("错误: 身份复核 LBA4 读取 {}B，预期 {SECTOR}B", raw.len()),
            ));
        }
        let actual = diskio::lba4_label_id_from(&raw);
        if actual.as_deref() != Some(expected) {
            return Err(err(
                EXIT_TARGET,
                format!(
                    "错误: 设备在选择/确认期间发生变化(expected onlyid={expected}, actual onlyid={})，拒绝继续",
                    actual.as_deref().unwrap_or("未知")
                ),
            ));
        }
    }
    if let Some(expected) = expected_device_id {
        let raw = dev
            .read_sector(7)
            .map_err(|error| err(EXIT_IO, format!("错误: 身份复核读取 LBA7 失败: {error}")))?;
        if raw.len() != SECTOR {
            return Err(err(
                EXIT_IO,
                format!("错误: 身份复核 LBA7 读取 {}B，预期 {SECTOR}B", raw.len()),
            ));
        }
        let actual = identify(runner, disk, &raw).device_id;
        if let Some(actual) = actual {
            if actual != expected {
                let candidates = generate_candidates(runner, disk);
                let expected_still_matches_hardware =
                    candidates.iter().any(|candidate| candidate == expected);
                let actual_still_matches_hardware =
                    candidates.iter().any(|candidate| candidate == &actual);
                if !expected_still_matches_hardware || !actual_still_matches_hardware {
                    return Err(err(
                        EXIT_TARGET,
                        format!(
                            "错误: 设备在选择/确认期间发生变化(expected device_id={expected}, actual device_id={actual})，拒绝继续"
                        ),
                    ));
                }
            }
        } else {
            // Plain whole-disk FAT/exFAT/NTFS media may legitimately carry
            // filesystem boot code in LBA4/LBA7. Nonzero bytes alone are not
            // EDP evidence. Preserve fail-closed behavior by requiring the
            // same positive Plain proof used by the canonical media observer
            // whenever LBA4's identity-tag bytes are nonzero.
            let lba4 = dev.read_sector(4).map_err(|error| {
                err(
                    EXIT_IO,
                    format!("错误: Plain 身份复核读取 LBA4 失败: {error}"),
                )
            })?;
            if lba4.len() != SECTOR {
                return Err(err(
                    EXIT_IO,
                    format!(
                        "错误: Plain 身份复核 LBA4 读取 {}B，预期 {SECTOR}B",
                        lba4.len()
                    ),
                ));
            }
            let tag = diskio::lba4_tag16_from(&lba4)
                .ok_or_else(|| err(EXIT_IO, "错误: Plain 身份复核无法读取 LBA4 身份标签"))?;
            if diskio::lba4_label_id_from(&lba4).is_some() {
                return Err(err(
                    EXIT_TARGET,
                    "错误: LBA7 无法识别但 LBA4 仍含有效 EDP onlyid；疑似损坏 EDP，拒绝按 Plain 硬件身份继续",
                ));
            }
            if tag.iter().any(|&byte| byte != 0) {
                let image = read_image(dev)?;
                let total_sectors = sysinfo::disk_total_sectors(runner, disk).ok_or_else(|| {
                    err(
                        EXIT_TARGET,
                        "错误: Plain 身份复核无法取得物理总扇区数，拒绝按硬件身份继续",
                    )
                })?;
                if !protocol_image_confirms_plain(&image, total_sectors) {
                    return Err(err(
                        EXIT_TARGET,
                        "错误: LBA7 无法识别且 LBA4 非零，同时缺少可验证 Plain 正向证据；疑似损坏 EDP，拒绝继续",
                    ));
                }
            }
            if !generate_candidates(runner, disk)
                .iter()
                .any(|candidate| candidate == expected)
            {
                return Err(err(
                    EXIT_TARGET,
                    format!(
                        "错误: 设备在选择/确认期间发生变化(expected Plain device_id={expected}, 当前硬件候选不匹配)，拒绝继续"
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn verify_resume_identity_pin(
    runner: &dyn CmdRunner,
    disk: u32,
    expected: &MediaIdentityResumePin,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<()> {
    let fresh = super::media_identity_observer::observe_media_identity_readonly(runner, disk, dev)?;
    expected
        .verify(&fresh.snapshot, &fresh.protocol_image)
        .map_err(|conflict| {
            err(
                EXIT_TARGET,
                format!("错误: TUI resume 目标介质身份 pin 不一致: {conflict:?}"),
            )
        })
}

pub(crate) fn auto_pick_disk(
    runner: &dyn CmdRunner,
    prompt: &mut dyn Prompter,
) -> EdpCliResult<u32> {
    DeviceSelector::new(None).resolve(runner, prompt)
}

/// restore 主流程: bin=None 时交互列出本盘备份并选择。
#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct ShortSectorDev;

    impl SectorDev for ShortSectorDev {
        fn read_sector(&mut self, _lba: u32) -> io::Result<Vec<u8>> {
            Ok(vec![0u8; SECTOR - 1])
        }

        fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn read_image_rejects_short_sector_without_panicking() {
        let error = read_image(&mut ShortSectorDev).unwrap_err();
        assert_eq!(error.code, EXIT_IO);
        assert!(error.msg.contains("512B"), "{}", error.msg);
    }

    fn ntfs_plain_protocol_image(total_sectors: u64) -> Vec<u8> {
        let mut image = vec![0u8; METADATA_IMAGE_LEN];
        let boot = &mut image[..SECTOR];
        boot[..3].copy_from_slice(&[0xeb, 0x52, 0x90]);
        boot[3..11].copy_from_slice(b"NTFS    ");
        boot[11..13].copy_from_slice(&(SECTOR as u16).to_le_bytes());
        boot[13] = 8;
        boot[21] = 0xf8;
        boot[40..48].copy_from_slice(&(total_sectors - 1).to_le_bytes());
        boot[48..56].copy_from_slice(&4u64.to_le_bytes());
        boot[56..64].copy_from_slice(&8u64.to_le_bytes());
        boot[510..512].copy_from_slice(&[0x55, 0xaa]);
        image
    }

    #[test]
    fn plain_confirmation_allows_nonzero_lba4_filesystem_code() {
        let total = 30_277_632u64;
        let mut image = ntfs_plain_protocol_image(total);
        image[4 * SECTOR..4 * SECTOR + 16].copy_from_slice(&[
            0x66, 0x61, 0x90, 0x1f, 0x07, 0xc3, 0x06, 0x1e, 0x66, 0x60, 0x66, 0xb8, 1, 0, 0, 0,
        ]);
        assert!(protocol_image_confirms_plain(&image, total));
    }

    #[test]
    fn plain_confirmation_rejects_edp_onlyid_even_with_valid_filesystem_boot() {
        let total = 30_277_632u64;
        let mut image = ntfs_plain_protocol_image(total);
        image[4 * SECTOR..4 * SECTOR + 9].copy_from_slice(b"$$$123$$$");
        assert!(!protocol_image_confirms_plain(&image, total));
    }

    fn restore_test_hardware(
        serial: &str,
    ) -> super::super::media_identity::HardwareIdentityEvidence {
        let raw_serial = serial.to_string();
        let serial = super::super::media_identity::serial_digest_evidence(Some(serial));
        super::super::media_identity::HardwareIdentityEvidence {
            vid: Some(0x2bdf),
            pid: Some(0x0300),
            serial: Some(raw_serial),
            serial_sha256: serial.sha256,
            serial_quality: serial.quality,
            vendor: Some("HIKSEMI".into()),
            product: Some("HIKSEMI".into()),
            revision: Some("1.00".into()),
            transport: Some(crate::platform::NativeTransport::Uas),
            total_sectors: Some(245_760_000),
            logical_sector_size: Some(SECTOR as u32),
        }
    }

    fn restore_test_plain(serial: &str) -> MediaIdentitySnapshot {
        MediaIdentitySnapshot::plain(
            restore_test_hardware(serial),
            super::super::media_identity::DerivedProtocolEvidence::default(),
            super::super::media_identity::IdentityObservation::default(),
        )
    }

    fn restore_test_edp(serial: &str, onlyid: &str) -> MediaIdentitySnapshot {
        MediaIdentitySnapshot {
            hardware: restore_test_hardware(serial),
            protocol: super::super::media_identity::ProtocolIdentityEvidence {
                device_id: Some("disk&ven_hiksemi&prod_".into()),
                onlyid: Some(onlyid.into()),
                provision_kind: Some(crate::provision::DiskProvisionKind::Mode0),
                lba4_identity_digest: Some(format!("digest-{onlyid}")),
            },
            derived: super::super::media_identity::DerivedProtocolEvidence::default(),
            observation: super::super::media_identity::IdentityObservation::default(),
        }
    }

    #[test]
    fn plain_backup_can_restore_same_physical_device_after_edp_provisioning() {
        let backup = restore_test_plain("HIKSEMI-SERIAL-001");
        let target = restore_test_edp("HIKSEMI-SERIAL-001", "914806819");
        let geometry = RestoreGeometryRequirements {
            total_sectors: 245_760_000,
            logical_sector_size: SECTOR as u32,
        };
        let backup_tag = [0u8; 16];
        let target_tag = *b"$$$914806819$$$";
        authorize_restore(
            &backup,
            &target,
            Some(backup_tag),
            &target_tag,
            true,
            geometry,
        )
        .expect("same physical media Plain rollback must be authorized");
    }

    #[test]
    fn edp_backup_tag_mismatch_remains_rejected() {
        let backup = restore_test_edp("HIKSEMI-SERIAL-001", "111111111");
        let target = restore_test_edp("HIKSEMI-SERIAL-001", "914806819");
        let geometry = RestoreGeometryRequirements {
            total_sectors: 245_760_000,
            logical_sector_size: SECTOR as u32,
        };
        let error = authorize_restore(
            &backup,
            &target,
            Some(*b"$$$111111111$$$\0"),
            b"$$$914806819$$$\0",
            true,
            geometry,
        )
        .unwrap_err();
        assert!(error.msg.contains("身份标签与当前盘不一致"));
    }

    struct PatternSectorDev;

    impl SectorDev for PatternSectorDev {
        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            Ok(vec![lba as u8; SECTOR])
        }

        fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn read_image_preserves_lba_zero_to_twelve_order() {
        let image = read_image(&mut PatternSectorDev).unwrap();
        assert_eq!(image.len(), METADATA_IMAGE_LEN);
        for lba in 0..METADATA_SECTOR_COUNT {
            assert!(
                image[lba * SECTOR..(lba + 1) * SECTOR]
                    .iter()
                    .all(|&byte| byte == lba as u8),
                "LBA{} 在拼接镜像中的位置错误",
                lba
            );
        }
    }
}
