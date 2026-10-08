use std::collections::BTreeMap;
use std::io;

use edpcli::{
    application::filesystem::FilesystemKind,
    application::support::{EXIT_INTERMEDIATE, EXIT_IO, EXIT_ROLLED_BACK, SECTOR},
    diskio::{
        atomic_write_official_provision_sectors, execute_borrowed_data_transaction_observed,
        execute_borrowed_data_transaction_scoped_observed, execute_write_transaction,
        execute_write_transaction_observed, BorrowedFormatBounds, BorrowedFormatLayout,
        SectorWriteStage, TransactionActivityPhase, WriteTransactionPlan,
    },
    ports::SectorDev,
    provision::{
        build_plain_provision_write_plan, PlainCleanupExtent, PlainPartitionSpec,
        PlainProvisionPlan,
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

    fn reopen_rdwr(&mut self, _: std::time::Duration) -> std::io::Result<()> {
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
fn transaction_activity_reports_actual_mirror_write_and_readback_work() {
    let mut plan = WriteTransactionPlan::new(100);
    plan.insert(10, vec![1; SECTOR], SectorWriteStage::Data, "data")
        .unwrap();
    plan.insert(0, vec![2; SECTOR], SectorWriteStage::Commit, "mbr")
        .unwrap();
    let mut events = Vec::new();
    let mut dev = MemoryDev::default();
    execute_write_transaction_observed(&mut dev, &plan, &mut |event| events.push(event)).unwrap();
    assert_eq!(events[0].phase, TransactionActivityPhase::SyncPreflight);
    assert_eq!(events[0].total, 0, "sync has no countable work");
    for phase in [
        TransactionActivityPhase::Mirror,
        TransactionActivityPhase::Write,
        TransactionActivityPhase::Readback,
    ] {
        let work: Vec<_> = events.iter().filter(|event| event.phase == phase).collect();
        assert_eq!(
            work.iter().map(|event| event.current).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert!(work.iter().all(|event| event.total == 2));
    }
    let write_end = events
        .iter()
        .position(|event| event.phase == TransactionActivityPhase::Write && event.current == 2)
        .unwrap();
    assert_eq!(events[write_end + 1].phase, TransactionActivityPhase::Sync);
    assert_eq!(events[write_end + 1].total, 0);
    assert_eq!(
        events[write_end + 2].phase,
        TransactionActivityPhase::Readback
    );
}

#[test]
fn progress_sink_panic_cannot_interrupt_transaction_or_rollback() {
    let mut plan = WriteTransactionPlan::new(100);
    plan.insert(0, vec![2; SECTOR], SectorWriteStage::Commit, "mbr")
        .unwrap();
    let mut dev = MemoryDev::default();
    execute_write_transaction_observed(&mut dev, &plan, &mut |_| panic!("sink failed")).unwrap();
    assert_eq!(dev.sectors[&0], vec![2; SECTOR]);
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
    assert!(
        error.msg.contains("injected provision failure"),
        "{}",
        error.msg
    );
    assert!(error.msg.contains("LBA20"), "{}", error.msg);
    assert_eq!(dev.sectors, before);
}

#[test]
fn plain_plan_maps_to_the_same_generic_transaction_stages() {
    let plain = PlainProvisionPlan::new(
        100_000,
        vec![PlainPartitionSpec::new(
            2_048,
            20_000,
            FilesystemKind::ExFat,
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
    assert!(
        error.msg.contains("injected provision failure"),
        "{}",
        error.msg
    );
    assert!(error.msg.contains("LBA2"), "{}", error.msg);
    assert_eq!(dev.sectors, before);
}

#[derive(Clone, Copy)]
enum FailurePhase {
    Sync,
    Readback,
    WriteAndRollback,
}
struct DiagnosticDev {
    inner: MemoryDev,
    phase: FailurePhase,
    reads: usize,
}
impl SectorDev for DiagnosticDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        self.reads += 1;
        if matches!(self.phase, FailurePhase::Readback) && self.reads == 2 {
            return Err(io::Error::other("injected readback failure"));
        }
        self.inner.read_sector(lba)
    }
    fn write_sector(&mut self, lba: u32, bytes: &[u8]) -> io::Result<()> {
        if matches!(self.phase, FailurePhase::WriteAndRollback) {
            self.inner.calls += 1;
            return Err(io::Error::other(if self.inner.calls == 1 {
                "original write failure"
            } else {
                "rollback write failure"
            }));
        }
        self.inner.write_sector(lba, bytes)
    }
    fn sync(&mut self) -> io::Result<()> {
        self.inner.syncs += 1;
        if matches!(self.phase, FailurePhase::Sync) && self.inner.syncs == 2 {
            return Err(io::Error::other("injected sync failure"));
        }
        Ok(())
    }

    fn reopen_rdwr(&mut self, _: std::time::Duration) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn transaction_diagnostics_retain_sync_readback_and_rollback_causes() {
    let mut plan = WriteTransactionPlan::new(100);
    plan.insert(10, vec![1; SECTOR], SectorWriteStage::Data, "data")
        .unwrap();
    for (phase, cause, stage) in [
        (FailurePhase::Sync, "injected sync failure", "缓存同步"),
        (
            FailurePhase::Readback,
            "injected readback failure",
            "读回 LBA10",
        ),
        (
            FailurePhase::WriteAndRollback,
            "original write failure",
            "写入 LBA10",
        ),
    ] {
        let mut dev = DiagnosticDev {
            inner: MemoryDev::default(),
            phase,
            reads: 0,
        };
        let error = execute_write_transaction(&mut dev, &plan).unwrap_err();
        assert!(error.msg.contains(cause), "{}", error.msg);
        assert!(error.msg.contains(stage), "{}", error.msg);
        if matches!(phase, FailurePhase::WriteAndRollback) {
            assert_eq!(error.code, edpcli::application::support::EXIT_INTERMEDIATE);
            assert!(
                error.msg.contains("rollback write failure"),
                "{}",
                error.msg
            );
        } else {
            assert_eq!(error.code, EXIT_ROLLED_BACK);
            assert_eq!(dev.inner.sectors[&10], vec![0; SECTOR]);
        }
    }
}

#[test]
fn contiguous_rollback_restores_sparse_noncontiguous_lbas_including_mbr_last() {
    let mut plan = WriteTransactionPlan::new(130_000);
    for (lba, stage) in [
        (100_000, SectorWriteStage::Data),
        (1024, SectorWriteStage::Data),
        (12, SectorWriteStage::Metadata),
        (0, SectorWriteStage::Commit),
    ] {
        plan.insert(lba, vec![0xcc; SECTOR], stage, "sparse")
            .unwrap();
    }
    let mut dev = MemoryDev::default();
    for &lba in &[0, 12, 1024, 100_000] {
        dev.sectors.insert(lba, vec![(lba % 251) as u8; SECTOR]);
    }
    let previous = dev.sectors.clone();
    dev.fail_call = Some(3);
    let error = execute_write_transaction(&mut dev, &plan).unwrap_err();
    assert_eq!(error.code, EXIT_ROLLED_BACK, "{}", error.msg);
    assert_eq!(dev.sectors, previous);
    assert_eq!(&dev.writes[0..2], &[1024, 100_000]);
}

#[test]
fn file_sector_read_into_matches_allocating_read_and_rejects_truncated_data() {
    use edpcli::diskio::FileDev;
    let path = std::env::temp_dir().join(format!(
        "edpcli-sector-readinto-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut bytes = vec![0x32; SECTOR * 2];
    bytes[SECTOR..].fill(0x72);
    std::fs::write(&path, &bytes).unwrap();
    let mut dev = FileDev::open_rdonly(path.to_str().unwrap()).unwrap();
    let mut buffer = [0_u8; SECTOR];
    dev.read_sector_into(1, &mut buffer).unwrap();
    assert_eq!(buffer.as_slice(), dev.read_sector(1).unwrap());
    assert!(buffer.iter().all(|&value| value == 0x72));
    std::fs::write(&path, &bytes[..SECTOR]).unwrap();
    let error = dev.read_sector_into(1, &mut buffer).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    assert!(error.to_string().contains("LBA1"), "{error}");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn mirror_read_failure_after_first_sector_prevents_any_write() {
    struct ReadFaultDev(MemoryDev);
    impl SectorDev for ReadFaultDev {
        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            if lba == 32 {
                return Err(io::Error::other("read second mirror error"));
            }
            self.0.read_sector(lba)
        }
        fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
            self.0.write_sector(lba, data)
        }
        fn sync(&mut self) -> io::Result<()> {
            self.0.sync()
        }
        fn reopen_rdwr(&mut self, wait: std::time::Duration) -> io::Result<()> {
            self.0.reopen_rdwr(wait)
        }
    }
    let mut plan = WriteTransactionPlan::new(100);
    plan.insert(5, vec![5; SECTOR], SectorWriteStage::Data, "first")
        .unwrap();
    plan.insert(32, vec![32; SECTOR], SectorWriteStage::Data, "second")
        .unwrap();
    let mut dev = ReadFaultDev(MemoryDev::default());
    let error = execute_write_transaction(&mut dev, &plan).unwrap_err();
    assert_eq!(error.code, EXIT_IO);
    assert!(error.msg.contains("LBA32"), "{}", error.msg);
    assert_eq!(dev.0.syncs, 1);
    assert!(dev.0.writes.is_empty());
}

#[test]
fn unexpected_mirror_sector_size_blocks_all_writes() {
    struct MalformedDev(MemoryDev);
    impl SectorDev for MalformedDev {
        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            if lba == 19 {
                Ok(vec![0xaa; SECTOR - 1])
            } else {
                self.0.read_sector(lba)
            }
        }
        fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
            self.0.write_sector(lba, data)
        }
        fn sync(&mut self) -> io::Result<()> {
            self.0.sync()
        }
        fn reopen_rdwr(&mut self, wait: std::time::Duration) -> io::Result<()> {
            self.0.reopen_rdwr(wait)
        }
    }
    let mut plan = WriteTransactionPlan::new(100);
    plan.insert(19, vec![2; SECTOR], SectorWriteStage::Data, "malformed")
        .unwrap();
    let mut dev = MalformedDev(MemoryDev::default());
    let error = execute_write_transaction(&mut dev, &plan).unwrap_err();
    assert_eq!(error.code, EXIT_IO);
    assert!(error.msg.contains("LBA19"), "{}", error.msg);
    assert!(
        dev.0.writes.is_empty(),
        "malformed mirror must abort before write"
    );
    assert_eq!(dev.0.syncs, 1, "preflight sync must remain first");
}

#[test]
fn borrowed_format_transaction_has_owned_transaction_sync_mirror_and_readback_semantics() {
    let payloads = [[0x41_u8; SECTOR], [0x81; SECTOR], [0xc1; SECTOR]];
    let borrowed = [
        (63_u32, &payloads[0]),
        (500_u32, &payloads[1]),
        (1200_u32, &payloads[2]),
    ];
    let mut owned = WriteTransactionPlan::new(2000);
    for &(lba, bytes) in &borrowed {
        owned
            .insert(lba, bytes.to_vec(), SectorWriteStage::Data, "filesystem")
            .unwrap();
    }
    let mut a = MemoryDev::default();
    let mut b = MemoryDev::default();
    for dev in [&mut a, &mut b] {
        for &lba in &[63, 500, 1200] {
            dev.sectors.insert(lba, vec![0xda; SECTOR]);
        }
    }
    let mut owned_events = Vec::new();
    let mut borrowed_events = Vec::new();
    execute_write_transaction_observed(&mut a, &owned, &mut |event| owned_events.push(event))
        .unwrap();
    execute_borrowed_data_transaction_observed(&mut b, 2000, &borrowed, &mut |event| {
        borrowed_events.push(event)
    })
    .unwrap();
    assert_eq!(a.sectors, b.sectors);
    assert_eq!(a.writes, b.writes);
    assert_eq!(a.syncs, b.syncs);
    assert_eq!(owned_events, borrowed_events);
}

#[test]
fn borrowed_format_plan_validation_fails_before_any_io() {
    let bytes = [[0x21_u8; SECTOR], [0x31; SECTOR]];
    type BorrowedCase<'a> = (&'a str, u64, &'a [(u32, &'a [u8; SECTOR])]);
    let cases: [BorrowedCase<'_>; 5] = [
        ("unsorted", 400, &[(100, &bytes[0]), (63, &bytes[1])]),
        ("duplicate", 400, &[(63, &bytes[0]), (63, &bytes[1])]),
        ("out of bounds", 100, &[(100, &bytes[0])]),
        ("protocol sector", 400, &[(0, &bytes[0])]),
        ("protocol metadata", 400, &[(12, &bytes[0])]),
    ];
    for (label, sectors, writes) in cases {
        let mut dev = MemoryDev::default();
        let err =
            execute_borrowed_data_transaction_observed(&mut dev, sectors, writes, &mut |_| {})
                .unwrap_err();
        assert_eq!(err.code, EXIT_IO, "{label}: {}", err.msg);
        assert_eq!(dev.syncs, 0, "{label} must fail before even sync");
        assert!(dev.writes.is_empty(), "{label} must not write");
    }
}

#[test]
fn borrowed_format_rollback_restores_all_touched_sectors_after_partial_write() {
    let data = [[0x23_u8; SECTOR], [0x42; SECTOR], [0x61; SECTOR]];
    let writes = [(63, &data[0]), (101, &data[1]), (2050, &data[2])];
    let mut dev = MemoryDev::default();
    for &lba in &[63, 101, 2050] {
        dev.sectors.insert(lba, vec![(lba % 251) as u8; SECTOR]);
    }
    let original = dev.sectors.clone();
    dev.fail_call = Some(2);
    let mut phases = Vec::new();
    let err =
        execute_borrowed_data_transaction_observed(&mut dev, 3000, &writes, &mut |activity| {
            phases.push(activity.phase)
        })
        .unwrap_err();
    assert_eq!(err.code, EXIT_ROLLED_BACK, "{}", err.msg);
    assert_eq!(dev.sectors, original);
    assert!(phases.contains(&TransactionActivityPhase::RollbackWrite));
    assert!(phases.contains(&TransactionActivityPhase::RollbackReadback));
    assert_eq!(
        dev.syncs, 2,
        "preflight and rollback sync; normal write sync is skipped on write failure"
    );
}

#[derive(Default)]
struct BatchedFixtureDev {
    inner: MemoryDev,
    read_batches: usize,
    write_batches: usize,
    fail_batch_after_first_sector: bool,
    corrupt_first_readback_lba: Option<u32>,
}

impl SectorDev for BatchedFixtureDev {
    fn max_contiguous_sectors(&self) -> usize {
        4
    }
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        self.inner.read_sector(lba)
    }
    fn read_contiguous_sectors_into(
        &mut self,
        first: u32,
        sectors: &mut [[u8; SECTOR]],
    ) -> io::Result<()> {
        self.read_batches += 1;
        for (index, dst) in sectors.iter_mut().enumerate() {
            let lba = first + index as u32;
            dst.copy_from_slice(&self.inner.read_sector(lba)?);
            if self.inner.syncs == 2 && self.corrupt_first_readback_lba == Some(lba) {
                dst[0] ^= 0xff;
            }
        }
        Ok(())
    }
    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
        self.inner.write_sector(lba, data)
    }
    fn write_contiguous_sectors(&mut self, first: u32, sectors: &[[u8; SECTOR]]) -> io::Result<()> {
        self.write_batches += 1;
        if self.fail_batch_after_first_sector {
            self.fail_batch_after_first_sector = false;
            self.inner.write_sector(first, &sectors[0])?;
            return Err(io::Error::other("partial batch write injected"));
        }
        for (index, sector) in sectors.iter().enumerate() {
            self.inner.write_sector(first + index as u32, sector)?;
        }
        Ok(())
    }
    fn sync(&mut self) -> io::Result<()> {
        self.inner.sync()
    }
    fn reopen_rdwr(&mut self, wait: std::time::Duration) -> io::Result<()> {
        self.inner.reopen_rdwr(wait)
    }
}

