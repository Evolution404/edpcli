use crate::application::backup_coverage::{BackupCoverage, BackupCoverageRegion};
use crate::application::media_identity::{
    DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityObservation, MediaIdentityPin,
    MediaIdentitySnapshot, ProtocolIdentityEvidence, SerialQuality,
};
use crate::application::partition_table::{
    PartitionSource, PartitionTableExtent, PartitionTableKind, PartitionTableSnapshot,
    PhysicalPartition,
};
use crate::application::EdpfPartition;
use crate::application::{BackupIntegrityStatus, BackupWorkspaceItem};
use crate::backup_metadata::{Lba7CompatibilityGeometry, PartitionGeometry};
use crate::common::{METADATA_IMAGE_LEN, SECTOR};
use crate::disk_scan::Row;
use crate::edpb::ArtifactCompleteness;
use crate::filesystem::FilesystemKind;
use crate::provision::{
    DiskProvisionKind, OfficialPartitionMode, PartitionRole, ProvisionTarget, RegionDisposition,
};
use crate::tui::state::{
    ProvisionConfirmationAction, ProvisionConfirmationDataEffect,
    ProvisionConfirmationFilesystemEffect, ProvisionConfirmationOverall,
    ProvisionConfirmationPasswordEffect, ProvisionConfirmationRegion, ProvisionConfirmationTarget,
    ProvisionConfirmationViewModel, ProvisionResultPartition, ProvisionResultSnapshot,
};

const TOTAL_SECTORS: u64 = 125_000_000;

fn pin(row: &Row) -> MediaIdentityPin {
    let serial = crate::application::media_identity::serial_digest_evidence(row.serial.as_deref());
    let snapshot = MediaIdentitySnapshot {
        hardware: HardwareIdentityEvidence {
            vid: u16::from_str_radix(&row.vid, 16).ok(),
            pid: u16::from_str_radix(&row.pid, 16).ok(),
            serial: row.serial.clone(),
            serial_sha256: serial.sha256,
            serial_quality: serial.quality,
            vendor: Some("DEMO".into()),
            product: Some(format!("Scene{}", row.disk)),
            revision: Some("1.0".into()),
            total_sectors: Some(TOTAL_SECTORS),
            logical_sector_size: Some(SECTOR as u32),
            ..HardwareIdentityEvidence::default()
        },
        protocol: ProtocolIdentityEvidence {
            device_id: row.device_id.clone(),
            onlyid: row.onlyid.clone(),
            provision_kind: Some(row.provision_kind),
            lba4_identity_digest: None,
        },
        derived: DerivedProtocolEvidence::default(),
        observation: IdentityObservation::default(),
    };
    MediaIdentityPin::new(snapshot, &vec![0; METADATA_IMAGE_LEN])
}

