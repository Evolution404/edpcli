//! Source identity and disposition projection regressions (never touch a disk).
use super::*;
use crate::provision::{PartitionRole as Role, RegionDisposition};

const MODES: [DiskProvisionKind; 5] = [
    DiskProvisionKind::Plain,
    DiskProvisionKind::Mode0,
    DiskProvisionKind::Mode1,
    DiskProvisionKind::Mode2,
    DiskProvisionKind::Mode3,
];
fn roles(mode: DiskProvisionKind) -> &'static [Role] {
    match mode {
        DiskProvisionKind::Plain => &[],
        DiskProvisionKind::Mode0 => &[Role::Boot, Role::Share, Role::Encrypt],
        DiskProvisionKind::Mode1 => &[Role::BootShareCombined, Role::Encrypt],
        DiskProvisionKind::Mode2 => &[Role::Encrypt],
        DiskProvisionKind::Mode3 => &[Role::Boot, Role::Share],
    }
}
fn geometry(role: Role) -> (u64, u64, bool) {
    match role {
        Role::Boot => (63, 20_417, false),
        Role::Share => (20_480, 280_000, true),
        Role::BootShareCombined => (63, 300_417, false),
        Role::Encrypt => (300_480, 200_000, true),
        Role::CompatibilityReserve => unreachable!(),
    }
}
fn source(mode: DiskProvisionKind, sector: u32) -> Vec<SourcePartition> {
    if mode == DiskProvisionKind::Plain {
        return vec![SourcePartition {
            id: SourcePartitionId::PlainMbr { slot: 0 },
            label: "普通分区P1".into(),
            role: None,
            start_lba: 2048,
            sector_count: 400_000,
            sector_bytes: sector,
            filesystem: None,
            physically_encrypted: false,
        }];
    }
    roles(mode)
        .iter()
        .enumerate()
        .map(|(slot, role)| {
            let (start, count, encrypted) = geometry(*role);
            SourcePartition {
                id: SourcePartitionId::Edp { slot, role: *role },
                label: role.label().into(),
                role: Some(*role),
                start_lba: start,
                sector_count: count,
                sector_bytes: sector,
                filesystem: None,
                physically_encrypted: encrypted,
            }
        })
        .collect()
}

#[test]
fn complete_25_source_target_matrix_across_four_native_sectors() {
    for sector in [512, 1024, 2048, 4096] {
        let plan = NativeVirtualDiskPlan {
            total_sectors: 1_000_000,
            sector_bytes: sector,
            writes: Vec::new(),
        };
        let mut cases = 0;
        for from in MODES {
            for to in MODES {
                let sources = source(from, sector);
                let targets: Vec<NativePreviewPartition> = if to == DiskProvisionKind::Plain {
                    vec![NativePreviewPartition {
                        role: None,
                        start_lba: 2048,
                        sector_count: 400_000,
                        filesystem: None,
                        formatted: true,
                        physically_encrypted: false,
                        disposition: None,
                        password_disposition: None,
                    }]
                } else {
                    roles(to)
                        .iter()
                        .map(|role| {
                            let (start, count, encrypted) = geometry(*role);
                            let preserve = sources.iter().any(|s| {
                                s.role == Some(*role)
                                    && s.start_lba == start
                                    && s.sector_count == count
                                    && s.physically_encrypted == encrypted
                            });
                            NativePreviewPartition {
                                role: Some(*role),
                                start_lba: start,
                                sector_count: count,
                                filesystem: None,
                                formatted: !preserve,
                                physically_encrypted: encrypted,
                                disposition: Some(if preserve {
                                    RegionDisposition::PreserveVerified
                                } else {
                                    RegionDisposition::Rebuild
                                }),
                                password_disposition: None,
                            }
                        })
                        .collect()
                };
                let impact = project_native_impact(sources.clone(), &targets, &plan).unwrap();
                assert_eq!(
                    impact.sources.len(),
                    sources.len(),
                    "{sector} {from:?}->{to:?}"
                );
                assert_eq!(
                    impact.source_discarded.len() + impact.source_retained.len(),
                    sources.len(),
                    "{sector} {from:?}->{to:?}"
                );
                assert_eq!(
                    impact.target_formatted.len(),
                    targets.iter().filter(|p| p.formatted).count()
                );
                for mapping in &impact.sources {
                    assert_eq!(
                        mapping.preserved_target.is_some(),
                        impact.source_retained.contains(&mapping.source.label)
                    );
                }
                if from == DiskProvisionKind::Mode1 && to == DiskProvisionKind::Mode0 {
                    assert_eq!(impact.source_discarded, ["二合一区"]);
                    assert_eq!(impact.source_retained, ["保密区"]);
                    assert_eq!(impact.target_formatted, ["启动区", "交换区"]);
                    assert_eq!(impact.key_operations, ["交换区（新 FileKey）"]);
                }
                if from == DiskProvisionKind::Mode2 && to == DiskProvisionKind::Mode2 {
                    assert_eq!(impact.source_retained, ["保密区"]);
                    assert!(impact.source_discarded.is_empty());
                    assert!(!impact.source_retained.iter().any(|s| s.contains("兼容")));
                }
                cases += 1;
            }
        }
        assert_eq!(cases, 25, "{sector}");
    }
}