fn contiguous_test_plan() -> WriteTransactionPlan {
    let mut plan = WriteTransactionPlan::new(100);
    for lba in 20..29 {
        plan.insert(
            lba,
            vec![lba as u8; SECTOR],
            SectorWriteStage::Data,
            "batch",
        )
        .unwrap();
    }
    plan.insert(
        1,
        vec![0x81; SECTOR],
        SectorWriteStage::Metadata,
        "metadata",
    )
    .unwrap();
    plan.insert(0, vec![0xb0; SECTOR], SectorWriteStage::Commit, "mbr")
        .unwrap();
    plan
}

#[test]
fn batched_file_style_io_preserves_stage_order_sync_and_per_sector_progress() {
    let mut dev = BatchedFixtureDev::default();
    let mut events = Vec::new();
    execute_write_transaction_observed(&mut dev, &contiguous_test_plan(), &mut |e| events.push(e))
        .unwrap();
    assert!(dev.write_batches >= 2);
    assert!(dev.read_batches >= 2);
    assert_eq!(dev.inner.writes, [20, 21, 22, 23, 24, 25, 26, 27, 28, 1, 0]);
    assert_eq!(dev.inner.syncs, 2);
    for phase in [
        TransactionActivityPhase::Mirror,
        TransactionActivityPhase::Write,
        TransactionActivityPhase::Readback,
    ] {
        let progress: Vec<_> = events
            .iter()
            .filter(|event| event.phase == phase)
            .map(|event| event.current)
            .collect();
        assert_eq!(progress, (0..=11).collect::<Vec<_>>(), "{phase:?}");
    }
}