pub(super) fn disk(disk: u32, kind: DiskProvisionKind) -> Row {
    let edp = kind != DiskProvisionKind::Plain;
    let partitions = match kind {
        DiskProvisionKind::Mode0 => Some(vec![
            EdpfPartition {
                ptype: 1,
                active: 1,
                enc: 0,
                start_lba: 63,
                size_bytes: 20_417 * SECTOR as u64,
            },
            EdpfPartition {
                ptype: 2,
                active: 1,
                enc: 1,
                start_lba: 20_480,
                size_bytes: 4_000_000 * SECTOR as u64,
            },
            EdpfPartition {
                ptype: 4,
                active: 1,
                enc: 1,
                start_lba: 4_020_480,
                size_bytes: 2_097_153 * SECTOR as u64,
            },
        ]),
        DiskProvisionKind::Mode1 => Some(vec![
            EdpfPartition {
                ptype: 2,
                active: 1,
                enc: 1,
                start_lba: 63,
                size_bytes: 4_000_000 * SECTOR as u64,
            },
            EdpfPartition {
                ptype: 4,
                active: 1,
                enc: 1,
                start_lba: 4_000_063,
                size_bytes: 2_097_153 * SECTOR as u64,
            },
        ]),
        _ => None,
    };
    let partition_table = (!edp).then_some(PartitionTableSnapshot {
        kind: PartitionTableKind::Mbr,
        partitions: vec![PhysicalPartition {
            index: 1,
            start_lba: 2_048,
            sector_count: 80_000_000,
            source: PartitionSource::Mbr {
                partition_type: 0x07,
                primary_slot: Some(1),
            },
            filesystem: Some("exFAT".into()),
            volume_label: Some("演示普通盘".into()),
        }],
        table_extents: vec![PartitionTableExtent {
            label: "MBR".into(),
            start_lba: 0,
            sector_count: 1,
        }],
        issues: Vec::new(),
    });
    let lce = edp.then_some(Lba7CompatibilityGeometry {
        start_lba: TOTAL_SECTORS - 2_000,
        sector_count: 6,
        lba7_pointer_entries: Vec::new(),
        official_partition_mode: None,
        chs_expected_start_lba: None,
    });
    let mut row = Row {
        disk,
        size: TOTAL_SECTORS * SECTOR as u64,
        vid: "1234".into(),
        pid: format!("{disk:04x}"),
        proto: "USB".into(),
        serial: Some(format!("DEMO-SERIAL-{disk}")),
        hardware_model: None,
        device_id: edp.then(|| format!("demo&ven_edp&prod_scene{disk}")),
        identity_pin: None,
        onlyid: edp.then(|| format!("14022599{disk:02}")),
        dept: Some("DEMO 输电运检中心与 Unicode 演示部门🙂".into()),
        user: Some("DEMO 张三".into()),
        label: Some("DEMO-SAFE6".into()),
        force_change_password: Some(false),
        cancel_password_complexity_check: Some(false),
        max_share_password_errors: Some(5),
        max_encrypt_password_errors: Some(5),
        n_baks: 2,
        n_possible_baks: 1,
        denied: false,
        probe_error: None,
        provision_kind: kind,
        partitions,
        partition_table,
        partition_table_error: None,
        lce,
    };
    row.identity_pin = Some(pin(&row));
    row
}

