use std::collections::BTreeMap;
use std::io;

use edpcli::{
    common::{EXIT_IO, EXIT_ROLLED_BACK, SECTOR},
    diskio::{
        atomic_write_official_provision_sectors, execute_write_transaction, SectorDev,
        SectorWriteStage, WriteTransactionPlan,
    },
    provision::{
        build_plain_provision_write_plan, OfficialFilesystemFormat, PlainCleanupExtent,
        PlainPartitionSpec, PlainProvisionPlan,
    },
};

#[derive(Default)]
struct MemoryDev {
    sectors: BTreeMap<u32, Vec<u8>>,
    writes: Vec<u32>,
    syncs: usize,
    calls: usize,
    fail_call: Option<usize>,
}

impl SectorDev for MemoryDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        Ok(self
            .sectors
            .get(&lba)
            .cloned()
            .unwrap_or_else(|| vec![0; SECTOR]))
    }

    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
        self.calls += 1;
        if self.fail_call == Some(self.calls) {
            self.fail_call = None;
            return Err(io::Error::other("injected provision failure"));
        }
        self.writes.push(lba);
        self.sectors.insert(lba, data.to_vec());
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        self.syncs += 1;
        Ok(())
    }
}

fn patch() -> BTreeMap<u32, Vec<u8>> {
    let mut out = BTreeMap::new();
    for lba in 0..=12u32 {
        out.insert(lba, vec![lba as u8; SECTOR]);
    }
    out.insert(63, vec![0x63; SECTOR]);
    out.insert(120, vec![0x78; SECTOR]);
    out.insert(900, vec![0x90; SECTOR]);
    out
}

#[test]
fn generic_transaction_orders_by_stage_then_lba_and_commits_last() {
    let mut plan = WriteTransactionPlan::new(1024);
    plan.insert(0, vec![0; SECTOR], SectorWriteStage::Commit, "mbr")
        .unwrap();
    plan.insert(12, vec![12; SECTOR], SectorWriteStage::Metadata, "metadata")
        .unwrap();
    plan.insert(4, vec![4; SECTOR], SectorWriteStage::Metadata, "metadata")
        .unwrap();
    plan.insert(900, vec![9; SECTOR], SectorWriteStage::Data, "filesystem")
        .unwrap();
    plan.insert(63, vec![6; SECTOR], SectorWriteStage::Data, "lce")
        .unwrap();

    let mut dev = MemoryDev::default();
    execute_write_transaction(&mut dev, &plan).unwrap();
    assert_eq!(dev.writes, vec![63, 900, 4, 12, 0]);
}

#[test]
fn generic_transaction_rejects_duplicate_and_out_of_bounds_writes() {
    let mut plan = WriteTransactionPlan::new(100);
    plan.insert(5, vec![0; SECTOR], SectorWriteStage::Data, "first")
        .unwrap();
    assert!(plan
        .insert(5, vec![1; SECTOR], SectorWriteStage::Metadata, "duplicate")
        .unwrap_err()
        .contains("LBA5"));
    assert!(plan
        .insert(100, vec![0; SECTOR], SectorWriteStage::Data, "outside")
        .unwrap_err()
        .contains("末端"));
}

#[test]
fn generic_transaction_rolls_back_exact_touched_set() {
    let mut plan = WriteTransactionPlan::new(100);
    plan.insert(10, vec![0x10; SECTOR], SectorWriteStage::Data, "data")
        .unwrap();
    plan.insert(
        20,
        vec![0x20; SECTOR],
        SectorWriteStage::Metadata,
        "metadata",
    )
    .unwrap();
    plan.insert(0, vec![0; SECTOR], SectorWriteStage::Commit, "mbr")
        .unwrap();
    let mut dev = MemoryDev::default();
    for &lba in [0u32, 10, 20].iter() {
        dev.sectors.insert(lba, vec![0xaa; SECTOR]);
    }
    let before = dev.sectors.clone();
    dev.fail_call = Some(2);
    let error = execute_write_transaction(&mut dev, &plan).unwrap_err();
    assert_eq!(error.code, EXIT_ROLLED_BACK, "{}", error.msg);
    assert_eq!(dev.sectors, before);
}

#[test]
fn plain_plan_maps_to_the_same_generic_transaction_stages() {
    let plain = PlainProvisionPlan::new(
        100_000,
        vec![PlainPartitionSpec::new(
            2_048,
            20_000,
            OfficialFilesystemFormat::ExFat,
            "DATA",
        )],
    )
    .unwrap();
    let plain = build_plain_provision_write_plan(
        &plain,
        Some(PlainCleanupExtent::new(50_000, 6)),
        &[0x1234_5678],
    )
    .unwrap();
    let transaction = WriteTransactionPlan::from_plain_provision(&plain).unwrap();

    assert_eq!(
        transaction.writes().get(&0).unwrap().stage,
        SectorWriteStage::Commit
    );
    assert_eq!(
        transaction.writes().get(&1).unwrap().stage,
        SectorWriteStage::Metadata
    );
    assert_eq!(
        transaction.writes().get(&2_048).unwrap().stage,
        SectorWriteStage::Data
    );
    assert_eq!(
        transaction.writes().get(&50_000).unwrap().stage,
        SectorWriteStage::Data
    );
    assert!(!transaction.writes().contains_key(&3));
}

#[test]
fn official_writer_commits_data_then_metadata_then_mbr() {
    let mut dev = MemoryDev::default();
    atomic_write_official_provision_sectors(&mut dev, &patch(), 1024).unwrap();
    assert_eq!(&dev.writes[..3], &[63, 120, 900]);
    assert_eq!(&dev.writes[3..15], &(1u32..=12).collect::<Vec<_>>());
    assert_eq!(dev.writes.last(), Some(&0));
    assert!(dev.syncs >= 2);
}

#[test]
fn official_writer_rejects_incomplete_or_out_of_bounds_plan_before_writing() {
    let mut missing = patch();
    missing.remove(&8);
    let mut dev = MemoryDev::default();
    assert_eq!(
        atomic_write_official_provision_sectors(&mut dev, &missing, 1024)
            .unwrap_err()
            .code,
        EXIT_IO
    );
    assert!(dev.writes.is_empty());

    let mut outside = patch();
    outside.insert(1024, vec![0; SECTOR]);
    let mut dev = MemoryDev::default();
    assert_eq!(
        atomic_write_official_provision_sectors(&mut dev, &outside, 1024)
            .unwrap_err()
            .code,
        EXIT_IO
    );
    assert!(dev.writes.is_empty());
}

#[test]
fn official_writer_rolls_back_the_whole_plan() {
    let p = patch();
    let mut dev = MemoryDev::default();
    for &lba in p.keys() {
        dev.sectors.insert(lba, vec![0xaa; SECTOR]);
    }
    let before = dev.sectors.clone();
    dev.fail_call = Some(5);
    let error = atomic_write_official_provision_sectors(&mut dev, &p, 1024).unwrap_err();
    assert_eq!(error.code, EXIT_ROLLED_BACK, "{}", error.msg);
    assert_eq!(dev.sectors, before);
}
