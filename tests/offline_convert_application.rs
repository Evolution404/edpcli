mod common;

use std::fs;

use common::{fixture, golden, load_disk_image, sha256, TmpDir};
use edpcli::application::offline_convert::{run, OfflineConvertRequest};
use edpcli::common::SECTOR;

#[test]
fn offline_convert_service_matches_protocol_gold_and_writes_expected_outputs() {
    let Some(image) = load_disk_image("netac") else {
        return;
    };
    let Some((_, device_id)) = fixture("netac") else {
        return;
    };
    let source = TmpDir::new("offline_convert_source");
    for lba in [0usize, 6, 7, 9, 12] {
        fs::write(
            source.0.join(format!("LBA{lba:02}.bin")),
            &image[lba * SECTOR..(lba + 1) * SECTOR],
        )
        .unwrap();
    }

    let preview = run(&OfflineConvertRequest {
        source_dir: source.0.clone(),
        device_id: device_id.to_string(),
        size_gb: None,
        output_dir: None,
    })
    .expect("offline preview");
    let expected = golden("netac");
    assert_eq!(preview.result.share, expected.share);
    assert_eq!(preview.result.enc_start, expected.enc_start);
    assert_eq!(preview.result.enc_size, expected.enc_size);
    assert_eq!(preview.result.crc, expected.crc);
    assert_eq!(preview.result.k0, expected.k0);
    assert_eq!(sha256(&preview.result.lba0), expected.lba0);
    assert_eq!(sha256(&preview.result.lba6), expected.lba6);
    assert_eq!(sha256(&preview.result.lba7), expected.lba7);
    assert_eq!(sha256(&preview.result.lba12), expected.lba12);
    assert_eq!(preview.reports.len(), 3);
    assert!(preview.output_dir.is_none());

    let output = TmpDir::new("offline_convert_output");
    let written = run(&OfflineConvertRequest {
        source_dir: source.0.clone(),
        device_id: device_id.to_string(),
        size_gb: None,
        output_dir: Some(output.0.clone()),
    })
    .expect("offline write");
    assert_eq!(written.output_dir.as_deref(), Some(output.0.as_path()));
    assert_eq!(
        sha256(&fs::read(output.0.join("LBA00.bin")).unwrap()),
        expected.lba0
    );
    assert_eq!(
        sha256(&fs::read(output.0.join("LBA06.bin")).unwrap()),
        expected.lba6
    );
    assert_eq!(
        sha256(&fs::read(output.0.join("LBA07.bin")).unwrap()),
        expected.lba7
    );
    assert_eq!(
        sha256(&fs::read(output.0.join("LBA12.bin")).unwrap()),
        expected.lba12
    );
    assert_eq!(
        output.0.join("LBA09.bin").exists(),
        !expected.lba9_none,
        "LBA9 output presence must follow the conversion plan"
    );
}
