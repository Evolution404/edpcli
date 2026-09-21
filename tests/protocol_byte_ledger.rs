//! Machine checks for the profile-aware LBA0-LBA12 byte ledger.

use std::collections::{HashMap, HashSet};

const LEDGER: &str = include_str!("../audit/protocol/byte_ledger.tsv");
const EVIDENCE: &str = include_str!("../audit/protocol/evidence_manifest.tsv");
const DOC: &str = include_str!("../docs/EDP_PROTOCOL_REVERSE_ENGINEERING.md");

fn parse_hex(value: &str) -> usize {
    usize::from_str_radix(value, 16).expect("hex ledger offset")
}

fn offsets(spec: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for piece in spec.split(',') {
        let (start, end) = match piece.split_once('-') {
            Some((start, end)) => (parse_hex(start), parse_hex(end)),
            None => {
                let value = parse_hex(piece);
                (value, value)
            }
        };
        assert!(start <= end && end < 512, "bad ledger range: {piece}");
        out.extend(start..=end);
    }
    out
}

fn evidence_modalities() -> HashMap<&'static str, &'static str> {
    let mut out = HashMap::new();
    for line in EVIDENCE
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
    {
        let cols: Vec<_> = line.split('\t').collect();
        assert_eq!(cols.len(), 9, "bad evidence manifest row: {line}");
        assert!(
            out.insert(cols[0], cols[1]).is_none(),
            "duplicate evidence id"
        );
    }
    out
}

#[test]
fn byte_ledger_covers_exactly_6656_bytes_without_overlap() {
    let evidence = evidence_modalities();
    let mut seen = vec![false; 13 * 512];
    let mut complete = [0usize; 13];
    let mut partial = [0usize; 13];

    for line in LEDGER
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
    {
        let cols: Vec<_> = line.split('\t').collect();
        assert_eq!(cols.len(), 9, "bad byte-ledger row: {line}");
        let lba: usize = cols[0].parse().expect("numeric LBA");
        assert!(lba < 13, "unexpected LBA: {lba}");
        let status = cols[2];
        assert!(matches!(status, "COMPLETE" | "PARTIAL" | "UNKNOWN"));
        assert!(
            !cols[4].is_empty(),
            "every ledger row needs profile tags: {line}"
        );

        let producer_ids: Vec<_> = cols[5].split(';').filter(|id| !id.is_empty()).collect();
        let consumer_ids: Vec<_> = cols[6].split(';').filter(|id| !id.is_empty()).collect();
        let physical_ids: Vec<_> = cols[7].split(';').filter(|id| !id.is_empty()).collect();
        for id in producer_ids
            .iter()
            .chain(consumer_ids.iter())
            .chain(physical_ids.iter())
        {
            assert!(
                evidence.contains_key(id),
                "unknown evidence id {id}: {line}"
            );
        }
        if status == "COMPLETE" {
            assert!(
                !producer_ids.is_empty(),
                "COMPLETE lacks producer evidence: {line}"
            );
            assert!(
                !consumer_ids.is_empty(),
                "COMPLETE lacks consumer evidence: {line}"
            );
            assert!(
                !physical_ids.is_empty(),
                "COMPLETE lacks physical evidence: {line}"
            );
            assert!(
                physical_ids.iter().all(|id| evidence[id] == "physical"),
                "COMPLETE physical evidence must point to physical captures: {line}"
            );
            assert!(
                producer_ids
                    .iter()
                    .any(|id| matches!(evidence[id], "static" | "virtual")),
                "COMPLETE producer evidence must identify static or official virtual proof: {line}"
            );
        }

        for offset in offsets(cols[1]) {
            let index = lba * 512 + offset;
            assert!(!seen[index], "overlap at LBA{lba}+0x{offset:03x}");
            seen[index] = true;
            match status {
                "COMPLETE" => complete[lba] += 1,
                "PARTIAL" => partial[lba] += 1,
                _ => {}
            }
        }
    }

    assert!(
        seen.into_iter().all(|value| value),
        "byte ledger contains gaps"
    );
    let expected_complete = [
        142, 512, 512, 0, 487, 512, 497, 512, 492, 384, 384, 512, 512,
    ];
    let expected_partial = [370, 0, 0, 512, 25, 0, 15, 0, 20, 128, 128, 0, 0];
    assert_eq!(complete, expected_complete);
    assert_eq!(partial, expected_partial);
    assert_eq!(complete.iter().sum::<usize>(), 5458);
    assert_eq!(partial.iter().sum::<usize>(), 1198);

    for lba in 0..13 {
        let progress = format!("| LBA{lba} | {} | {} | 0 |", complete[lba], partial[lba]);
        assert!(
            DOC.contains(&progress),
            "canonical strict-progress table diverged from byte ledger: {progress}"
        );
    }
}

#[test]
fn historical_rejections_and_lba10_sample_gate_are_explicit() {
    let matrix = include_str!("../audit/protocol/historical_matrix.tsv");
    assert!(matrix.contains("rejected for exact legacy profile"));
    assert!(matrix.contains("join59"));
    assert!(matrix.contains("strict nonzero HSerialCRC"));
    assert!(matrix.contains("dynamic_MBR_template"));
    assert!(matrix.contains("2019-11-12"));
    assert!(matrix.contains("earlier complete paired-set locator only"));
    assert!(matrix.contains("UsbOnlyInfo=0"));
    assert!(matrix.contains("cemssafeudisklabeltool.exe local variants"));

    let lba10 = LEDGER
        .lines()
        .find(|line| line.starts_with("10\t000-07f\tPARTIAL\t"))
        .expect("LBA10 active EESI row");
    assert!(lba10.contains("all designated gold is zero"));
    assert!(lba10.contains("allowed authentic enabled sample"));
}

#[test]
fn local_labeltool_patches_cannot_be_mistaken_for_official_protocol_evidence() {
    let note = include_str!("../audit/protocol/labeltool_variant_diff.md");
    for required in [
        "b530a82b29bbc43be8d415225392ca135ab7df8a8d9f69c6598493c4942e9e11",
        "0x00449B76",
        "0x0042DDC0",
        "0x004289FB",
        "must not be cited as official producer evidence",
        "CreateBusManageImp",
        "BusManageImp::WriteLabel@0x100A28E0",
        "0x996 = 2454",
    ] {
        assert!(
            note.contains(required),
            "label-tool integrity boundary lost evidence: {required}"
        );
    }
}

#[test]
fn evidence_modalities_remain_distinct() {
    let modalities: HashSet<_> = evidence_modalities().into_values().collect();
    assert!(modalities.contains("physical"));
    assert!(modalities.contains("virtual"));
    assert!(modalities.contains("static"));
}

#[test]
fn lba3_manufacturing_gate_keeps_fw_marker_page_distinct_from_host_lba3() {
    let note = include_str!("../audit/protocol/lba3_identity.md");
    for required in [
        "file_size - 0x200",
        "CBaseController::virtual_464",
        "marker-page readers",
        "0x459C7EB5",
        "0x22A482A8",
        "Local capture inventory is insufficient to lock PS2307 versus PS2309",
        "do **not** contain USB serial",
    ] {
        assert!(
            note.contains(required),
            "LBA3 identity/manufacturing gate lost evidence boundary: {required}"
        );
    }
}