#[test]
fn partially_written_contiguous_batch_rolls_back_every_planned_sector() {
    let mut dev = BatchedFixtureDev {
        fail_batch_after_first_sector: true,
        ..Default::default()
    };
    for lba in [0, 1, 20, 21, 22, 23, 24, 25, 26, 27, 28] {
        dev.inner.sectors.insert(lba, vec![0x97; SECTOR]);
    }
    let before = dev.inner.sectors.clone();
    let mut events = Vec::new();
    let error = execute_write_transaction_observed(&mut dev, &contiguous_test_plan(), &mut |e| {
        events.push(e)
    })
    .unwrap_err();
    assert_eq!(error.code, EXIT_ROLLED_BACK, "{}", error.msg);
    assert!(error.msg.contains("partial batch write injected"));
    assert_eq!(dev.inner.sectors, before);
    assert_eq!(dev.inner.syncs, 2, "preflight + rollback sync");
    assert!(events
        .iter()
        .any(|e| e.phase == TransactionActivityPhase::RollbackReadback && e.current == 11));
    assert_eq!(
        dev.inner.writes.last(),
        Some(&0),
        "rollback still commits MBR last"
    );
}

#[test]
fn corrupted_contiguous_readback_triggers_exact_rollback() {
    let mut dev = BatchedFixtureDev {
        corrupt_first_readback_lba: Some(22),
        ..Default::default()
    };
    for lba in [0, 1, 20, 21, 22, 23, 24, 25, 26, 27, 28] {
        dev.inner.sectors.insert(lba, vec![0x74; SECTOR]);
    }
    let before = dev.inner.sectors.clone();
    let error = execute_write_transaction(&mut dev, &contiguous_test_plan()).unwrap_err();
    assert_eq!(error.code, EXIT_ROLLED_BACK, "{}", error.msg);
    assert!(error.msg.contains("LBA22"), "{}", error.msg);
    assert_eq!(dev.inner.sectors, before);
    assert_eq!(dev.inner.syncs, 3, "preflight + write sync + rollback sync");
}

