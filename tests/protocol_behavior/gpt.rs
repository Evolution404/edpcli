use edpcli::protocol::{lba1::*, lba2::*, profile::GptLayout};
fn fixture(text: &str) -> [u8; 512] {
    let h: String = text.split_whitespace().collect();
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}
#[test]
pub fn gpt_virtual_header_and_entries_are_typed_and_crc_checked() {
    let raw1 = fixture(include_str!(
        "../fixtures/protocol_evidence/official_virtual_gpt_lba1.hex"
    ));
    let mut raw2 = fixture(include_str!(
        "../fixtures/protocol_evidence/official_virtual_gpt_lba2.hex"
    ));
    let view = parse_lba1(&raw1).unwrap();
    let GptHeaderState::Enabled(header) = &view.header else {
        panic!("expected virtual GPT")
    };
    assert_eq!(header.revision, 0x10000);
    assert_eq!(header.header_size, 92);
    assert_eq!(header.current_lba, 1);
    assert_eq!(header.backup_lba, 4194303);
    assert_eq!(
        (header.first_usable_lba, header.last_usable_lba),
        (34, 4194270)
    );
    assert_eq!(
        (
            header.partition_entries_lba,
            header.entry_count,
            header.entry_size
        ),
        (2, 128, 128)
    );
    assert_eq!(
        header.disk_guid,
        [
            0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26,
            0x99, 0xc7
        ]
    );
    assert_eq!(view.reconstruct(), raw1);
    let mut full_array = vec![0; 128 * 128];
    full_array[..512].copy_from_slice(&raw2);
    assert!(header.validate_partition_array(&full_array).is_ok());
    assert!(header.validate_partition_array(&raw2).is_err()); // 512B cannot prove a 16KiB array CRC.
    full_array[1000] = 1;
    assert!(header.validate_partition_array(&full_array).is_err());
    // Consumer ignores all 112 residual bytes of each unused entry.
    for base in [128, 256, 384] {
        raw2[base + 16..base + 128].fill(0xa5);
    }
    let entries = parse_lba2(&raw2, GptLayout::Enabled).unwrap();
    let GptEntry::Used(part) = &entries.entries.as_ref().unwrap()[0] else {
        panic!()
    };
    assert_eq!(
        (part.first_lba, part.last_lba, part.attributes),
        (63, 4194270, 0)
    );
    assert_eq!(
        part.unique_guid,
        [
            0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
            0xff
        ]
    );
    assert_eq!(part.name_utf16, [0; 36]);
    for e in &entries.entries.as_ref().unwrap()[1..] {
        let GptEntry::Unused { residual } = e else {
            panic!()
        };
        assert_eq!(residual.bytes(), &[0xa5; 112]);
    }
    assert_eq!(entries.reconstruct(), raw2);
    for offset in [8, 12, 16, 24, 55, 91] {
        let mut broken = raw1;
        broken[offset] ^= 0x80;
        assert!(parse_lba1(&broken).is_err(), "corrupt header byte {offset}");
    }
}
#[test]
pub fn gpt_absent_is_explicit_and_unknown_never_defaults_to_absent() {
    let zero = [0; 512];
    assert!(matches!(
        parse_lba1(&zero).unwrap().header,
        GptHeaderState::Absent
    ));
    assert_eq!(parse_lba1(&zero).unwrap().reconstruct(), zero);
    assert!(parse_lba2(&zero, GptLayout::Absent)
        .unwrap()
        .entries
        .is_none());
    assert!(parse_lba2(&zero, GptLayout::Unknown).is_err());
    assert!(parse_lba1(&[0xa5; 512]).is_err());
    assert!(parse_lba2(&[0xa5; 512], GptLayout::Absent).is_err());
}