pub(super) fn provision_review_projection(row: &Row) -> ProvisionConfirmationViewModel {
    use crate::disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind};
    use crate::tui::disk_layout::DiskCapacitySelection;

    let total_sectors = row.size / SECTOR as u64;
    let lce = row
        .lce
        .as_ref()
        .expect("demo EDP disk must have LCE geometry");
    let partitions = row
        .partitions
        .as_ref()
        .expect("demo EDP disk must have official partitions")
        .iter()
        .enumerate()
        .filter_map(|(index, partition)| {
            let role = match (row.provision_kind, index, partition.ptype) {
                (DiskProvisionKind::Mode1, 0, 2) => PartitionRole::BootShareCombined,
                (DiskProvisionKind::Mode2, 0, 1) => PartitionRole::CompatibilityReserve,
                (_, _, 1) => PartitionRole::Boot,
                (_, _, 2) => PartitionRole::Share,
                (_, _, 4) => PartitionRole::Encrypt,
                _ => return None,
            };
            Some(DiskLayoutSegment {
                label: role.label().into(),
                start_lba: partition.start_lba,
                sector_count: partition.size_bytes / SECTOR as u64,
                kind: DiskRegionKind::from_partition_role(role),
            })
        })
        .collect::<Vec<_>>();
    let layout =
        DiskLayoutModel::canonical_edp(total_sectors, partitions, lce.start_lba, lce.sector_count)
            .expect("demo EDP layout must be complete");
    let visible = layout.collapsed_tail_model();
    let regions = visible
        .segments
        .iter()
        .filter_map(|segment| {
            let selection = DiskCapacitySelection::from_segment(segment)?;
            let role = match segment.kind {
                DiskRegionKind::Boot => Some(PartitionRole::Boot),
                DiskRegionKind::Share => Some(PartitionRole::Share),
                DiskRegionKind::Combined => Some(PartitionRole::BootShareCombined),
                DiskRegionKind::Encrypt => Some(PartitionRole::Encrypt),
                DiskRegionKind::Compatibility => Some(PartitionRole::CompatibilityReserve),
                _ => None,
            };
            let action = match segment.kind {
                DiskRegionKind::Protocol | DiskRegionKind::Tail => {
                    ProvisionConfirmationAction::Fixed
                }
                DiskRegionKind::Reserved => ProvisionConfirmationAction::Preserve,
                DiskRegionKind::Free => ProvisionConfirmationAction::Free,
                DiskRegionKind::Share | DiskRegionKind::Combined | DiskRegionKind::Encrypt => {
                    ProvisionConfirmationAction::Passthrough
                }
                DiskRegionKind::Compatibility => ProvisionConfirmationAction::New,
                DiskRegionKind::Boot => ProvisionConfirmationAction::Preserve,
                _ => ProvisionConfirmationAction::Fixed,
            };
            let partition_data = matches!(
                segment.kind,
                DiskRegionKind::Boot
                    | DiskRegionKind::Share
                    | DiskRegionKind::Combined
                    | DiskRegionKind::Encrypt
            );
            let encrypted_partition = matches!(
                segment.kind,
                DiskRegionKind::Share | DiskRegionKind::Combined | DiskRegionKind::Encrypt
            );
            let filesystem_partition = partition_data;
            Some(ProvisionConfirmationRegion {
                label: segment.label.clone(),
                role,
                selection,
                sector_count: segment.sector_count,
                action,
                data_effect: if partition_data {
                    ProvisionConfirmationDataEffect::Preserve
                } else {
                    ProvisionConfirmationDataEffect::None
                },
                password_effect: if encrypted_partition {
                    ProvisionConfirmationPasswordEffect::Preserve
                } else {
                    ProvisionConfirmationPasswordEffect::None
                },
                filesystem_effect: if filesystem_partition {
                    ProvisionConfirmationFilesystemEffect::Keep
                } else {
                    ProvisionConfirmationFilesystemEffect::None
                },
                reason_summary: match segment.kind {
                    DiskRegionKind::Free => "未分配空间保持空闲。".into(),
                    DiskRegionKind::Protocol => "EDP 主协议区按目标计划更新。".into(),
                    DiskRegionKind::Reserved => "协议保留区保持原位置。".into(),
                    DiskRegionKind::Tail => "盘尾协议区域保持固定。".into(),
                    _ => "现有区域满足演示中的无损保留条件。".into(),
                },
                technical_basis: vec![format!(
                    "演示投影 · LBA {}..{}",
                    segment.start_lba,
                    segment
                        .start_lba
                        .saturating_add(segment.sector_count)
                        .saturating_sub(1)
                )],
            })
        })
        .collect();

    ProvisionConfirmationViewModel {
        target: ProvisionConfirmationTarget {
            disk: row.disk,
            total_sectors,
            device_id: row.device_id.clone().unwrap_or_else(|| "DEMO".into()),
            vid: u16::from_str_radix(&row.vid, 16).ok(),
            pid: u16::from_str_radix(&row.pid, 16).ok(),
            onlyid: row.onlyid.clone(),
            target: ProvisionTarget::Official(OfficialPartitionMode::DefaultThreePartition),
        },
        layout,
        overall: ProvisionConfirmationOverall {
            cleared_regions: 0,
            reformatted_regions: 0,
            password_changed_regions: 0,
        },
        algorithm: Some(crate::provision::OfficialLabelAlgorithm::Sms4),
        geometry_note: None,
        regions,
    }
}

