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
fn region_a_locator_evidence_closes_three_brand_chs_formula_without_closing_iir_binding() {
    let locator: serde_json::Value = serde_json::from_str(include_str!(
        "../audit/region_a/evidence/region_a_locator_algorithm_20260922.json"
    ))
    .unwrap();

    assert_eq!(locator["status"], "COMPLETE");
    assert_eq!(
        locator["formula"]["for_512_byte_sectors"],
        "RegionA_LBA = CHS_sectors - 0x700 = CHS_sectors - 1792"
    );

    let samples = locator["physical_cross_brand"].as_array().unwrap();
    assert_eq!(samples.len(), 3);
    for sample in samples {
        assert_eq!(
            sample["formula_region_a_lba"],
            sample["decoded_lba7_entry1_start_sector"]
        );
        assert_eq!(
            sample["formula_region_a_lba"],
            sample["decoded_lba7_entry2_start_sector"]
        );
        assert_eq!(sample["decoded_lba7_region_size_bytes"], 0xC00);
        assert_eq!(sample["prev_512_all_zero"], true);
        assert_eq!(sample["next_512_all_zero"], true);
    }

    let evidence = evidence_modalities();
    assert_eq!(evidence["S-REGIONA-LOCATOR"], "static");
    assert_eq!(evidence["P-REGIONA-LOCATOR-3DISK"], "physical");
    assert!(DOC.contains("Region A 物理定位算法已 COMPLETE"));
    assert!(DOC.contains("sectormanage64::ReadIIR"));
    assert!(DOC.contains("保持 PARTIAL"));
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

#[test]
fn region_a_cipher_descriptor_is_aes256_cbc_and_matches_standard_crypto() {
    let descriptor: serde_json::Value = serde_json::from_str(include_str!(
        "../audit/region_a/evidence/cipher_descriptor_aes256_20260922.json"
    ))
    .unwrap();
    assert_eq!(descriptor["descriptor_va"], "0x1801cd190");
    assert_eq!(descriptor["nid"], 427);
    assert_eq!(descriptor["block_size"], 16);
    assert_eq!(descriptor["key_len"], 32);
    assert_eq!(descriptor["iv_len"], 16);
    assert_eq!(descriptor["classification"], "AES-256-CBC");
    assert_eq!(descriptor["openssl_matches_official_prefix"], true);
    assert_eq!(
        descriptor["official_prefix_sha256"],
        descriptor["openssl_prefix_sha256"]
    );
    assert!(!DOC.contains("Region A 主 IIR wrapper 实际使用 **AES-192-CBC**"));
    assert!(DOC.contains("Region A 主 IIR wrapper 实际使用 **AES-256-CBC**"));
}

#[test]
fn init_final_key_profiles_are_closed_but_both_fail_physical_lexar_integrity() {
    let derivation: serde_json::Value = serde_json::from_str(include_str!(
        "../audit/region_a/evidence/init_key_derivation_20260922.json"
    ))
    .unwrap();
    assert_eq!(derivation["input_len"], 32);
    assert_eq!(
        derivation["input_hex"],
        "767276446c6c00383048335437333457474e444d4b5059504d38304559583100"
    );
    assert_eq!(
        derivation["md5_digest_hex"],
        "8eeaa2062efa0b371076ebe510d98f18"
    );
    assert_eq!(
        derivation["final_core_key_ascii"],
        "8eeaa2062efa0b371076ebe510d98f18"
    );

    let profiles: serde_json::Value = serde_json::from_str(include_str!(
        "../audit/region_a/evidence/init_key_profiles_20260922.json"
    ))
    .unwrap();
    let profiles = profiles["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 3);
    assert_eq!(profiles[0]["formatter"], "%02X");
    assert_eq!(
        profiles[0]["final_key_ascii"],
        "8EEAA2062EFA0B371076EBE510D98F18"
    );
    assert_eq!(profiles[1]["formatter"], "%02x");
    assert_eq!(profiles[2]["formatter"], "%02x");

    let lower: serde_json::Value = serde_json::from_str(include_str!(
        "../audit/region_a/evidence/decrypt_init_key_candidate_crc_20260922.json"
    ))
    .unwrap();
    assert_eq!(lower["main_crc"]["ok"], false);
    assert_eq!(lower["all_crc_ok"], false);
    assert_eq!(
        lower["segment_crcs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["ok"] == true)
            .count(),
        0
    );

    let upper: serde_json::Value = serde_json::from_str(include_str!(
        "../audit/region_a/evidence/decrypt_uppercase_key_candidate_crc_20260922.json"
    ))
    .unwrap();
    assert_eq!(upper["key_ascii"], "8EEAA2062EFA0B371076EBE510D98F18");
    assert_eq!(upper["main_crc"]["ok"], false);
    assert_eq!(upper["all_crc_ok"], false);
    assert_eq!(
        upper["segment_crcs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["ok"] == true)
            .count(),
        0
    );
}

#[test]
fn iir_address_chain_is_static_complete_but_physical_binding_remains_partial() {
    let chain: serde_json::Value = serde_json::from_str(include_str!(
        "../audit/region_a/evidence/iir_address_chain_20260922.json"
    ))
    .unwrap();

    assert_eq!(chain["status"], "PARTIAL");
    assert_eq!(chain["lexar_physical"]["region_a_start_lba"], 243623933u64);
    assert_eq!(
        chain["lexar_physical"]["required_partinfo2_sector_num"],
        243624189u64
    );
    assert_eq!(
        chain["lexar_physical"]["relation"],
        "required_partinfo2_sector_num - 256 == region_a_start_lba"
    );
    assert!(chain["missing_evidence"]
        .as_str()
        .unwrap()
        .contains("not directly observed"));

    let wire = include_str!("../audit/region_a/wire_byte_ledger.tsv");
    assert!(wire.contains("000-7ff\tPARTIAL"));
    assert!(wire.contains("physical Lexar PartInfo[2] runtime value is still missing"));
    assert!(DOC.contains("保持 PARTIAL，禁止写成“已证明同址”"));
}

#[test]
fn current_x64_core_key_dataflow_is_closed_without_overclaiming_runtime_uniqueness() {
    let evidence: serde_json::Value = serde_json::from_str(include_str!(
        "../audit/region_a/evidence/core_key_provenance_20260922.json"
    ))
    .unwrap();

    assert_eq!(evidence["status"], "STATIC_BOUNDARY_CLOSED_CURRENT_X64");
    assert_eq!(
        evidence["direct_writer"]["function"],
        "sub_18000b520 / SectorManageImp::Init"
    );
    assert_eq!(
        evidence["direct_writer"]["default_x64_key_ascii"],
        "8eeaa2062efa0b371076ebe510d98f18"
    );
    assert_eq!(evidence["consumers"].as_array().unwrap().len(), 3);
    assert_eq!(
        evidence["bounded_write_scan"]["direct_core_key_payload_writers_found"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        evidence["bounded_write_scan"]["explicit_set_or_load_core_key_api_found"],
        false
    );
    assert!(evidence["limitations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str().unwrap().contains("indirect memory mutation")));
    assert!(DOC.contains("只发现 `Init` 这一处直接 core-key payload writer"));
}

#[test]
fn partinfo_transport_uses_readonly_fe06_and_aes256_ecb_but_physical_response_is_missing() {
    let evidence: serde_json::Value = serde_json::from_str(include_str!(
        "../audit/region_a/evidence/partinfo_transport_crypto_20260922.json"
    ))
    .unwrap();

    assert_eq!(
        evidence["status"],
        "STATIC_COMPLETE_PHYSICAL_RESPONSE_MISSING"
    );
    assert_eq!(evidence["command"]["cdb_hex"], "fe0600000000000000000000");
    assert_eq!(evidence["command"]["transfer_direction"], "device-to-host");
    assert_eq!(evidence["command"]["transfer_length"], 512);
    assert_eq!(evidence["response_crypto"]["cipher"], "AES-256-ECB");
    assert_eq!(evidence["response_crypto"]["key_len"], 32);
    assert_eq!(evidence["response_crypto"]["block_size"], 16);
    assert_eq!(evidence["response_crypto"]["rounds"], 14);
    assert_eq!(
        evidence["response_crypto"]["key_ascii"],
        "1234567890abcdefFEDCBA!@#$%^&*()"
    );
    assert_eq!(
        evidence["decrypted_layout"]["partinfo2_sector_num_offset"],
        56
    );
    assert_eq!(
        evidence["decrypted_layout"]["partinfo2_expected_for_current_lexar"],
        243624189u64
    );
    assert_eq!(
        evidence["physical_status"]["current_lexar_response_captured"],
        false
    );
    assert!(DOC.contains("**AES-256-ECB**"));
    assert!(DOC.contains("真实 Lexar 的 512B `FE 06` DATA-IN 尚未成功捕获"));
}
