use edpcli::application::inspect::{
    decode_sector, AdvancedInspectMode, AdvancedInspectWorkspace, InspectErrorKind,
};
use edpcli::application::inspect_tree::{build_inspect_topology, InspectNodeKind};
use edpcli::common::{METADATA_IMAGE_LEN, SECTOR};
use edpcli::inspect::InspectMeta;
use edpcli::inspect_target::InspectDiskContext;
use edpcli::platform::{HardwareProbe, InquiryInfo, NativeTransport};
use edpcli::protocol::lba7_compat::locate_lba7_compatibility_extent_from_geometry;
use edpcli::provision::{
    generate_official_image, wrap_file_key, wrap_legacy_lba7_file_key, FileKeyWrapMode,
    OfficialPartitionMode, OfficialPartitionSizes, OfficialProvisionPlan, OnlyId, ProvisionEntropy,
    ProvisionMetadata, ProvisionProfile, ProvisionSpec, TargetIdentity,
};
use edpcli::tui::state::{AdvancedInspectSource, AppState};

fn official_spec() -> ProvisionSpec {
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

fn official_plan(mode: OfficialPartitionMode) -> OfficialProvisionPlan {
    let compat = locate_lba7_compatibility_extent_from_geometry(1024, 255, 63, SECTOR as u32)
        .expect("verified compatibility geometry");
    OfficialProvisionPlan::new(
        mode,
        OfficialPartitionSizes::new(32, 64, 128),
        compat,
        wrap_legacy_lba7_file_key(
            b"0000aaaa",
            [0x7d, 0x9e, 0xe4, 0xe8, 0x75, 0x4a, 0xd4, 0x38],
        ),
        wrap_file_key(
            b"ProofPass1!",
            [
                0x14, 0x71, 0x96, 0xf5, 0xa2, 0xec, 0x79, 0x12, 0xed, 0xf1, 0x3f, 0x75, 0xd7, 0x66,
                0xcb, 0x42,
            ],
            FileKeyWrapMode::Sm4,
        ),
    )
    .unwrap()
}

fn workspace(context: &InspectDiskContext) -> AdvancedInspectWorkspace {
    AdvancedInspectWorkspace {
        source: "I9 synthetic disk".into(),
        meta: InspectMeta::default(),
        mode: AdvancedInspectMode::Meta,
        items: Vec::new(),
        export_dir: None,
        topology: build_inspect_topology(context),
    }
}

#[test]
fn all_four_official_modes_keep_lce_and_logical_partitions_in_one_full_disk_topology() {
    let spec = official_spec();
    let entropy = ProvisionEntropy::new([0x5a; 252]);

    for mode in [
        OfficialPartitionMode::DefaultThreePartition,
        OfficialPartitionMode::BootShareCombined,
        OfficialPartitionMode::WholeDiskEncrypted,
        OfficialPartitionMode::IntranetExtranetDualPartition,
    ] {
        let plan = official_plan(mode);
        let logical = plan.logical_partitions(SECTOR as u64).unwrap();
        let image = generate_official_image(&spec, &entropy, &plan).unwrap();
        let context = InspectDiskContext::new(
            image.as_bytes().to_vec(),
            Some(spec.target().device_id().to_string()),
            spec.target().total_sectors(),
        );

        assert_eq!(context.partitions.len(), logical.len(), "{mode:?}");
        let lce = context.lce.as_ref().expect("official mode must expose LCE");
        let topology = build_inspect_topology(&context);

        for partition in &context.partitions {
            assert!(
                topology
                    .regions_for_lba(partition.start_sector)
                    .iter()
                    .any(|node| node.id == format!("region.partition.{}", partition.index)),
                "{mode:?} missing logical partition {}",
                partition.index
            );
        }
        assert!(
            topology
                .regions_for_lba(lce.start_lba)
                .iter()
                .any(|node| node.id == "region.lce"),
            "{mode:?} missing LCE"
        );
        assert!(
            topology
                .regions_for_lba(logical[0].start_sector)
                .iter()
                .all(|node| !node.id.starts_with("region.mbr_partition.")),
            "{mode:?} duplicated the MBR exposure as a second partition"
        );

        let row_count = match &topology.root.children {
            edpcli::application::inspect_tree::InspectChildren::Materialized(children) => {
                children.len()
            }
            _ => panic!("root must materialize only structural regions"),
        };
        assert!(row_count < 32, "{mode:?} topology is unexpectedly eager");
    }
}

#[test]
fn plain_mbr_partition_is_browsable_and_lce_absence_is_explicit() {
    let total = 2_000_000u64;
    let mut protocol = vec![0u8; METADATA_IMAGE_LEN];
    let entry = 0x1be;
    protocol[entry + 4] = 0x07;
    protocol[entry + 8..entry + 12].copy_from_slice(&2_048u32.to_le_bytes());
    protocol[entry + 12..entry + 16].copy_from_slice(&1_000_000u32.to_le_bytes());
    protocol[510..512].copy_from_slice(&[0x55, 0xaa]);

    let context = InspectDiskContext::new(protocol, None, total);
    assert!(context.lce.is_none());
    assert!(context.partitions.is_empty());

    let topology = build_inspect_topology(&context);
    assert!(topology
        .regions_for_lba(900_000)
        .iter()
        .any(|node| node.id == "region.mbr_partition.0"));
    assert!(topology
        .regions_for_lba(900_000)
        .iter()
        .all(|node| node.id != "region.lce"));

    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(99)));
    state.advanced_inspect_finish(Ok(workspace(&context)));
    state.advanced_inspect_jump_lba(900_000).unwrap();
    let rows = state.advanced_inspect_tree_rows();
    let selected = state.advanced_inspect().unwrap().tree_selected;
    assert_eq!(rows[selected].kind, InspectNodeKind::Sector);
    assert_eq!(rows[selected].range.start_lba, 900_000);
    assert!(rows.len() < 100, "Plain jump materialized too many rows");
}

#[test]
fn huge_sparse_disk_jump_and_unknown_decode_remain_bounded_and_fail_closed() {
    let total = 4_000_000_000u64;
    let context = InspectDiskContext::new(vec![0; METADATA_IMAGE_LEN], None, total);
    let topology = build_inspect_topology(&context);
    let target = 3_000_000_123u64;

    let unknown = topology
        .primary_region_for_lba(target)
        .expect("target must belong to an unknown region");
    assert!(unknown.id.starts_with("region.unknown."));
    let decode_error = decode_sector(
        &context,
        &InspectMeta::default(),
        target,
        &[0x5a; SECTOR],
        None
    )
    .unwrap_err();
    assert_eq!(decode_error.kind(), InspectErrorKind::Decode);
    assert!(decode_error.message().contains("不属于已注册 decoder"));

    let mut state = AppState::new();
    assert!(state.begin_advanced_inspect(AdvancedInspectSource::Disk(100)));
    state.advanced_inspect_finish(Ok(workspace(&context)));
    state.advanced_inspect_jump_lba(target).unwrap();

    let rows = state.advanced_inspect_tree_rows();
    let selected = state.advanced_inspect().unwrap().tree_selected;
    assert_eq!(rows[selected].range.start_lba, target);
    let materialized = rows
        .iter()
        .filter(|row| row.kind == InspectNodeKind::Sector)
        .count();
    assert!(
        materialized <= 64,
        "sparse disk materialized {materialized} sectors"
    );
    assert!(
        rows.len() < 100,
        "sparse disk tree grew to {} rows",
        rows.len()
    );
    assert!(state
        .advanced_inspect()
        .unwrap()
        .result
        .as_ref()
        .unwrap()
        .items
        .is_empty());
}