#[test]
fn edp_format_rejects_full_protocol_reserve_without_io_but_accepts_first_data_sector() {
    let bytes = [0x39_u8; SECTOR];
    for lba in [0, 12, 13, 20, 62] {
        let mut dev = MemoryDev::default();
        let result = execute_borrowed_data_transaction_observed(
            &mut dev,
            100,
            &[(lba, &bytes)],
            &mut |_| {},
        );
        assert!(result.is_err(), "EDP LBA{lba} unexpectedly accepted");
        assert_eq!(dev.syncs, 0, "LBA{lba} must fail before sync");
        assert!(dev.writes.is_empty());
    }
    let mut dev = MemoryDev::default();
    execute_borrowed_data_transaction_observed(&mut dev, 100, &[(63, &bytes)], &mut |_| {})
        .unwrap();
    assert_eq!(dev.writes, vec![63]);
    assert_eq!(dev.syncs, 2);
}

#[test]
fn scoped_partition_format_rejects_other_partition_and_plain_mbr_overlaps() {
    let bytes = [0x13_u8; SECTOR];
    for (description, layout, start, end, written_lba, allowed) in [
        (
            "edp within start",
            BorrowedFormatLayout::Edp,
            100,
            120,
            100,
            true,
        ),
        (
            "edp within end",
            BorrowedFormatLayout::Edp,
            100,
            120,
            119,
            true,
        ),
        (
            "edp below partition",
            BorrowedFormatLayout::Edp,
            100,
            120,
            99,
            false,
        ),
        (
            "edp after partition",
            BorrowedFormatLayout::Edp,
            100,
            120,
            120,
            false,
        ),
        ("edp header", BorrowedFormatLayout::Edp, 62, 100, 63, false),
        (
            "edp reserved",
            BorrowedFormatLayout::Edp,
            13,
            100,
            13,
            false,
        ),
        ("plain earliest", BorrowedFormatLayout::Plain, 1, 3, 1, true),
        (
            "plain LBA3 protected",
            BorrowedFormatLayout::Plain,
            1,
            4,
            1,
            false,
        ),
        (
            "plain zero protected",
            BorrowedFormatLayout::Plain,
            0,
            3,
            1,
            false,
        ),
        (
            "plain typical",
            BorrowedFormatLayout::Plain,
            2048,
            2200,
            2048,
            true,
        ),
    ] {
        let mut dev = MemoryDev::default();
        let result = execute_borrowed_data_transaction_scoped_observed(
            &mut dev,
            3000,
            &[(written_lba, &bytes)],
            BorrowedFormatBounds {
                start_lba: start,
                end_exclusive: end,
                layout,
            },
            &mut |_| {},
        );
        assert_eq!(result.is_ok(), allowed, "{description}: {result:?}");
        if allowed {
            assert_eq!(dev.writes, vec![written_lba], "{description}");
            assert_eq!(dev.syncs, 2);
        } else {
            assert!(dev.writes.is_empty(), "{description} wrote media");
            assert_eq!(dev.syncs, 0, "{description} must fail before any I/O");
        }
    }
}

