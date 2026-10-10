use super::*;
use crate::application::provision::PlannedPartitionFormat;
use crate::filesystem::FilesystemKind;
use crate::provision::{
    OfficialPartitionFilesystems, OfficialPartitionMode as Mode, OfficialPartitionSizes,
    PartitionRole as Role, RegionDisposition as Disposition, TargetPartitionGeometry,
    TargetPartitionPlan, TargetProvisionPlan,
};

#[test]
fn native_result_uses_actual_partition_disposition_not_forced_rebuild() {
    for disposition in [
        Disposition::PreserveVerified,
        Disposition::PreserveOpaque,
        Disposition::RewrapVerified,
        Disposition::Rebuild,
    ] {
        let part = crate::application::provision::native_flow::NativePreviewPartition {
            role: Some(Role::Encrypt),
            start_lba: 100,
            sector_count: 200,
            filesystem: Some(FilesystemKind::ExFat),
            formatted: disposition == Disposition::Rebuild,
            physically_encrypted: true,
            disposition: Some(disposition),
            password_disposition: None,
        };
        for logical_bytes in [512, 1024, 2048, 4096] {
            let projected = ProvisionResultPartition::from_native(&part, logical_bytes);
            assert_eq!(projected.disposition, Some(disposition));
            assert_eq!(projected.size_bytes, 200 * u64::from(logical_bytes));
            assert_eq!(projected.selected_for_format, part.formatted);
        }
    }
}

#[test]
fn result_partitions_preserve_evidence_instead_of_format_defaults() {
    for mode in [Mode::DefaultThreePartition, Mode::WholeDiskEncrypted] {
        let targets = crate::provision::official_format_targets_with_filesystems(
            mode,
            OfficialPartitionSizes::new(32, 64, 128),
            512,
            OfficialPartitionFilesystems::defaults(),
        )
        .unwrap();
        let mut target_plan = TargetProvisionPlan {
            mode,
            unallocated_sectors: 0,
            partitions: targets
                .iter()
                .map(|target| TargetPartitionPlan {
                    geometry: TargetPartitionGeometry {
                        role: target.role,
                        partition_type: target.geometry.partition_type,
                        start_lba: target.geometry.start_sector,
                        sector_count: target.geometry.sector_count(),
                        physically_encrypted: target.physically_encrypted,
                        filesystem: if target.role == Role::Boot {
                            Some(FilesystemKind::Ntfs)
                        } else {
                            None
                        },
                    },
                    disposition: match target.role {
                        Role::CompatibilityReserve => Disposition::Rebuild,
                        Role::Boot => Disposition::PreserveVerified,
                        _ => Disposition::PreserveOpaque,
                    },
                    password_disposition: None,
                    source_password_knowledge: None,
                    target_password_policy: None,
                    reason: String::new(),
                    preserved_record: None,
                })
                .collect(),
        };
        let mut choices = targets
            .into_iter()
            .map(|target| PlannedPartitionFormat {
                filesystem: target.filesystem,
                target,
                selected: false,
                volume_label: String::new(),
                volume_serial: 1,
                prepared_image: None,
                verification_image: None,
            })
            .collect::<Vec<_>>();
        for choice in &choices {
            let partition = ProvisionResultPartition::from_format(choice, Some(&target_plan));
            assert_eq!(
                partition.filesystem,
                if partition.role == Some(Role::Boot) {
                    Some(FilesystemKind::Ntfs)
                } else {
                    None
                }
            );
            assert_eq!(
                ProvisionResultPartition::from_format(choice, None).filesystem,
                None
            );
        }
        for choice in &mut choices {
            if choice.target.format_capable {
                choice.selected = true;
                choice.filesystem = Some(FilesystemKind::Fat32);
            }
            let partition = ProvisionResultPartition::from_format(choice, Some(&target_plan));
            assert_eq!(
                partition.filesystem,
                if partition.role == Some(Role::CompatibilityReserve) {
                    None
                } else {
                    Some(FilesystemKind::Fat32)
                }
            );
        }
        choices[0].selected = false;
        target_plan.partitions[0].geometry.start_lba += 1;
        let partition = ProvisionResultPartition::from_format(&choices[0], Some(&target_plan));
        assert_eq!(partition.filesystem, None);
        assert_eq!(partition.disposition, None);
    }
}
