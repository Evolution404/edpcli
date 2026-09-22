use std::collections::HashMap;

use edpcli::sha256::sha256_hex;

const EVIDENCE: &str = include_str!("../audit/region_a/evidence_manifest.tsv");
const WIRE: &str = include_str!("../audit/region_a/wire_byte_ledger.tsv");
const PLAIN: &str = include_str!("../audit/region_a/plain_byte_ledger.tsv");
const DOC: &str = include_str!("../docs/REGION_A_REVERSE_ENGINEERING.md");
const PHYSICAL: &[u8] = include_bytes!("../audit/region_a/gold/lexar_region_a_lba243623933.bin");

fn parse_hex(value: &str) -> usize {
    usize::from_str_radix(value, 16).expect("hex Region A offset")
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
        assert!(
            start <= end && end < limit,
            "bad Region A ledger range: {piece}"
        );
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
        assert_eq!(cols.len(), 9, "bad evidence row: {line}");
        assert!(matches!(
            cols[1],
            "physical" | "static" | "virtual" | "negative"
        ));
        assert!(
            out.insert(cols[0], cols[1]).is_none(),
            "duplicate evidence id"
        );
    }
    out
}

#[test]
fn region_a_wire_ledger_covers_exactly_3072_bytes_without_overlap() {
    let evidence = evidence_modalities();
    let mut seen = vec![false; 0xC00];
    let mut complete = 0usize;
    let mut partial = 0usize;
    let mut unknown = 0usize;

    for line in WIRE.lines().skip(1).filter(|line| !line.trim().is_empty()) {
        let cols: Vec<_> = line.split('\t').collect();
        assert_eq!(cols.len(), 8, "bad wire ledger row: {line}");
        let status = cols[1];
        assert!(matches!(status, "COMPLETE" | "PARTIAL" | "UNKNOWN"));
        let producer: Vec<_> = cols[4].split(';').filter(|id| !id.is_empty()).collect();
        let consumer: Vec<_> = cols[5].split(';').filter(|id| !id.is_empty()).collect();
        let physical: Vec<_> = cols[6].split(';').filter(|id| !id.is_empty()).collect();
        for id in producer
            .iter()
            .chain(consumer.iter())
            .chain(physical.iter())
        {
            assert!(
                evidence.contains_key(id),
                "unknown evidence id {id}: {line}"
            );
        }
        if status == "COMPLETE" {
            assert!(!producer.is_empty() && !consumer.is_empty() && !physical.is_empty());
            assert!(physical.iter().any(|id| evidence[id] == "physical"));
        }
        if status == "PARTIAL" {
            assert!(!producer.is_empty() && !consumer.is_empty() && !physical.is_empty());
        }
        for offset in offsets(cols[0], 0xC00) {
            assert!(!seen[offset], "wire overlap at +0x{offset:03x}");
            seen[offset] = true;
            match status {
                "COMPLETE" => complete += 1,
                "PARTIAL" => partial += 1,
                "UNKNOWN" => unknown += 1,
                _ => unreachable!(),
            }
        }
    }
    assert!(
        seen.into_iter().all(|value| value),
        "wire ledger contains gaps"
    );
    assert_eq!((complete, partial, unknown), (0, 2048, 1024));
    assert!(DOC.contains("| Region A +0x000..+0xbff | 0 | 2048 | 1024 |"));
}

