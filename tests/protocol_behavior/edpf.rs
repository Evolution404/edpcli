use edpcli::{
    crypto::{crc32_bare, xor_rolling},
    diskio::parse_backup_name,
    protocol::{
        edpf::PassInfo,
        lba7::{parse_lba7, Entry2},
        profile::{Lba7EntryCount, Lba7PassinfoVersion},
    },
};

#[test]
pub fn lba7_packed_entries_and_pass_info_replay_all_physical_profiles() {
    let mut entry_counts = [0usize; 2];
    let mut versions = [0usize; 2];
    for row in include_str!("../../audit/protocol/gold_samples.tsv")
        .lines()
        .skip(1)
    {
        let c: Vec<_> = row.split('\t').collect();
        let image =
            std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(c[4])).unwrap();
        let raw: &[u8; 512] = image[7 * 512..8 * 512].try_into().unwrap();
        let device_id = if c[0] == "authentic-nopwd" {
            "disk&ven_sandisk&prod_ultra&rev_1.00".to_string()
        } else {
            parse_backup_name(c[3]).unwrap().device_id
        };
        let crc = crc32_bare(device_id.as_bytes());
        let plain = xor_rolling(raw, (crc & 0xffff) ^ (crc >> 16));
        let count = u32::from_le_bytes(plain[8..12].try_into().unwrap());
        let pass_stored: [u8; 14] = plain[0xc0..0xce].try_into().unwrap();
        let pass = PassInfo::decode_stored(&pass_stored);
        assert_eq!(pass.encode_stored(), pass_stored);
        let count_profile = match count {
            2 => {
                entry_counts[0] += 1;
                Lba7EntryCount::TwoEntry
            }
            3 => {
                entry_counts[1] += 1;
                Lba7EntryCount::ThreeEntry
            }
            other => panic!("unexpected LBA7 entry count {other}"),
        };
        let version_profile = match pass.version {
            0x0064 => {
                versions[0] += 1;
                Lba7PassinfoVersion::LegacyV0064
            }
            0x0206 => {
                versions[1] += 1;
                Lba7PassinfoVersion::CurrentV0206
            }
            other => panic!("unexpected LBA7 pass-info version 0x{other:04x}"),
        };
        let view = parse_lba7(raw, crc, count_profile, version_profile).unwrap();
        assert_eq!(view.reconstruct(), *raw);
        assert_eq!(view.reencode(), *raw);
        assert_eq!(view.pass_info, pass);
        assert_eq!(view.entries_0_1[0].partition_count, count);
        assert_eq!(view.entries_0_1[1].partition_count, count);
        match (&view.entry2, count) {
            (Entry2::Absent, 2) => {}
            (Entry2::Present(entry), 3) => assert_eq!(entry.partition_count, 3),
            other => panic!("entry2 profile mismatch: {other:?}"),
        }
        assert!(view.post_table_zero.iter().all(|byte| *byte == 0));
    }
    assert!(entry_counts.iter().all(|count| *count > 0));
    assert!(versions.iter().all(|count| *count > 0));
}

#[test]
pub fn pass_info_storage_transform_is_exact_and_profile_axes_fail_closed() {
    let pass = PassInfo {
        version: 0x0206,
        force_change_share: 1,
        max_share_password_errors: 5,
        current_share_password_errors: 2,
        force_change_encrypt: 1,
        max_encrypt_password_errors: 6,
        current_encrypt_password_errors: 3,
        no_password_set: 1,
        no_password_no_check_ip: 1,
        no_usb_check_password_safe: 1,
        reset_file_key: 1,
        share_backup_prompt_period: 7,
        encrypt_backup_prompt_period: 9,
    };
    let stored = pass.encode_stored();
    assert_eq!(stored[0], 0x06 ^ 0x88);
    assert_eq!(stored[3], 5 ^ 0x88);
    assert_eq!(stored[6], 6 ^ 0x88);
    assert_eq!(PassInfo::decode_stored(&stored), pass);
}