#[test]
fn retained_source_rejects_any_write_overlap_and_wrong_semantics() {
    let src = source(DiskProvisionKind::Mode2, 4096);
    let secret = &src[0];
    let mut target = NativePreviewPartition {
        role: Some(Role::Encrypt),
        start_lba: secret.start_lba,
        sector_count: secret.sector_count,
        filesystem: None,
        formatted: false,
        physically_encrypted: true,
        disposition: Some(RegionDisposition::RewrapVerified),
        password_disposition: Some(crate::provision::PasswordDisposition::Rewrap),
    };
    let mut plan = NativeVirtualDiskPlan {
        total_sectors: 1_000_000,
        sector_bytes: 4096,
        writes: vec![],
    };
    let impact = project_native_impact(src.clone(), &[target.clone()], &plan).unwrap();
    assert_eq!(impact.source_retained, ["保密区"]);
    assert_eq!(impact.key_operations, ["保密区（仅改密）"]);
    assert!(impact.target_formatted.is_empty());
    plan.writes.push(crate::filesystem::NativeFilesystemWrite {
        relative_lba: secret.start_lba,
        data: vec![0; 4096],
    });
    assert!(project_native_impact(src.clone(), &[target.clone()], &plan).is_err());
    plan.writes.clear();
    target.physically_encrypted = false;
    assert!(project_native_impact(src.clone(), &[target.clone()], &plan).is_err());
    target.physically_encrypted = true;
    target.disposition = Some(RegionDisposition::Rebuild);
    assert!(project_native_impact(src, &[target], &plan).is_err());
}
#[test]
fn plain_mbr_multislot_identity_and_unknown_sources_fail_closed() {
    for sector in [512, 4096] {
        let mut prefix = vec![vec![0u8; sector as usize]; 13];
        let mbr = &mut prefix[0];
        mbr[510] = 0x55;
        mbr[511] = 0xaa;
        for (slot, start, count) in [(0usize, 2048u32, 1000u32), (2, 100_000, 50_000)] {
            let offset = 446 + 16 * slot;
            mbr[offset + 4] = 0x07;
            mbr[offset + 8..offset + 12].copy_from_slice(&start.to_le_bytes());
            mbr[offset + 12..offset + 16].copy_from_slice(&count.to_le_bytes());
        }
        let sources =
            capture_source_partitions(DiskProvisionKind::Plain, None, &prefix, sector, 1_000_000)
                .unwrap();
        assert_eq!(
            sources.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(),
            ["普通分区P1", "普通分区P3"]
        );
        assert_eq!(sources[1].id, SourcePartitionId::PlainMbr { slot: 2 });
        let plan = NativeVirtualDiskPlan {
            sector_bytes: sector,
            total_sectors: 1_000_000,
            writes: vec![],
        };
        let impact = project_native_impact(sources, &[], &plan).unwrap();
        assert_eq!(impact.source_discarded, ["普通分区P1", "普通分区P3"]);
        prefix[0][510] = 0;
        assert!(capture_source_partitions(
            DiskProvisionKind::Plain,
            None,
            &prefix,
            sector,
            1_000_000
        )
        .is_err());
        prefix[0].fill(0);
        prefix[0][0] = 0x99;
        prefix[0][510] = 0x55;
        prefix[0][511] = 0xaa;
        assert!(capture_source_partitions(
            DiskProvisionKind::Plain,
            None,
            &prefix,
            sector,
            1_000_000
        )
        .is_err());
        prefix[0].fill(0);
        assert!(capture_source_partitions(
            DiskProvisionKind::Plain,
            None,
            &prefix,
            sector,
            1_000_000
        )
        .unwrap()
        .is_empty());
    }
}
#[test]
fn start_or_capacity_changes_rebuild_and_cannot_claim_preservation() {
    for sector in [512, 1024, 2048, 4096] {
        let source = source(DiskProvisionKind::Mode1, sector);
        let secret = source
            .iter()
            .find(|p| p.role == Some(Role::Encrypt))
            .unwrap();
        let (start, count, _) = geometry(Role::Encrypt);
        let plan = NativeVirtualDiskPlan {
            sector_bytes: sector,
            total_sectors: 1_000_000,
            writes: vec![],
        };
        for (new_start, new_count) in [
            (start + 1, count),
            (start, count + 1),
            (start - 1, count),
            (start, count - 1),
        ] {
            let target = NativePreviewPartition {
                role: Some(Role::Encrypt),
                start_lba: new_start,
                sector_count: new_count,
                filesystem: None,
                physically_encrypted: true,
                formatted: true,
                disposition: Some(RegionDisposition::Rebuild),
                password_disposition: None,
            };
            let projected =
                project_native_impact(vec![secret.clone()], std::slice::from_ref(&target), &plan)
                    .unwrap();
            assert_eq!(
                projected.source_discarded,
                ["保密区"],
                "{sector} {new_start} {new_count}"
            );
            assert_eq!(projected.target_formatted, ["保密区"]);
            let nonformatted = NativePreviewPartition {
                formatted: false,
                disposition: Some(RegionDisposition::PreserveVerified),
                ..target
            };
            assert!(project_native_impact(vec![secret.clone()], &[nonformatted], &plan).is_err());
        }
    }
}
