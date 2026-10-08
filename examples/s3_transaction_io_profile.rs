//! Disposable regular-file end-to-end transaction stage timing and call counts.
//! `single` emulates the raw-device max_contiguous_sectors=1 *interface* only;
//! it is NOT an actual raw-disk latency measurement or a write policy proposal.
use edpcli::{
    diskio::{
        execute_write_transaction_observed, FileDev, SectorWriteStage, TransactionActivityPhase,
        WriteTransactionPlan,
    },
    ports::SectorDev,
};
use std::{
    fs, io,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
struct Counts {
    single_reads: u64,
    batch_reads: u64,
    single_writes: u64,
    batch_writes: u64,
    syncs: u64,
}
struct MeteredDevice {
    inner: FileDev,
    counts: Counts,
    max_batch: usize,
}
impl SectorDev for MeteredDevice {
    fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
        self.counts.single_reads += 1;
        self.inner.read_sector(lba)
    }
    fn read_sector_into(&mut self, lba: u32, out: &mut [u8; 512]) -> io::Result<()> {
        self.counts.single_reads += 1;
        self.inner.read_sector_into(lba, out)
    }
    fn read_contiguous_sectors_into(
        &mut self,
        first: u32,
        into: &mut [[u8; 512]],
    ) -> io::Result<()> {
        self.counts.batch_reads += 1;
        self.inner.read_contiguous_sectors_into(first, into)
    }
    fn write_sector(&mut self, lba: u32, data: &[u8]) -> io::Result<()> {
        self.counts.single_writes += 1;
        self.inner.write_sector(lba, data)
    }
    fn write_contiguous_sectors(&mut self, first: u32, data: &[[u8; 512]]) -> io::Result<()> {
        self.counts.batch_writes += 1;
        self.inner.write_contiguous_sectors(first, data)
    }
    fn max_contiguous_sectors(&self) -> usize {
        self.max_batch
    }
    fn sync(&mut self) -> io::Result<()> {
        self.counts.syncs += 1;
        self.inner.sync()
    }
    fn reopen_rdwr(&mut self, wait: Duration) -> io::Result<()> {
        self.inner.reopen_rdwr(wait)
    }
}
struct TempImage(PathBuf);
impl Drop for TempImage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn main() {
    println!("sectors,shape,io_limit,plan_ms,preflight_ms,mirror_ms,write_ms,sync_ms,readback_ms,total_ms,single_reads,batch_reads,single_writes,batch_writes,syncs");
    for count in [128_u32, 1024, 4096, 16384] {
        for shape in ["contiguous", "strided"] {
            for max_batch in [1_usize, 128] {
                let path = TempImage(std::env::temp_dir().join(format!(
                        "edpcli-s3-io-{}-{}-{}-{}-{}.img",
                        std::process::id(),
                        count,
                        shape,
                        max_batch,
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_nanos()
                    )));
                let upper = count * if shape == "strided" { 2 } else { 1 };
                let f = fs::File::create(&path.0).unwrap();
                f.set_len((u64::from(upper) + 2048) * 512).unwrap();
                let mut dev = MeteredDevice {
                    inner: FileDev::open_rdwr(path.0.to_str().unwrap(), Duration::ZERO).unwrap(),
                    counts: Counts::default(),
                    max_batch,
                };
                let time = Instant::now();
                let mut plan = WriteTransactionPlan::new(u64::from(upper) + 2048);
                for i in 1..=count {
                    let lba = if shape == "strided" { i * 2 } else { i };
                    plan.insert(
                        lba,
                        vec![(i % 251) as u8; 512],
                        SectorWriteStage::Data,
                        "profile",
                    )
                    .unwrap();
                }
                plan.insert(0, vec![0xa5; 512], SectorWriteStage::Commit, "last")
                    .unwrap();
                let plan_ms = time.elapsed().as_secs_f64() * 1000.;
                let begin = Instant::now();
                let mut markers = Vec::new();
                execute_write_transaction_observed(&mut dev, &plan, &mut |event| {
                    if event.current == 0 {
                        markers.push((event.phase, Instant::now()));
                    }
                })
                .unwrap();
                let total_ms = begin.elapsed().as_secs_f64() * 1000.;
                let mut duration = [0_f64; 5];
                for (index, (phase, instant)) in markers.iter().enumerate() {
                    let end = markers
                        .get(index + 1)
                        .map(|(_, t)| *t)
                        .unwrap_or_else(Instant::now);
                    let elapsed = end.duration_since(*instant).as_secs_f64() * 1000.;
                    let stage = match phase {
                        TransactionActivityPhase::SyncPreflight => 0,
                        TransactionActivityPhase::Mirror => 1,
                        TransactionActivityPhase::Write => 2,
                        TransactionActivityPhase::Sync => 3,
                        TransactionActivityPhase::Readback => 4,
                        _ => panic!("unexpected rollback stage"),
                    };
                    duration[stage] += elapsed;
                }
                let c = &dev.counts;
                let end_lba = if shape == "strided" { count * 2 } else { count };
                assert_eq!(
                    dev.inner.read_sector_u64(end_lba as u64).unwrap(),
                    vec![(count % 251) as u8; 512]
                );
                assert_eq!(dev.inner.read_sector_u64(0).unwrap(), vec![0xa5; 512]);
                assert!(c.single_reads + c.batch_reads > 0 && c.single_writes + c.batch_writes > 0);
                assert_eq!(c.syncs, 2);
                println!("{count},{shape},{max_batch},{plan_ms:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{total_ms:.3},{},{},{},{},{}",duration[0],duration[1],duration[2],duration[3],duration[4],c.single_reads,c.batch_reads,c.single_writes,c.batch_writes,c.syncs);
            }
        }
    }
}
