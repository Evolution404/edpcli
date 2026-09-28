//! Production post-restore format authorization with a sparse virtual sector device.
#![cfg(target_os = "macos")]

use crate::common::{self, load_disk_image, netac_runner};
use edpcli::application::media_identity::MediaIdentityPin;
use edpcli::application::post_restore::{
    assess_partitions_readonly, format_partition_on_disk, MetadataRestoreOutcome,
    MetadataRestoreReport, PartitionFormatRequest, PostRestorePartitionState,
};
use edpcli::application::Prompter;
use edpcli::common::SECTOR;
use edpcli::diskio::SectorDev;
use edpcli::edpb::ManifestPartition;
use edpcli::provision::OfficialFilesystemFormat;
use std::collections::BTreeMap;
use std::io;
use std::time::Duration;

const TOTAL: u64 = 122_880_000;

struct SparseFormatDev {
    sectors: BTreeMap<u32, Vec<u8>>,
    writes: Vec<u32>,
    syncs: usize,
    swap_on_reopen: bool,
    fail_write: bool,
}

impl SparseFormatDev {
    fn new() -> Self {
        let mut mbr = vec![0u8; SECTOR];
        for (slot, start) in [(0, 2_048u32), (1, 150_000u32)] {
            let base = 446 + slot * 16;
            mbr[base + 4] = 0x07;
            mbr[base + 8..base + 12].copy_from_slice(&start.to_le_bytes());
            mbr[base + 12..base + 16].copy_from_slice(&100_000u32.to_le_bytes());
        }
        mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
        Self {
            sectors: BTreeMap::from([(0, mbr)]),
            writes: Vec::new(),
            syncs: 0,
            swap_on_reopen: false,
            fail_write: false,
        }
    }
}

impl SectorDev for SparseFormatDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        Ok(self
            .sectors
            .get(&lba)
            .cloned()
            .unwrap_or_else(|| vec![0u8; SECTOR]))
    }

    fn write_sector(&mut self, lba: u32, bytes: &[u8]) -> io::Result<()> {
        if self.fail_write {
            return Err(io::Error::other("injected format failure"));
        }
        self.writes.push(lba);
        self.sectors.insert(lba, bytes.to_vec());
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        self.syncs += 1;
        Ok(())
    }

    fn reopen_rdwr(&mut self, _wait: Duration) -> io::Result<()> {
        if self.swap_on_reopen {
            self.sectors.get_mut(&0).unwrap()[454..458].copy_from_slice(&4_096u32.to_le_bytes());
        }
        Ok(())
    }
}

struct Confirm(bool);

impl Prompter for Confirm {
    fn prompt_line(&mut self, _message: &str) -> String {
        String::new()
    }

    fn confirm_yes(&mut self, _message: &str) -> bool {
        self.0
    }
}

fn partitions() -> Vec<ManifestPartition> {
    [(1, 2_048), (2, 150_000)]
        .into_iter()
        .map(|(index, start_lba)| ManifestPartition {
            index,
            role: Some("mbr_primary".into()),
            partition_type: Some("mbr:0x07".into()),
            start_lba,
            sector_count: 100_000,
            filesystem_hint: Some("exfat".into()),
            volume_label_hint: None,
        })
        .collect()
}

fn fixture(dev: &mut SparseFormatDev) -> (common::FakeRunner, MetadataRestoreOutcome) {
    let runner = netac_runner(6);
    let observed = edpcli::application::media_identity_observer::observe_media_identity_readonly(
        &runner, 6, dev,
    )
    .unwrap();
    let pin = MediaIdentityPin::new(observed.snapshot, &observed.protocol_image);
    let parts = partitions();
    let assessment = assess_partitions_readonly(dev, "plain", "", TOTAL, &parts).unwrap();
    let outcome = MetadataRestoreOutcome {
        report: MetadataRestoreReport {
            metadata_restored: true,
            readback_verified: true,
            restored_artifact_ids: vec!["raw.plain.partition_table.0".into()],
        },
        assessment,
        partitions: parts,
        device_state: "plain".into(),
        device_id: String::new(),
        total_sectors: TOTAL,
        format_target_pin: Some(
            edpcli::application::media_identity::MediaIdentityResumePin::from_pin(&pin),
        ),
    };
    (runner, outcome)
}

fn request() -> PartitionFormatRequest {
    PartitionFormatRequest {
        partition_index: 1,
        filesystem: OfficialFilesystemFormat::ExFat,
    }
}

fn run(
    runner: &common::FakeRunner,
    dev: &mut SparseFormatDev,
    confirm: bool,
    outcome: &MetadataRestoreOutcome,
) -> edpcli::application::post_restore::PostRestoreFormatResult {
    format_partition_on_disk(
        runner,
        6,
        dev,
        &mut Confirm(confirm),
        outcome.format_target_pin.as_ref().unwrap(),
        outcome,
        &request(),
        "恢复卷",
        0x1234_5678,
    )
}

#[test]
fn unconfirmed_format_has_zero_writes() {
    let mut dev = SparseFormatDev::new();
    let (runner, outcome) = fixture(&mut dev);
    assert!(run(&runner, &mut dev, false, &outcome).result.is_err());
    assert!(dev.writes.is_empty());
}

#[test]
fn non_needs_format_has_zero_writes() {
    let mut dev = SparseFormatDev::new();
    let (runner, mut outcome) = fixture(&mut dev);
    outcome.assessment.partitions[0].state = PostRestorePartitionState::Usable;
    assert!(run(&runner, &mut dev, true, &outcome).result.is_err());
    assert!(dev.writes.is_empty());
}

