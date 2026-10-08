//! P0 performance fixture: a disposable regular file, never a raw USB device.
use edpcli::diskio::{
    execute_write_transaction_observed, FileDev, SectorWriteStage, TransactionActivityPhase,
    WriteTransactionPlan,
};
use std::{
    fs,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
fn rss_kib() -> i64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe {
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
    }
    #[cfg(target_os = "macos")]
    {
        usage.ru_maxrss / 1024
    }
    #[cfg(not(target_os = "macos"))]
    {
        usage.ru_maxrss
    }
}
#[cfg(not(unix))]
fn rss_kib() -> i64 {
    -1
}

struct TempImage(PathBuf);
impl Drop for TempImage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn main() {
    println!("sectors,plan_ms,preflight_ms,mirror_ms,write_ms,sync_ms,readback_ms,total_ms,inspect_1k_ms,peak_rss_kib");
    for count in [128_u32, 1024] {
        let path = std::env::temp_dir().join(format!(
            "edpcli-p0-{}-{}-{}.img",
            std::process::id(),
            count,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let image = TempImage(path);
        fs::write(&image.0, vec![0_u8; (count as usize + 2048) * 512]).unwrap();
        let mut dev =
            FileDev::open_rdwr(image.0.to_str().unwrap(), std::time::Duration::ZERO).unwrap();
        let start = Instant::now();
        let mut plan = WriteTransactionPlan::new(u64::from(count) + 2048);
        for lba in 1..=count {
            plan.insert(
                lba,
                vec![(lba % 251) as u8; 512],
                SectorWriteStage::Data,
                "fixture",
            )
            .unwrap();
        }
        plan.insert(0, vec![0xA5; 512], SectorWriteStage::Commit, "commit last")
            .unwrap();
        let planned = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let mut transitions = Vec::new();
        execute_write_transaction_observed(&mut dev, &plan, &mut |activity| {
            if activity.current == 0 {
                transitions.push((activity.phase, Instant::now()));
            }
        })
        .expect("disposable image transaction must pass readback");
        let total = start.elapsed().as_secs_f64() * 1000.0;
        let mut durations = [0.0_f64; 5];
        for (index, (phase, time)) in transitions.iter().enumerate() {
            let end = transitions
                .get(index + 1)
                .map(|(_, next)| *next)
                .unwrap_or_else(Instant::now);
            let ms = end.duration_since(*time).as_secs_f64() * 1000.0;
            match phase {
                TransactionActivityPhase::SyncPreflight => durations[0] += ms,
                TransactionActivityPhase::Mirror => durations[1] += ms,
                TransactionActivityPhase::Write => durations[2] += ms,
                TransactionActivityPhase::Sync => durations[3] += ms,
                TransactionActivityPhase::Readback => durations[4] += ms,
                _ => panic!("unexpected rollback or unrelated stage in benchmark"),
            }
        }
        let start = Instant::now();
        for index in 0..1000 {
            let sector = dev.read_sector_u64((index % u64::from(count)) + 1).unwrap();
            assert_eq!(sector.len(), 512);
        }
        let inspect = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{count},{planned:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{total:.3},{inspect:.3},{}",
            durations[0],
            durations[1],
            durations[2],
            durations[3],
            durations[4],
            rss_kib()
        );
    }
}
