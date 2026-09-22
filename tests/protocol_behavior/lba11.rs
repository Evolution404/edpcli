use edpcli::{
    diskio::parse_backup_name,
    protocol::{lba11::parse_lba11, profile::Lba11Capacity},
};

#[test]
pub fn lba11_disk_size_and_repair_chs_profiles_replay_physical_gold() {
    let mut disk_size_count = 0usize;
    let mut chs_count = 0usize;
    let mut checked = 0usize;
    for row in include_str!("../../audit/protocol/gold_samples.tsv")
        .lines()
        .skip(1)
    {
        let c: Vec<_> = row.split('\t').collect();
        if c[0] != "strict-encrypted" {
            continue;
        }
        let meta = parse_backup_name(c[3]).unwrap();
        let sectors = meta.secs.unwrap();
        let image =
            std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(c[4])).unwrap();
        let raw: &[u8; 512] = image[11 * 512..12 * 512].try_into().unwrap();
        let disk_size = sectors * 512;
        let (profile, view) = match parse_lba11(
            raw,
            &meta.vid,
            &meta.pid,
            disk_size,
            Lba11Capacity::DiskSize,
        ) {
            Ok(view) => {
                disk_size_count += 1;
                (Lba11Capacity::DiskSize, view)
            }
            Err(_) => {
                let view = parse_lba11(
                    raw,
                    &meta.vid,
                    &meta.pid,
                    disk_size,
                    Lba11Capacity::RepairChs,
                )
                .unwrap();
                chs_count += 1;
                (Lba11Capacity::RepairChs, view)
            }
        };
        assert_eq!(view.profile, profile);
        assert_eq!(view.uid.value(), meta.device_id.as_bytes());
        assert!(view.uid.backing().iter().all(|byte| *byte == 0));
        assert!(view.random252.iter().all(|byte| *byte != 0xff));
        assert_eq!(view.reconstruct(), *raw);
        assert_eq!(view.reencode(), *raw);
        checked += 1;
    }
    assert!(checked > 0);
    assert!(disk_size_count > 0);
    assert!(chs_count > 0);
}

#[test]
pub fn lba11_capacity_axis_is_explicit_and_fail_closed() {
    let zero = [0u8; 512];
    assert!(parse_lba11(&zero, "1234", "5678", 1, Lba11Capacity::Unknown).is_err());
    assert!(parse_lba11(&zero, "12", "5678", 1, Lba11Capacity::DiskSize).is_err());
}
