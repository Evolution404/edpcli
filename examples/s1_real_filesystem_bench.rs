//! Read-only real FAT metadata construction/verification, plus disposable-file
//! transaction readback for the selected sparse metadata. No raw devices.
use edpcli::application::filesystem::{
    build_empty_filesystem_typed, default_registry, estimate_format_resources, FilesystemError,
    FilesystemErrorKind, FilesystemGeometry, FilesystemKind, FilesystemReader, FormatRequest,
    SparseFilesystemImage,
};
use edpcli::diskio::{
    execute_borrowed_data_transaction_scoped_observed, BorrowedFormatBounds, BorrowedFormatLayout,
    FileDev,
};
use edpcli::ports::SectorDev;
use edpcli::provision::{decrypt_mode2, EdpSm4Transform};
use std::{
    fs::{self, File},
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const START: u64 = 2048;
const KEY: [u8; 16] = [
    0x14, 0x71, 0x96, 0xf5, 0xa2, 0xec, 0x79, 0x12, 0xed, 0xf1, 0x3f, 0x75, 0xd7, 0x66, 0xcb, 0x42,
];

#[cfg(unix)]
fn peak_rss_kib() -> i64 {
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
fn peak_rss_kib() -> i64 {
    -1
}

struct Reader<'a>(&'a SparseFilesystemImage);
impl FilesystemReader for Reader<'_> {
    fn sector_size(&self) -> u32 {
        512
    }
    fn sector_count(&self) -> u64 {
        self.0.volume_sectors()
    }
    fn read_sector(&mut self, relative: u64) -> Result<[u8; 512], FilesystemError> {
        self.0.sector_or_zero(relative).ok_or_else(|| {
            FilesystemError::new(
                FilesystemErrorKind::ReadFailure,
                "filesystem benchmark range",
            )
        })
    }
}
struct TempImage(PathBuf);
impl Drop for TempImage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn format(kind: &str) -> FilesystemKind {
    match kind {
        "fat16" => FilesystemKind::Fat16,
        "fat32" => FilesystemKind::Fat32,
        "exfat" => FilesystemKind::ExFat,
        _ => panic!("filesystem must be fat16/fat32/exfat"),
    }
}
fn main() {
    let argv: Vec<_> = std::env::args().collect();
    assert_eq!(
        argv.len(),
        4,
        "usage: s1_real_filesystem_bench fat16|fat32|exfat sectors plain|sm4"
    );
    let kind = format(&argv[1]);
    let volume: u64 = argv[2].parse().unwrap();
    let sm4 = match argv[3].as_str() {
        "sm4" => true,
        "plain" => false,
        _ => panic!("mode"),
    };
    let estimate = estimate_format_resources(kind, volume).unwrap();
    let before = peak_rss_kib();
    let build = Instant::now();
    let plain =
        build_empty_filesystem_typed(kind, START, volume, 0x89ab_cdef, Some("EDPTEST")).unwrap();
    let build_ms = build.elapsed().as_secs_f64() * 1000.;
    let build_rss = peak_rss_kib();
    let crypto = Instant::now();
    let physical = if sm4 {
        plain.transformed(&EdpSm4Transform::new(KEY))
    } else {
        plain.clone()
    };
    let crypto_ms = crypto.elapsed().as_secs_f64() * 1000.;
    let crypto_rss = peak_rss_kib();
    let verify = Instant::now();
    let registry = default_registry();
    let driver = registry.driver(kind).unwrap();
    let mut request = FormatRequest::new(kind);
    request.volume_label = Some("EDPTEST".to_string());
    request.volume_serial = Some(0x89ab_cdef);
    let expected = driver.expected_format_metadata(&request).unwrap();
    let mut reader = Reader(&plain);
    driver
        .verify_format(
            &mut reader,
            FilesystemGeometry::new(START, volume, 512),
            &expected,
        )
        .unwrap();
    if sm4 {
        // Both ends and a middle metadata sample must decrypt exactly.
        let keys: Vec<_> = physical.sectors().keys().copied().collect();
        for relative in [keys[0], keys[keys.len() / 2], *keys.last().unwrap()] {
            let raw = physical.sectors().get(&relative).unwrap();
            let decoded = decrypt_mode2(raw, &KEY).unwrap();
            assert_eq!(decoded.as_slice(), plain.sectors().get(&relative).unwrap());
        }
    }
    let verify_ms = verify.elapsed().as_secs_f64() * 1000.;
    let last = *physical.sectors().keys().last().unwrap();
    // The file ends after the last touched metadata sector; avoid creating a
    // massive full-volume file just to exercise rollback and sector readback.
    let path = std::env::temp_dir().join(format!(
        "edpcli-s1-real-format-{}-{}-{}-{}.img",
        std::process::id(),
        argv[1],
        volume,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let temp = TempImage(path);
    File::create(&temp.0)
        .unwrap()
        .set_len((START + last + 1) * 512)
        .unwrap();
    let mut dev = FileDev::open_rdwr(temp.0.to_str().unwrap(), Duration::ZERO).unwrap();
    let prepare = Instant::now();
    let writes: Vec<_> = physical
        .sectors()
        .iter()
        .map(|(&lba, sector)| ((START + lba) as u32, sector))
        .collect();
    let prepare_ms = prepare.elapsed().as_secs_f64() * 1000.;
    let transaction = Instant::now();
    execute_borrowed_data_transaction_scoped_observed(
        &mut dev,
        START + volume,
        &writes,
        BorrowedFormatBounds {
            start_lba: START,
            end_exclusive: START + volume,
            layout: BorrowedFormatLayout::Edp,
        },
        &mut |_| {},
    )
    .unwrap();
    let transaction_ms = transaction.elapsed().as_secs_f64() * 1000.;
    // The real disk transaction already verifies every touched sector. Check
    // the first and last ones again without accessing physical hardware.
    for (&relative, expected) in [
        physical.sectors().first_key_value().unwrap(),
        physical.sectors().last_key_value().unwrap(),
    ] {
        assert_eq!(
            &dev.read_sector((START + relative) as u32).unwrap()[..],
            expected.as_slice()
        );
    }
    println!("fs={} volume={} mode={} metadata_sectors={} payload_bytes={} estimate_working_bytes={} build_ms={:.3} transform_ms={:.3} verify_ms={:.3} prepare_ms={:.3} transaction_ms={:.3} rss_before_kib={} rss_build_kib={} rss_transform_kib={} rss_peak_kib={}",
        argv[1], volume, argv[3], physical.sectors().len(), physical.metadata_bytes(),
        estimate.working_payload_bytes, build_ms, crypto_ms, verify_ms,
        prepare_ms, transaction_ms, before, build_rss, crypto_rss, peak_rss_kib());
}