#[test]
fn physical_identity_mismatch_has_zero_writes() {
    let mut dev = SparseFormatDev::new();
    let (runner, mut outcome) = fixture(&mut dev);
    outcome.format_target_pin.as_mut().unwrap().vid = Some(0xffff);
    assert!(run(&runner, &mut dev, true, &outcome).result.is_err());
    assert!(dev.writes.is_empty());
}

#[test]
fn geometry_mismatch_has_zero_writes() {
    let mut dev = SparseFormatDev::new();
    let (runner, mut outcome) = fixture(&mut dev);
    outcome.partitions[0].sector_count += 1;
    assert!(run(&runner, &mut dev, true, &outcome).result.is_err());
    assert!(dev.writes.is_empty());
}

#[test]
fn reopen_media_swap_has_zero_writes() {
    let mut dev = SparseFormatDev::new();
    let (runner, outcome) = fixture(&mut dev);
    dev.swap_on_reopen = true;
    assert!(run(&runner, &mut dev, true, &outcome).result.is_err());
    assert!(dev.writes.is_empty());
}

#[test]
fn formats_only_selected_partition_and_reassesses_usable() {
    let mut dev = SparseFormatDev::new();
    let (runner, outcome) = fixture(&mut dev);
    let result = run(&runner, &mut dev, true, &outcome);
    assert!(result.result.is_ok(), "{:?}", result.result);
    assert!(!dev.writes.is_empty());
    assert!(dev.writes.iter().all(|lba| (2_048..102_048).contains(lba)));
    assert_eq!(dev.syncs, 1);
    let after =
        assess_partitions_readonly(&mut dev, "plain", "", TOTAL, &outcome.partitions).unwrap();
    assert_eq!(after.partitions[0].state, PostRestorePartitionState::Usable);
    assert_eq!(
        after.partitions[1].state,
        PostRestorePartitionState::NeedsFormat
    );
}

#[test]
fn format_failure_preserves_verified_metadata_restore_result() {
    let mut dev = SparseFormatDev::new();
    let (runner, outcome) = fixture(&mut dev);
    dev.fail_write = true;
    assert!(run(&runner, &mut dev, true, &outcome).result.is_err());
    assert!(outcome.report.metadata_restored);
    assert!(outcome.report.readback_verified);
}

#[test]
fn encrypted_edp_partition_cannot_enter_plaintext_format() {
    let Some(image) = load_disk_image("netac") else {
        eprintln!("跳过: 真实协议夹具不可用");
        return;
    };
    let protocol = image[..13 * SECTOR].to_vec();
    let provision = edpcli::provision::ProvisionImage::from_bytes(protocol.clone()).unwrap();
    let parsed = edpcli::provision::parse_existing_provision(
        &provision,
        "disk&ven_netac&prod_onlydisk",
        TOTAL,
    )
    .unwrap()
    .unwrap();
    let Some((index, partition)) = parsed
        .profile
        .partitions
        .iter()
        .enumerate()
        .find(|(index, _)| parsed.records[*index].lba12.need_encrypt != 0)
    else {
        eprintln!("跳过: 夹具没有加密分区");
        return;
    };
    let manifest = ManifestPartition {
        index: index as u32 + 1,
        role: Some("encrypt".into()),
        partition_type: Some("edp:4".into()),
        start_lba: partition.start_lba,
        sector_count: partition.sector_count,
        filesystem_hint: Some("exfat".into()),
        volume_label_hint: None,
    };
    let mut dev = SparseFormatDev::new();
    for lba in 0..13 {
        dev.sectors.insert(
            lba as u32,
            protocol[lba * SECTOR..(lba + 1) * SECTOR].to_vec(),
        );
    }
    let runner = netac_runner(6);
    let observed = edpcli::application::media_identity_observer::observe_media_identity_readonly(
        &runner, 6, &mut dev,
    )
    .unwrap();
    let pin = MediaIdentityPin::new(observed.snapshot, &observed.protocol_image);
    let assessment = edpcli::application::post_restore::PostRestoreAssessment {
        partitions: vec![edpcli::application::post_restore::PostRestorePartition {
            index: manifest.index,
            role: manifest.role.clone(),
            start_lba: manifest.start_lba,
            sector_count: manifest.sector_count,
            filesystem_hint: manifest.filesystem_hint.clone(),
            detected_filesystem: None,
            state: PostRestorePartitionState::NeedsFormat,
            detail: "fixture".into(),
        }],
        issues: Vec::new(),
    };
    let outcome = MetadataRestoreOutcome {
        report: MetadataRestoreReport {
            metadata_restored: true,
            readback_verified: true,
            restored_artifact_ids: vec!["raw.protocol.lba0_12".into()],
        },
        assessment,
        partitions: vec![manifest],
        device_state: "edp".into(),
        device_id: "disk&ven_netac&prod_onlydisk".into(),
        total_sectors: TOTAL,
        format_target_pin: Some(
            edpcli::application::media_identity::MediaIdentityResumePin::from_pin(&pin),
        ),
    };
    let result = format_partition_on_disk(
        &runner,
        6,
        &mut dev,
        &mut Confirm(true),
        outcome.format_target_pin.as_ref().unwrap(),
        &outcome,
        &PartitionFormatRequest {
            partition_index: index as u32 + 1,
            filesystem: OfficialFilesystemFormat::ExFat,
        },
        "恢复卷",
        0x1234_5678,
    );
    assert!(result.result.is_err());
    assert!(dev.writes.is_empty());
}
