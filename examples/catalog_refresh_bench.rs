//! Host-only read benchmark. Creates synthetic backup files, never opens raw devices.
use edpcli::{
    edpb::{self, CoreCapture},
    infrastructure::backup_store::display_catalog::scan_backup_dir_display,
    ports::ReadControl,
};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
fn fixture(dir: &Path, count: usize, mixed: bool) {
    let raw = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/protocol/disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin"));
    for index in 0..count {
        let path = dir.join(format!("{index:04}.edpb"));
        if mixed && index % 2 == 0 {
            fs::write(path, vec![0x42; 8192]).unwrap();
            continue;
        }
        let capture = CoreCapture {
            snapshot_id: format!("bench-{index}"),
            created_epoch: 1_789_000_000,
            disk_number: Some(6),
            vid: "0dd8".into(),
            pid: "2005".into(),
            device_id: "disk&ven_netac&prod_onlydisk".into(),
            onlyid: Some("1402259934".into()),
            total_sectors: Some(122_880_000),
            logical_sector_size: 512,
            edpcli_version: "benchmark".into(),
            device_state: "encrypted".into(),
            lba0_12: raw,
        };
        edpb::write_core_backup(&path, &capture).unwrap();
    }
}
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
fn control() -> ReadControl {
    ReadControl::new(1024 * 1024 * 1024, Duration::from_secs(30))
}
fn main() {
    let root = std::env::temp_dir().join(format!(
        "edpcli-catalog-bench-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    println!("dataset,count,cold_ms,cold_bytes,warm_p50_ms,warm_p95_ms,warm_bytes,warm_hits,peak_rss_kib");
    for mixed in [false, true] {
        for count in [10, 100, 1000] {
            let dir = root.join(format!("{mixed}-{count}"));
            fs::create_dir(&dir).unwrap();
            fixture(&dir, count, mixed);
            let cold = scan_backup_dir_display(&dir, &control()).unwrap();
            assert_eq!(cold.entries.len(), count);
            let mut durations = Vec::new();
            let mut last = None;
            for _ in 0..9 {
                let scan = scan_backup_dir_display(&dir, &control()).unwrap();
                assert_eq!(scan.entries.len(), count);
                durations.push(scan.elapsed.as_secs_f64() * 1000.0);
                last = Some(scan);
            }
            durations.sort_by(f64::total_cmp);
            let last = last.unwrap();
            println!(
                "{},{count},{:.3},{},{:.3},{:.3},{},{},{}",
                if mixed { "mixed" } else { "valid" },
                cold.elapsed.as_secs_f64() * 1000.0,
                cold.bytes_read,
                durations[4],
                durations[8],
                last.bytes_read,
                last.cache_hits,
                rss_kib()
            );
            fs::remove_dir_all(dir).unwrap();
        }
    }
    fs::write(root.join("cancel.edpb"), vec![0x42; 16 * 1024 * 1024]).unwrap();
    let control = control();
    let cancel = control.clone();
    let start = Instant::now();
    let handle = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1));
        cancel.cancel();
    });
    let result = scan_backup_dir_display(&root, &control);
    handle.join().unwrap();
    println!(
        "cancel_result={:?},cancel_response_ms={:.3},read_bytes={}",
        result.as_ref().err(),
        start.elapsed().as_secs_f64() * 1000.0,
        control.bytes_read()
    );
    assert!(result.is_err());
    fs::remove_dir_all(root).unwrap();
}