#[test]
fn iir_plain_ledger_covers_exactly_2048_bytes_and_never_claims_physical_completion() {
    let evidence = evidence_modalities();
    let mut seen = vec![false; 0x800];
    let mut complete = 0usize;
    let mut partial = 0usize;
    let mut unknown = 0usize;

    for line in PLAIN.lines().skip(1).filter(|line| !line.trim().is_empty()) {
        let cols: Vec<_> = line.split('\t').collect();
        assert_eq!(cols.len(), 7, "bad plaintext ledger row: {line}");
        let status = cols[1];
        assert!(matches!(status, "COMPLETE" | "PARTIAL" | "UNKNOWN"));
        let producer: Vec<_> = cols[3].split(';').filter(|id| !id.is_empty()).collect();
        let consumer: Vec<_> = cols[4].split(';').filter(|id| !id.is_empty()).collect();
        let physical_plain: Vec<_> = cols[5].split(';').filter(|id| !id.is_empty()).collect();
        for id in producer
            .iter()
            .chain(consumer.iter())
            .chain(physical_plain.iter())
        {
            assert!(
                evidence.contains_key(id),
                "unknown evidence id {id}: {line}"
            );
        }
        if status == "COMPLETE" {
            assert!(!producer.is_empty() && !consumer.is_empty() && !physical_plain.is_empty());
            assert!(physical_plain.iter().any(|id| evidence[id] == "physical"));
        }
        if status == "PARTIAL" {
            assert!(!producer.is_empty() && !consumer.is_empty());
            assert!(
                physical_plain.is_empty(),
                "real plaintext must remain unclaimed: {line}"
            );
            assert!(cols[6].contains("MISSING_PHYSICAL_PLAINTEXT"));
        }
        for offset in offsets(cols[0], 0x800) {
            assert!(!seen[offset], "plaintext overlap at +0x{offset:03x}");
            seen[offset] = true;
            match status {
                "COMPLETE" => complete += 1,
                "PARTIAL" => partial += 1,
                "UNKNOWN" => unknown += 1,
                _ => unreachable!(),
            }
        }
    }
    assert!(
        seen.into_iter().all(|value| value),
        "plaintext ledger contains gaps"
    );
    assert_eq!((complete, partial, unknown), (0, 1052, 996));
    assert!(DOC.contains("| IIR plaintext +0x000..+0x7ff | 0 | 1052 | 996 |"));
}

#[test]
fn physical_lexar_region_a_fixture_is_exact_and_stable() {
    assert_eq!(PHYSICAL.len(), 0xC00);
    assert_eq!(
        sha256_hex(PHYSICAL),
        "fbf45d4713664d1e68eca634e8e9c04565be8b60d9ad4e1a18b7df3c766aaa24"
    );
    assert!(PHYSICAL.iter().filter(|&&byte| byte != 0).count() > 3000);
}

#[test]
fn reproduced_official_key_candidate_is_explicitly_rejected_by_iir_crcs() {
    let derive: serde_json::Value = serde_json::from_str(include_str!(
        "../audit/region_a/evidence/derive_iir_key_official_20260922.json"
    ))
    .unwrap();
    assert_eq!(derive["ret"], 1);
    assert_eq!(
        derive["output_0x20"],
        "8eeaa2062efa0b371076ebe510d98f1800000000000000000000000000000000"
    );

    let decrypt: serde_json::Value = serde_json::from_str(include_str!(
        "../audit/region_a/evidence/decrypt_candidate_20260922.json"
    ))
    .unwrap();
    let evp = decrypt["trace"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["hook"] == "EVP_CipherInit_ex(sub_18001fe70)")
        .expect("EVP init trace");
    assert_eq!(evp["cipher"], "0x1801cd190");
    assert_eq!(evp["iv_16_at_arg4"], "ZERO(NULL)");

    let crc: serde_json::Value = serde_json::from_str(include_str!(
        "../audit/region_a/evidence/decrypt_candidate_crc_20260922.json"
    ))
    .unwrap();
    assert_eq!(crc["main_crc"]["ok"], false);
    assert_eq!(crc["all_crc_ok"], false);
    assert_eq!(
        crc["segment_crcs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["ok"] == true)
            .count(),
        0
    );
}

#[test]
fn failed_init_iir_harness_is_negative_evidence_not_virtual_positive() {
    let run: serde_json::Value = serde_json::from_str(include_str!(
        "../audit/region_a/evidence/init_iir_runner_failure_20260922.json"
    ))
    .unwrap();
    assert_eq!(run["write_dev_called"], false);
    assert_eq!(
        run["generated_ct_first32"],
        "0000000000000000000000000000000000000000000000000000000000000000"
    );
    assert_eq!(run["parsed"]["all_crc_ok"], false);

    let profile = include_str!("../audit/region_a/profile_coverage.tsv");
    assert!(profile.contains("NOT_REPRODUCED"));
    assert!(profile.contains("N-IIR-INIT-RUNNER"));
}