// Fault matrix for the same shared executor used by Provision and formatting.
// All tests use memory-backed sectors and never open a device path.
#[derive(Default)]
struct TransactionFaultMatrixDev {
    inner: MemoryDev,
    read_calls: usize,
    fail_sync: Vec<usize>,
    fail_read: Vec<usize>,
    fail_write: Vec<usize>,
    fail_read_from: Option<usize>,
    fail_write_from: Option<usize>,
}
impl SectorDev for TransactionFaultMatrixDev {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        self.read_calls += 1;
        if self.fail_read.contains(&self.read_calls)
            || self
                .fail_read_from
                .is_some_and(|start| self.read_calls >= start)
        {
            return Err(io::Error::other(format!(
                "injected read #{}",
                self.read_calls
            )));
        }
        self.inner.read_sector(lba)
    }
    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
        self.inner.calls += 1;
        if self.fail_write.contains(&self.inner.calls)
            || self
                .fail_write_from
                .is_some_and(|start| self.inner.calls >= start)
        {
            return Err(io::Error::other(format!(
                "injected write #{}",
                self.inner.calls
            )));
        }
        self.inner.writes.push(lba);
        self.inner.sectors.insert(lba, data.to_vec());
        Ok(())
    }
    fn sync(&mut self) -> io::Result<()> {
        self.inner.syncs += 1;
        if self.fail_sync.contains(&self.inner.syncs) {
            return Err(io::Error::other(format!(
                "injected sync #{}",
                self.inner.syncs
            )));
        }
        Ok(())
    }
    fn reopen_rdwr(&mut self, _: std::time::Duration) -> io::Result<()> {
        Ok(())
    }
}
fn fault_matrix_plan() -> WriteTransactionPlan {
    let mut plan = WriteTransactionPlan::new(1024);
    for (lba, value, stage) in [
        (63, 0x1a, SectorWriteStage::Data),
        (64, 0x2b, SectorWriteStage::Data),
        (12, 0x3c, SectorWriteStage::Metadata),
        (0, 0x4d, SectorWriteStage::Commit),
    ] {
        plan.insert(lba, vec![value; SECTOR], stage, "fault matrix")
            .unwrap();
    }
    plan
}
fn fault_matrix_dev() -> TransactionFaultMatrixDev {
    let mut dev = TransactionFaultMatrixDev::default();
    for lba in [0, 12, 63, 64] {
        dev.inner
            .sectors
            .insert(lba, vec![(lba % 251) as u8; SECTOR]);
    }
    dev
}
#[test]
fn fault_matrix_pre_write_errors_never_mutate_any_sector() {
    for (label, fail_sync, fail_read) in [
        ("preflight sync", vec![1], vec![]),
        ("first mirror read", vec![], vec![1]),
        ("later mirror read", vec![], vec![3]),
    ] {
        let mut dev = fault_matrix_dev();
        let before = dev.inner.sectors.clone();
        dev.fail_sync = fail_sync;
        dev.fail_read = fail_read;
        let error = execute_write_transaction(&mut dev, &fault_matrix_plan()).unwrap_err();
        assert_eq!(error.code, EXIT_IO, "{label}: {}", error.msg);
        assert_eq!(dev.inner.sectors, before, "{label}");
        assert!(dev.inner.writes.is_empty(), "{label}");
    }
}
#[test]
fn fault_matrix_post_write_failures_restore_exact_bytes_and_commit_order() {
    for (label, fail_sync, fail_read, fail_write) in [
        ("write error", vec![], vec![], vec![2]),
        ("write sync", vec![2], vec![], vec![]),
        ("readback", vec![], vec![5], vec![]),
    ] {
        let mut dev = fault_matrix_dev();
        let before = dev.inner.sectors.clone();
        dev.fail_sync = fail_sync;
        dev.fail_read = fail_read;
        dev.fail_write = fail_write;
        let mut progress = Vec::new();
        let error =
            execute_write_transaction_observed(&mut dev, &fault_matrix_plan(), &mut |activity| {
                progress.push(activity)
            })
            .unwrap_err();
        assert_eq!(error.code, EXIT_ROLLED_BACK, "{label}: {}", error.msg);
        assert_eq!(dev.inner.sectors, before, "{label}");
        assert_eq!(
            dev.inner.writes.last(),
            Some(&0),
            "{label}: restore LBA0 last"
        );
        assert!(
            progress.iter().any(
                |event| event.phase == TransactionActivityPhase::RollbackReadback
                    && event.current == 4
            ),
            "{label}: complete rollback readback"
        );
    }
}
#[test]
fn fault_matrix_rollback_retry_can_recover_from_transient_failures() {
    let mut dev = fault_matrix_dev();
    let before = dev.inner.sectors.clone();
    // First normal write of LBA63 succeeds; second write fails.
    dev.fail_write = vec![2, 3];
    let error = execute_write_transaction(&mut dev, &fault_matrix_plan()).unwrap_err();
    assert_eq!(error.code, EXIT_ROLLED_BACK, "{}", error.msg);
    assert_eq!(dev.inner.sectors, before);
    assert!(
        dev.inner.calls > 5,
        "rollback must retry after injected error"
    );

    let mut dev = fault_matrix_dev();
    let before = dev.inner.sectors.clone();
    dev.fail_read = vec![5, 6];
    let error = execute_write_transaction(&mut dev, &fault_matrix_plan()).unwrap_err();
    assert_eq!(error.code, EXIT_ROLLED_BACK, "{}", error.msg);
    assert_eq!(dev.inner.sectors, before);
}
#[test]
fn fault_matrix_permanent_rollback_failure_is_reported_as_intermediate() {
    for (label, fail_sync, fail_read_from, fail_write_from) in [
        ("rollback write unavailable", vec![], None, Some(2)),
        ("rollback sync unavailable", vec![2, 3, 4], None, None),
        ("rollback verification unavailable", vec![], Some(6), None),
    ] {
        let mut dev = fault_matrix_dev();
        let before = dev.inner.sectors.clone();
        dev.fail_sync = fail_sync;
        dev.fail_read_from = fail_read_from;
        dev.fail_write_from = fail_write_from;
        // A deliberate normal failure ensures we enter the rollback path.
        dev.fail_write.push(2);
        let error = execute_write_transaction(&mut dev, &fault_matrix_plan()).unwrap_err();
        assert_eq!(error.code, EXIT_INTERMEDIATE, "{label}: {}", error.msg);
        assert!(
            error.msg.contains("中间状态") || error.msg.contains("中间态"),
            "{label}: {}",
            error.msg
        );
        // Unverifiable durability/rollback readback is INTERMEDIATE even
        // if simulated cached bytes happen to match the original image.
        if fail_write_from.is_some() {
            assert_ne!(dev.inner.sectors, before, "{label}: incomplete rollback");
        }
    }
}
