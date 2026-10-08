//! P2 host-only memory/transaction benchmark. Uses an owned disposable regular file.
//! Run in a fresh process to compare max RSS across commits; never opens a USB device.
use edpcli::diskio::{execute_write_transaction, FileDev, SectorWriteStage, WriteTransactionPlan};
use std::{
    fs::{self, File},
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
    let count: u32 = std::env::args()
        .nth(1)
        .map(|n| n.parse().expect("positive sector count"))
        .unwrap_or(32768);
    assert!((1..=131072).contains(&count));
    let path = std::env::temp_dir().join(format!(
        "edpcli-p2-{}-{}-{}.img",
        std::process::id(),
        count,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let image = TempImage(path);
    File::create(&image.0)
        .unwrap()
        .set_len((u64::from(count) + 2048) * 512)
        .unwrap();
    let mut dev = FileDev::open_rdwr(image.0.to_str().unwrap(), Duration::ZERO).unwrap();
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
    plan.insert(0, vec![0xa5; 512], SectorWriteStage::Commit, "mbr-last")
        .unwrap();
    let plan_ms = start.elapsed().as_secs_f64() * 1000.0;
    let rss_after_plan = rss_kib();
    let start = Instant::now();
    execute_write_transaction(&mut dev, &plan).unwrap();
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let rss_peak = rss_kib();
    assert_eq!(dev.read_sector_u64(0).unwrap(), vec![0xa5; 512]);
    assert_eq!(
        dev.read_sector_u64(u64::from(count)).unwrap(),
        vec![(count % 251) as u8; 512]
    );
    println!("count={count} plan_ms={plan_ms:.3} transaction_ms={elapsed_ms:.3} rss_after_plan_kib={rss_after_plan} peak_rss_kib={rss_peak} peak_delta_kib={}", rss_peak - rss_after_plan);
}