pub(super) fn provision_result_snapshot(row: &Row) -> ProvisionResultSnapshot {
    let target = match row.provision_kind {
        DiskProvisionKind::Mode0 => {
            ProvisionTarget::Official(OfficialPartitionMode::DefaultThreePartition)
        }
        DiskProvisionKind::Mode1 => {
            ProvisionTarget::Official(OfficialPartitionMode::BootShareCombined)
        }
        DiskProvisionKind::Mode2 => {
            ProvisionTarget::Official(OfficialPartitionMode::WholeDiskEncrypted)
        }
        DiskProvisionKind::Mode3 => {
            ProvisionTarget::Official(OfficialPartitionMode::IntranetExtranetDualPartition)
        }
        DiskProvisionKind::Plain => ProvisionTarget::Plain,
    };

    let partitions = if row.provision_kind == DiskProvisionKind::Plain {
        row.partition_table
            .as_ref()
            .map(|table| {
                table
                    .partitions
                    .iter()
                    .map(|partition| ProvisionResultPartition {
                        role: None,
                        filesystem: Some(FilesystemKind::ExFat),
                        start_lba: partition.start_lba,
                        size_bytes: partition.sector_count.saturating_mul(SECTOR as u64),
                        selected_for_format: true,
                        disposition: None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        row.partitions
            .as_ref()
            .map(|parts| {
                parts
                    .iter()
                    .enumerate()
                    .filter_map(|(index, partition)| {
                        let role = match (row.provision_kind, index, partition.ptype) {
                            (DiskProvisionKind::Mode1, 0, 2) => PartitionRole::BootShareCombined,
                            (DiskProvisionKind::Mode2, 0, 1) => PartitionRole::CompatibilityReserve,
                            (_, _, 1) => PartitionRole::Boot,
                            (_, _, 2) => PartitionRole::Share,
                            (_, _, 4) => PartitionRole::Encrypt,
                            _ => return None,
                        };
                        let filesystem = match role {
                            PartitionRole::CompatibilityReserve => None,
                            PartitionRole::Boot => Some(FilesystemKind::Fat16),
                            PartitionRole::Share
                            | PartitionRole::Encrypt
                            | PartitionRole::BootShareCombined => Some(FilesystemKind::ExFat),
                        };
                        Some(ProvisionResultPartition {
                            role: Some(role),
                            filesystem,
                            start_lba: partition.start_lba,
                            size_bytes: partition.size_bytes,
                            selected_for_format: role != PartitionRole::CompatibilityReserve,
                            disposition: Some(RegionDisposition::Rebuild),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    ProvisionResultSnapshot {
        disk: row.disk,
        target,
        total_bytes: row.size,
        partitions,
    }
}

fn coverage() -> BackupCoverage {
    BackupCoverage {
        regions: vec![
            BackupCoverageRegion {
                id: "protocol".into(),
                role: "EDP 主协议区".into(),
                total_sectors: Some(13),
                captured_sectors: 13,
                artifact_count: 1,
                completeness: ArtifactCompleteness::Complete,
            },
            BackupCoverageRegion {
                id: "tail-mirror".into(),
                role: "历史备份镜像".into(),
                total_sectors: Some(9),
                captured_sectors: 9,
                artifact_count: 1,
                completeness: ArtifactCompleteness::Complete,
            },
        ],
        extent_count: 2,
        artifact_count: 2,
    }
}

pub(super) fn backup(
    index: usize,
    name: &str,
    healthy: bool,
    source: &Row,
    physical_confirmed: bool,
) -> BackupWorkspaceItem {
    let mut identity = source
        .identity_pin
        .as_ref()
        .expect("demo device identity pin")
        .snapshot
        .clone();
    if !physical_confirmed {
        identity.hardware.serial = None;
        identity.hardware.serial_sha256 = None;
        identity.hardware.serial_quality = SerialQuality::Missing;
    }
    BackupWorkspaceItem {
        display_cached: false,
        index,
        path: std::path::PathBuf::from(format!("DEMO/{name}")),
        file_name: format!("DEMO-{name}"),
        display_time: "2026-09-27 12:00".into(),
        size_bytes: Some(TOTAL_SECTORS * SECTOR as u64),
        vid: Some(source.vid.clone()),
        pid: Some(source.pid.clone()),
        device_id: source.device_id.clone(),
        onlyid: source.onlyid.clone(),
        identity: Some(identity),
        user: Some("DEMO 张三".into()),
        dept: Some("DEMO 输电运检中心".into()),
        provision_kind: Some(DiskProvisionKind::Mode0),
        integrity_status: if healthy {
            BackupIntegrityStatus::Verified
        } else {
            BackupIntegrityStatus::Invalid
        },
        size_ok: healthy,
        verification_error: None,
        content_sha256: Some("0123456789abcdef".repeat(4)),
        coverage: Some(coverage()),
        restore_preview: None,
    }
}

pub(super) fn inspect_workspace(
    row: &Row,
) -> crate::application::inspect::AdvancedInspectWorkspace {
    use crate::application::inspect::{
        AbsoluteByteRange, AdvancedInspectItem, AdvancedInspectMode, AdvancedInspectWorkspace,
        InspectField, InspectFieldStatus, InspectFieldType,
    };
    use crate::inspect::{InspectFieldKey, InspectParseState};
    use crate::inspect_adapter::{FieldChild, FieldStyle};

    let mut context = crate::inspect_target::InspectDiskContext::new_with_partition_table(
        vec![0; METADATA_IMAGE_LEN],
        row.device_id.clone(),
        TOTAL_SECTORS,
        Some(row.provision_kind),
        row.partition_table.clone(),
        None,
    );
    if let Some(parts) = &row.partitions {
        context.partitions = parts
            .iter()
            .enumerate()
            .map(|(index, part)| PartitionGeometry {
                index,
                partition_type: part.ptype,
                partition_count: parts.len() as u32,
                need_disturb: 0,
                need_encrypt: part.enc,
                start_sector: part.start_lba,
                sector_size: SECTOR as u64,
                partition_size: part.size_bytes,
                sector_count: part.size_bytes / SECTOR as u64,
                user_key_crc: 0,
                file_key_crc: 0,
                encrypt_mode: if part.enc == 0 { 0 } else { 2 },
            })
            .collect();
    }
    context.lce = row.lce.clone();
    let disk_layout =
        crate::application::disk_layout::DiskLayoutModel::canonical_inspect_context(&context).ok();
    let fields = vec![
        InspectField {
            key: InspectFieldKey::Synthetic,
            range: AbsoluteByteRange {
                start: 8 * SECTOR as u64,
                end_exclusive: 8 * SECTOR as u64 + 32,
            },
            field_type: InspectFieldType::Identity,
            raw: vec![0x44; 32],
            decoded: vec![0x44; 32],
            field_logical: None,
            transform: None,
            status: InspectFieldStatus::Known,
            label: "DEMO 部门 / 用户".into(),
            value: "输电运检中心 · 张三🙂".into(),
            style: FieldStyle::Identity,
            group: Some("LBA8".into()),
            children: Vec::new(),
        },
        InspectField {
            key: InspectFieldKey::Lba8Elabel,
            range: AbsoluteByteRange {
                start: 8 * SECTOR as u64 + 32,
                end_exclusive: 8 * SECTOR as u64 + 64,
            },
            field_type: InspectFieldType::Identity,
            raw: vec![0x45; 32],
            decoded: vec![0x45; 32],
            field_logical: None,
            transform: None,
            status: InspectFieldStatus::Known,
            label: "E_LABEL".into(),
            value: "DEMO E_LABEL / 17 项".into(),
            style: FieldStyle::Identity,
            group: Some("LBA8".into()),
            children: (1..=17)
                .map(|index| FieldChild {
                    label: format!("DEMO 字段 {index}"),
                    value: format!("值 {index}"),
                    relative_range: None,
                })
                .collect(),
        },
        InspectField {
            key: InspectFieldKey::Synthetic,
            range: AbsoluteByteRange {
                start: 8 * SECTOR as u64 + 64,
                end_exclusive: 8 * SECTOR as u64 + 68,
            },
            field_type: InspectFieldType::Identity,
            raw: vec![0xff; 4],
            decoded: vec![0xff; 4],
            field_logical: None,
            transform: None,
            status: InspectFieldStatus::Unknown,
            label: "DEMO warning / error 字段".into(),
            value: "证据不足：模拟异常值".into(),
            style: FieldStyle::Identity,
            group: Some("LBA8".into()),
            children: Vec::new(),
        },
    ];
    let item = AdvancedInspectItem {
        lba: 8,
        regions: vec!["DEMO LBA8".into()],
        raw: vec![0x44; SECTOR],
        raw_sha256: "DEMO-RAW-SHA256".into(),
        raw_nonzero: SECTOR,
        decoded: Some(vec![0x45; SECTOR]),
        decode_ranges: vec![crate::inspect::DecodeRange::new(0, SECTOR)],
        decoded_sha256: Some("DEMO-DECODE-SHA256".into()),
        method: Some("DEMO typed fixture".into()),
        decode_error: None,
        parse_state: InspectParseState::Parsed,
        diagnostics: Vec::new(),
        fields,
        notes: vec!["DEMO 数据，不访问真实介质".into()],
        meta_text: Some("DEMO LBA8 部门、用户与 E_LABEL".into()),
    };
    AdvancedInspectWorkspace {
        source: format!("DEMO disk{}", row.disk),
        meta: crate::inspect::InspectMeta::default(),
        mode: AdvancedInspectMode::Meta,
        items: vec![item],
        export_dir: None,
        topology: crate::application::inspect_tree::build_inspect_topology(&context),
        disk_layout,
        disk_layout_issue: None,
        backup_manifest: None,
    }
}

/// Result fixtures use the same typed outcome as production, including partial failure.
pub(super) fn hydrate_result(state: &mut crate::tui::state::AppState, scene: &str) {
    use crate::application::provision::{
        PartitionFormatResult, ProvisionCommitOutcome, ProvisionCommitReport,
        ProvisionExecutionStatus, ProvisionWarning, ProvisionWriteOutcome,
    };
    use crate::tui::ui::UiMessage;
    if scene.ends_with("failure") {
        state.provision_mut().result_status = Some(ProvisionExecutionStatus::FatalFailure);
        state.provision_mut().result_outcome = None;
        state.provision_mut().message = Some(UiMessage::error(if scene.contains("rollback") {
            "DEMO：协议读回不一致；回滚读回失败，介质状态未确认。停止操作并检查设备。"
        } else {
            "DEMO：协议读回不一致；回滚已完成。重新检查设备后再生成计划。"
        }));
        return;
    }
    let plan = state
        .provision()
        .result_plan
        .as_ref()
        .expect("demo result plan");
    let partial = scene.ends_with("partial");
    let formats = plan
        .partitions
        .iter()
        .filter_map(|partition| partition.role)
        .enumerate()
        .map(|(index, role)| PartitionFormatResult {
            role,
            result: if partial && index == 1 {
                Err("DEMO：文件系统读回不一致；该分区不可声明可用".into())
            } else {
                Ok(())
            },
        })
        .collect();
    let outcome = ProvisionWriteOutcome {
        backup: crate::application::post_restore::MetadataBackupReport {
            path: "DEMO-before-provision.edpb".into(),
            partition_count: plan.partitions.len(),
            edp_protocol_saved: true,
        },
        commit: ProvisionCommitOutcome::Official(ProvisionCommitReport {
            provision_succeeded: true,
            formats,
        }),
        warnings: if partial {
            vec![ProvisionWarning::IncompleteFormat]
        } else if scene.ends_with("warning") {
            vec![ProvisionWarning::HostLineagePersistenceFailed(
                "DEMO：主机历史保存失败".into(),
            )]
        } else {
            vec![]
        },
    };
    state.provision_mut().result_status = Some(outcome.execution_status());
    state.provision_mut().result_outcome = Some(outcome);
}
