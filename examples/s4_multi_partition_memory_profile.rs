//! Production multi-partition formatting plan/materialization memory profile.
//! Never opens or writes any physical/virtual disk: only pure builders.
use edpcli::{
    application::{
        filesystem::{
            estimate_format_resources, FilesystemKind, FormatResourceBudget, FormatResourceEstimate,
        },
        provision::{plan_format_targets_typed, FormatOptions},
    },
    protocol::lba7_compat::locate_lba7_compatibility_extent_from_geometry,
    provision::{
        build_plain_provision_write_plan, wrap_file_key, wrap_legacy_lba7_file_key,
        FileKeyWrapMode, OfficialPartitionFilesystems, OfficialPartitionMode,
        OfficialPartitionSizes, OfficialProvisionPlan, PlainPartitionSpec, PlainProvisionPlan,
    },
};
use std::time::Instant;
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
fn geometry(big: bool) -> (u64, u64, u64) {
    if big {
        (32 * 2048, 8192 * 2048, 4096 * 2048)
    } else {
        (32 * 2048, 128 * 2048, 256 * 2048)
    }
}
fn resources(volumes: &[(FilesystemKind, u64)]) -> FormatResourceEstimate {
    let mut estimate = FormatResourceEstimate::from_sectors(0).unwrap();
    for &(filesystem, sectors) in volumes {
        estimate = estimate
            .checked_add(estimate_format_resources(filesystem, sectors).unwrap())
            .unwrap();
    }
    FormatResourceBudget::default().check(estimate).unwrap();
    estimate
}
fn main() {
    let name=std::env::args().nth(1).expect("usage: s4_multi_partition_memory_profile official-small|official-large|plain-large|budget-reject");
    let big = name.contains("large") || name == "budget-reject";
    let (boot, share, encrypt) = geometry(big);
    let volume_specs = [
        (FilesystemKind::Fat16, boot),
        (FilesystemKind::Fat32, share),
        (FilesystemKind::ExFat, encrypt),
    ];
    let start_rss = peak_rss_kib();
    let before = Instant::now();
    if name == "budget-reject" {
        let too_large = estimate_format_resources(FilesystemKind::Fat32, 67_108_864).unwrap();
        let estimate = too_large.checked_add(too_large).unwrap();
        assert!(FormatResourceBudget::default().check(estimate).is_err());
        println!("scenario=budget-reject status=rejected-before-alloc metadata_sectors={} estimate_payload_bytes={} estimate_working_bytes={} peak_rss_kib={}",estimate.metadata_sectors,estimate.image_payload_bytes,estimate.working_payload_bytes,peak_rss_kib());
        return;
    }
    let estimate = resources(&volume_specs);
    let resource_ms = before.elapsed().as_secs_f64() * 1000.;
    let estimate_rss = peak_rss_kib();
    let beginning = Instant::now();
    let mut image_count = 0_usize;
    let mut payload_count = 0_usize;
    let mut shared_count = 0_usize;
    let mut crypto_count = 0_usize;
    let plan_rss;
    let plan_ms;
    if name.starts_with("official") {
        let compat = locate_lba7_compatibility_extent_from_geometry(8192, 255, 63, 512).unwrap();
        let plan = OfficialProvisionPlan::new(
            OfficialPartitionMode::DefaultThreePartition,
            OfficialPartitionSizes::new(
                32,
                if big { 8192 } else { 128 },
                if big { 4096 } else { 256 },
            ),
            compat,
            wrap_legacy_lba7_file_key(
                b"0000aaaa",
                [0x7d, 0x9e, 0xe4, 0xe8, 0x75, 0x4a, 0xd4, 0x38],
            ),
            wrap_file_key(b"ProofPass1!", KEY, FileKeyWrapMode::Sm4),
        )
        .unwrap()
        .with_filesystems(OfficialPartitionFilesystems {
            boot: FilesystemKind::Fat16,
            share: FilesystemKind::Fat32,
            encrypt: FilesystemKind::ExFat,
        });
        let count = plan.format_targets().unwrap().len();
        let options = FormatOptions {
            boot: true,
            share: true,
            encrypt: true,
            boot_fs: FilesystemKind::Fat16,
            share_fs: FilesystemKind::Fat32,
            encrypt_fs: FilesystemKind::ExFat,
            ..FormatOptions::default()
        };
        let serials = (0..count)
            .map(|i| 0x1234_0000 + (i as u32))
            .collect::<Vec<_>>();
        let results = plan_format_targets_typed(&plan, &options, &serials, &KEY).unwrap();
        let selected = results.iter().filter(|p| p.selected).collect::<Vec<_>>();
        assert_eq!(selected.len(), 3);
        for choice in &selected {
            let plain = choice.verification_image.as_ref().unwrap();
            let physical = &choice.prepared_image.as_ref().unwrap().image;
            assert_eq!(plain.metadata_bytes(), physical.metadata_bytes());
            image_count += plain.sectors().len();
            payload_count += physical.metadata_bytes();
            if std::ptr::eq(plain.sectors(), physical.sectors()) {
                shared_count += 1;
            } else {
                crypto_count += 1;
                // All positions still resolve to the same transformed sector keys.
                assert_eq!(physical.sectors().len(), plain.sectors().len());
            }
        }
        assert_eq!(image_count as u64, estimate.metadata_sectors);
        plan_ms = beginning.elapsed().as_secs_f64() * 1000.;
        plan_rss = peak_rss_kib();
        std::hint::black_box(&results);
    } else if name == "plain-large" {
        let mut base = 2048_u64;
        let mut parts = Vec::new();
        for (fs, length) in volume_specs {
            parts.push(PlainPartitionSpec::new(base, length, fs, "TEST"));
            base += length;
        }
        let plan = PlainProvisionPlan::new(base + 4096, parts).unwrap();
        let result =
            build_plain_provision_write_plan(&plan, None, &[0x1234_0001, 0x1234_0002, 0x1234_0003])
                .unwrap();
        image_count = result.touched_sector_count();
        payload_count = image_count * 512;
        plan_ms = beginning.elapsed().as_secs_f64() * 1000.;
        plan_rss = peak_rss_kib();
        std::hint::black_box(&result);
    } else {
        panic!("unknown scenario");
    }
    let after = peak_rss_kib();
    println!("scenario={} status=ok selected_partitions=3 metadata_sectors={} plan_sectors={} estimate_payload_bytes={} estimate_working_bytes={} actual_image_payload_bytes={} shared_images={} encrypted_images={} resource_ms={:.3} plan_ms={:.3} rss_start_kib={} rss_estimate_kib={} rss_plan_kib={} rss_peak_kib={}",
        name,estimate.metadata_sectors,image_count,estimate.image_payload_bytes,estimate.working_payload_bytes,
        payload_count,shared_count,crypto_count,resource_ms,plan_ms,start_rss,estimate_rss,plan_rss,after);
}
