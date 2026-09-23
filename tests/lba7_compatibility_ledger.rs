use std::{collections::HashMap, fs, path::Path};

use edpcli::{backup_metadata::parse_lba7_compatibility_geometry, sha256::sha256_hex};

const EVIDENCE: &str = include_str!("../audit/protocol/lba7_compatibility/evidence_manifest.tsv");
const LEDGER: &str = include_str!("../audit/protocol/lba7_compatibility/byte_ledger.tsv");
const DOC: &str = include_str!("../docs/protocol/LCE.md");
const PRODUCER: &str = include_str!(
    "../audit/protocol/lba7_compatibility/evidence/official_lba7_producer_20260923.json"
);
const LEXAR_IMAGE: &[u8] = include_bytes!(
    "../audit/protocol/gold/strict-encrypted/disk4_243625984_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid3164177653_20260827_221910.bin"
);
const SANDISK_LBA7: &[u8] = include_bytes!(
    "../audit/protocol/lba7_compatibility/live_captures/sandisk_nopwd_20260923/lba7_raw.bin"
);
const SANDISK_COMPAT: &[u8] = include_bytes!(
    "../audit/protocol/lba7_compatibility/live_captures/sandisk_nopwd_20260923/lba7_compat_extent.bin"
);

fn parse_hex(value: &str) -> usize {
    usize::from_str_radix(value, 16).expect("hex compatibility-extent offset")
}

fn offsets(spec: &str, limit: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for piece in spec.split(',') {
        let (start, end) = match piece.split_once('-') {
            Some((start, end)) => (parse_hex(start), parse_hex(end)),
            None => {
                let value = parse_hex(piece);
                (value, value)
            }
        };
        assert!(start <= end && end < limit, "bad ledger range: {piece}");
        out.extend(start..=end);
    }
    out
}

fn evidence_rows() -> HashMap<&'static str, Vec<&'static str>> {
    EVIDENCE
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let cols = line.split('\t').collect::<Vec<_>>();
            assert_eq!(cols.len(), 9, "bad evidence row: {line}");
            (cols[0], cols)
        })
        .collect()
}

#[test]
fn evidence_manifest_hashes_every_current_artifact() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for cols in evidence_rows().values() {
        assert!(matches!(
            cols[1],
            "physical" | "static" | "static+provenance" | "virtual" | "negative"
        ));
        let path = root.join(cols[5]);
        let bytes =
            fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert_eq!(
            sha256_hex(&bytes),
            cols[4],
            "hash mismatch: {}",
            path.display()
        );
    }
}

#[test]
fn compatibility_byte_ledger_covers_exactly_0xc00_without_overlap() {
    let evidence = evidence_rows();
    let mut seen = vec![false; 0xC00];
    let mut complete = 0usize;

    for line in LEDGER
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
    {
        let cols = line.split('\t').collect::<Vec<_>>();
        assert_eq!(cols.len(), 8, "bad byte-ledger row: {line}");
        assert_eq!(cols[1], "COMPLETE");
        for ids in [&cols[4], &cols[5], &cols[6]] {
            for id in ids.split(';').filter(|id| !id.is_empty()) {
                assert!(evidence.contains_key(id), "unknown evidence id {id}");
            }
        }
        for offset in offsets(cols[0], 0xC00) {
            assert!(!seen[offset], "overlap at +0x{offset:03x}");
            seen[offset] = true;
            complete += 1;
        }
    }

    assert_eq!(complete, 0xC00);
    assert!(seen.into_iter().all(|value| value));
}

#[test]
fn official_producer_evidence_locks_mode_matrix_and_type_semantics() {
    let evidence: serde_json::Value = serde_json::from_str(PRODUCER).unwrap();
    let modes = evidence["official_mode_matrix"].as_array().unwrap();
    let actual = modes
        .iter()
        .map(|mode| {
            (
                mode["mode"].as_u64().unwrap(),
                mode["official_text"].as_str().unwrap(),
                mode["partion_types"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_u64().unwrap())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            (0, "缺省三分区", vec![1, 2, 4]),
            (1, "启动区和交换区二合一", vec![2, 4]),
            (2, "整盘加密", vec![1, 4]),
            (3, "内外网通用双分区", vec![1, 2]),
        ]
    );
    assert_eq!(
        evidence["partion_type_semantics"]["mapping"]["1"],
        "boot / m_bHasBootPart"
    );
    assert_eq!(
        evidence["partion_type_semantics"]["mapping"]["2"],
        "share-exchange / m_bHasSharePart"
    );
    assert_eq!(
        evidence["partion_type_semantics"]["mapping"]["4"],
        "encrypt-private / m_bHasEncryptPart"
    );
    assert!(evidence["lba7_compatibility_extent"]["consequence"]
        .as_str()
        .unwrap()
        .contains("not evidence that type2 aliases type4"));
}

#[test]
fn lexar_mode0_preserves_distinct_type2_and_type4_on_one_compatibility_extent() {
    let geometry = parse_lba7_compatibility_geometry(
        LEXAR_IMAGE,
        "disk&ven_lexar&prod_usb_flash_drive",
        243_625_984,
    )
    .unwrap();
    assert_eq!(geometry.start_lba, 243_623_933);
    assert_eq!(geometry.sector_count, 6);
    assert_eq!(
        geometry.official_partition_mode.as_deref(),
        Some("0 (缺省三分区)")
    );
    assert_eq!(geometry.lba7_pointer_entries.len(), 2);
    assert_eq!(geometry.lba7_pointer_entries[0].entry_index, 1);
    assert_eq!(geometry.lba7_pointer_entries[0].partition_type, 2);
    assert_eq!(
        geometry.lba7_pointer_entries[0].partition_role.as_deref(),
        Some("share")
    );
    assert_eq!(geometry.lba7_pointer_entries[1].entry_index, 2);
    assert_eq!(geometry.lba7_pointer_entries[1].partition_type, 4);
    assert_eq!(
        geometry.lba7_pointer_entries[1].partition_role.as_deref(),
        Some("encrypt")
    );
}

#[test]
fn live_sandisk_two_entry_pointer_and_payload_are_preserved() {
    let mut front = vec![0u8; 13 * 512];
    front[7 * 512..8 * 512].copy_from_slice(SANDISK_LBA7);
    let geometry = parse_lba7_compatibility_geometry(
        &front,
        "disk&ven_sandisk&prod_ultra&rev_1.00",
        120_176_640,
    )
    .unwrap();
    assert_eq!(geometry.start_lba, 120_164_408);
    assert_eq!(geometry.chs_expected_start_lba, Some(120_164_408));
    assert_eq!(geometry.lba7_pointer_entries.len(), 1);
    assert_eq!(geometry.lba7_pointer_entries[0].entry_index, 1);
    assert_eq!(geometry.lba7_pointer_entries[0].partition_type, 4);
    assert_eq!(SANDISK_COMPAT.len(), 0xC00);
    assert_eq!(
        sha256_hex(SANDISK_COMPAT),
        "aaeffbba440e553c2eb47accac54552c9b53af9d4951f728ac700a2e81aca0a0"
    );
}

#[test]
fn final_document_rejects_the_old_alias_model() {
    assert!(DOC.contains("type2 is not a type4 alias"));
    assert!(DOC.contains("fixed physical extent is **not type4-specific**"));
    assert!(DOC.contains("IIR remains a separate module and evidence chain"));
    assert!(!DOC.contains("# Region A"));
}
