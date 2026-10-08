//! Compare owned transaction-plan duplication and borrowed format sectors.
//! Entirely disposable file-backed media; does not access physical disks.
use edpcli::application::filesystem::SparseFilesystemImage;
use edpcli::diskio::{
    execute_borrowed_data_transaction_observed, execute_write_transaction, FileDev,
    SectorWriteStage, WriteTransactionPlan,
};
use std::{
    fs::{self, File},
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
fn rss_kib() -> i64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
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
    let variant = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "materialized".into());
    let count: u32 = std::env::args()
        .nth(2)
        .map(|n| n.parse().unwrap())
        .unwrap_or(32768);
    assert!((1..=131072).contains(&count));
    let start_lba = 2048_u32;
    let image = SparseFilesystemImage::from_writes(
        u64::from(count) + 1,
        (0..count).map(|lba| (u64::from(lba), [(lba % 251) as u8; 512])),
    );
    let rss_image = rss_kib();
    let path = std::env::temp_dir().join(format!(
        "edpcli-p2-format-{}-{}-{}.img",
        std::process::id(),
        count,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let file = TempImage(path);
    File::create(&file.0)
        .unwrap()
        .set_len((u64::from(start_lba) + u64::from(count) + 2048) * 512)
        .unwrap();
    let mut dev = FileDev::open_rdwr(file.0.to_str().unwrap(), Duration::ZERO).unwrap();
    let prepare = Instant::now();
    match variant.as_str() {
        "materialized" => {
            let mut plan = WriteTransactionPlan::new(u64::from(start_lba) + u64::from(count) + 1);
            for (&relative, sector) in image.sectors() {
                plan.insert(
                    start_lba + relative as u32,
                    sector.to_vec(),
                    SectorWriteStage::Data,
                    "partition filesystem",
                )
                .unwrap();
            }
            let preparation_ms = prepare.elapsed().as_secs_f64() * 1000.;
            let rss_prepared = rss_kib();
            let written = Instant::now();
            execute_write_transaction(&mut dev, &plan).unwrap();
            let transaction_ms = written.elapsed().as_secs_f64() * 1000.;
            println!("variant={variant} sectors={count} prepare_ms={preparation_ms:.3} transaction_ms={transaction_ms:.3} image_rss_kib={rss_image} prepared_rss_kib={rss_prepared} peak_rss_kib={} peak_delta_kib={}",rss_kib(),rss_kib()-rss_image);
        }
        "borrowed" => {
            let writes = image
                .sectors()
                .iter()
                .map(|(&relative, sector)| (start_lba + relative as u32, sector))
                .collect::<Vec<_>>();
            let preparation_ms = prepare.elapsed().as_secs_f64() * 1000.;
            let rss_prepared = rss_kib();
            let written = Instant::now();
            execute_borrowed_data_transaction_observed(
                &mut dev,
                u64::from(start_lba) + u64::from(count) + 1,
                &writes,
                &mut |_| {},
            )
            .unwrap();
            let transaction_ms = written.elapsed().as_secs_f64() * 1000.;
            println!("variant={variant} sectors={count} prepare_ms={preparation_ms:.3} transaction_ms={transaction_ms:.3} image_rss_kib={rss_image} prepared_rss_kib={rss_prepared} peak_rss_kib={} peak_delta_kib={}",rss_kib(),rss_kib()-rss_image);
        }
        _ => panic!("unsupported benchmark variant"),
    }
    assert_eq!(
        dev.read_sector_u64(u64::from(start_lba) + u64::from(count - 1))
            .unwrap(),
        vec![((count - 1) % 251) as u8; 512]
    );
}
