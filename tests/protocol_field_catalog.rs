//! Phase 1 machine gates for the LBA0-LBA12 field × profile model.
//!
//! The catalog deliberately separates owner axes from overlay axes:
//! - owner axes select the unique semantic owner for a byte in one profile state;
//! - overlay axes describe an independent producer/value variation and never claim ownership.
//!
//! This lets independent history axes compose without inventing a monolithic Legacy20xx version.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::PathBuf;

const TOTAL_BYTES: usize = 13 * 512;

#[derive(Clone, Debug, Eq, PartialEq)]
struct AxisState {
    role: String,
    effect_kind: String,
    bytes: BTreeSet<usize>,
}

#[derive(Clone, Debug)]
struct CatalogRow {
    field_id: String,
    lba: usize,
    start: usize,
    end: usize,
    profile_axis: String,
    profile: String,
    semantic_type: String,
    ownership: String,
    evolution_kind: String,
    producer_evidence: String,
    consumer_evidence: String,
    physical_evidence: String,
    code_symbol: String,
    test_symbol: String,
    semantic_status: String,
    implementation_status: String,
    behavior_test_status: String,
    ownership_test_symbol: String,
}

fn repo_file(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn parse_hex(value: &str) -> usize {
    usize::from_str_radix(value, 16).unwrap_or_else(|_| panic!("bad hex offset: {value}"))
}

fn expand_range_spec(spec: &str) -> BTreeSet<usize> {
    let mut out = BTreeSet::new();
    for part in spec.split(';').filter(|part| !part.is_empty()) {
        let (lba, offsets) = part
            .split_once(':')
            .unwrap_or_else(|| panic!("bad LBA range spec: {part}"));
        let lba: usize = lba.parse().expect("numeric LBA");
        assert!(lba < 13, "LBA out of range in {part}");
        let (start, end) = offsets
            .split_once('-')
            .map(|(start, end)| (parse_hex(start), parse_hex(end)))
            .unwrap_or_else(|| {
                let value = parse_hex(offsets);
                (value, value)
            });
        assert!(start <= end && end < 512, "bad byte range: {part}");
        for offset in start..=end {
            assert!(
                out.insert(lba * 512 + offset),
                "duplicate byte inside range spec: {part}"
            );
        }
    }
    out
}

fn parse_axes() -> BTreeMap<(String, String), AxisState> {
    let text = repo_file("audit/protocol/profile_axes.tsv");
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("axis\tstate\trole\tranges\teffect_kind\tdescription\tevidence"),
        "profile_axes.tsv header drift"
    );

    let mut axes = BTreeMap::new();
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let cols: Vec<_> = line.split('\t').collect();
        assert_eq!(cols.len(), 7, "bad profile axis row: {line}");
        assert!(
            matches!(cols[2], "owner" | "overlay"),
            "bad profile axis role: {line}"
        );
        assert!(
            matches!(
                cols[4],
                "AddedRemoved" | "EncodingChanged" | "OwnershipChanged" | "ProducerChanged"
            ),
            "bad profile effect kind: {line}"
        );
        assert!(
            !cols[5].trim().is_empty(),
            "axis description is empty: {line}"
        );
        assert!(!cols[6].trim().is_empty(), "axis evidence is empty: {line}");
        let key = (cols[0].to_owned(), cols[1].to_owned());
        let previous = axes.insert(
            key.clone(),
            AxisState {
                role: cols[2].to_owned(),
                bytes: expand_range_spec(cols[3]),
                effect_kind: cols[4].to_owned(),
            },
        );
        assert!(previous.is_none(), "duplicate axis/state: {key:?}");
    }
    axes
}

fn parse_catalog() -> Vec<CatalogRow> {
    let text = repo_file("audit/protocol/field_catalog.tsv");
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some(
            "field_id\tlba\tstart\tend\tlength\tprofile_axis\tprofile\tsemantic_type\tmeaning\tdecode_rule\tencode_rule\townership\tevolution_kind\tproducer_evidence\tconsumer_evidence\tphysical_evidence\timplementation_provenance\tcode_symbol\ttest_symbol\tsemantic_status\timplementation_status\tbehavior_test_status\townership_test_symbol"
        ),
        "field_catalog.tsv header drift"
    );

    let allowed_semantics = HashSet::from([
        "Scalar",
        "CString",
        "PackedStruct",
        "EncryptedRegion",
        "Checksum",
        "ProfileSelector",
        "OpaquePreserve",
        "UnownedBacking",
        "CompatibilityField",
        "Snapshot",
    ]);
    let allowed_evolution = HashSet::from([
        "Stable",
        "AddedRemoved",
        "EncodingChanged",
        "OwnershipChanged",
        "ProducerChanged",
    ]);

    let mut rows = Vec::new();
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let cols: Vec<_> = line.split('\t').collect();
        assert_eq!(cols.len(), 23, "bad field catalog row: {line}");
        let lba: usize = cols[1].parse().expect("numeric catalog LBA");
        let start = parse_hex(cols[2]);
        let end = parse_hex(cols[3]);
        let length: usize = cols[4].parse().expect("numeric catalog length");
        assert!(
            lba < 13 && start <= end && end < 512,
            "bad catalog range: {line}"
        );
        assert_eq!(length, end - start + 1, "catalog length mismatch: {line}");
        assert!(
            allowed_semantics.contains(cols[7]),
            "unknown semantic type: {line}"
        );
        assert!(
            allowed_evolution.contains(cols[12]),
            "unknown evolution kind: {line}"
        );
        assert!(
            matches!(cols[11], "owner" | "overlay"),
            "bad ownership kind: {line}"
        );
        for col in [8usize, 9, 10, 13, 14, 15, 16, 17, 18] {
            assert!(
                !cols[col].trim().is_empty(),
                "empty required catalog cell: {line}"
            );
        }
        assert_eq!(
            cols[19], "COMPLETE",
            "Phase 1 imports only closed semantics"
        );

        rows.push(CatalogRow {
            field_id: cols[0].to_owned(),
            lba,
            start,
            end,
            profile_axis: cols[5].to_owned(),
            profile: cols[6].to_owned(),
            semantic_type: cols[7].to_owned(),
            ownership: cols[11].to_owned(),
            evolution_kind: cols[12].to_owned(),
            producer_evidence: cols[13].to_owned(),
            consumer_evidence: cols[14].to_owned(),
            physical_evidence: cols[15].to_owned(),
            code_symbol: cols[17].to_owned(),
            test_symbol: cols[18].to_owned(),
            semantic_status: cols[19].to_owned(),
            implementation_status: cols[20].to_owned(),
            behavior_test_status: cols[21].to_owned(),
            ownership_test_symbol: cols[22].to_owned(),
        });
    }
    rows
}

fn evidence_modalities() -> BTreeMap<String, String> {
    let text = repo_file("audit/protocol/evidence_manifest.tsv");
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("id\tmodality\tsubject\tversion\tsha256\tlocator\tanchors\tproves\tlimitations"),
        "evidence_manifest.tsv header drift"
    );

    let mut modalities = BTreeMap::new();
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let cols: Vec<_> = line.split('\t').collect();
        assert_eq!(cols.len(), 9, "bad evidence manifest row: {line}");
        let previous = modalities.insert(cols[0].to_owned(), cols[1].to_owned());
        assert!(previous.is_none(), "duplicate evidence id: {}", cols[0]);
    }
    modalities
}

fn row_bytes(row: &CatalogRow) -> impl Iterator<Item = usize> {
    let base = row.lba * 512;
    (row.start..=row.end).map(move |offset| base + offset)
}

#[test]
fn orthogonal_profile_axes_keep_required_historical_forks_independent() {
    let axes = parse_axes();
    let required: [(&str, &[&str]); 6] = [
        ("lba0_bootstrap", &["zero", "usb-main-bsec", "netac-mbr"]),
        ("dept_layout", &["short", "join59", "join60"]),
        ("lba4_encoding", &["post-xor", "ordinary-rolling"]),
        ("lba10_eesi", &["absent-zero", "eesi-enabled"]),
        ("lba11_capacity", &["disk-size", "repair-chs"]),
        ("lba12_mode", &["legacy-v0064", "mode1", "mode2", "mode3"]),
    ];
    for (axis, states) in required {
        for state in states {
            assert!(
                axes.contains_key(&(axis.to_owned(), (*state).to_owned())),
                "missing required orthogonal profile state {axis}/{state}"
            );
        }
    }

    assert!(
        axes.keys().all(|(axis, _)| {
            !axis.contains("legacy20")
                && !axis.contains("legacy_20")
                && !axis.contains("historical_version")
        }),
        "profile model regressed to a monolithic historical-version axis"
    );

    let mut by_axis: BTreeMap<&str, Vec<&AxisState>> = BTreeMap::new();
    for ((axis, _), state) in &axes {
        by_axis.entry(axis).or_default().push(state);
    }
    for (axis, states) in &by_axis {
        assert!(
            states.len() >= 2,
            "profile axis needs at least two states: {axis}"
        );
        let first = states[0];
        for state in &states[1..] {
            assert_eq!(
                state.role, first.role,
                "axis states disagree on role for {axis}"
            );
            assert_eq!(
                state.effect_kind, first.effect_kind,
                "axis states disagree on effect kind for {axis}"
            );
            assert_eq!(
                state.bytes, first.bytes,
                "axis states must describe the same physical scope for {axis}"
            );
        }
    }

    let owner_axes: Vec<_> = by_axis
        .iter()
        .filter(|(_, states)| states[0].role == "owner")
        .map(|(axis, states)| (*axis, &states[0].bytes))
        .collect();
    for left in 0..owner_axes.len() {
        for right in left + 1..owner_axes.len() {
            let overlap = owner_axes[left].1.intersection(owner_axes[right].1).next();
            assert!(
                overlap.is_none(),
                "owner axes {} and {} overlap at physical byte {:?}; use an overlay axis for independent variation",
                owner_axes[left].0,
                owner_axes[right].0,
                overlap
            );
        }
    }
}

#[test]
fn field_catalog_has_exact_byte_ownership_for_every_profile_state() {
    let axes = parse_axes();
    let rows = parse_catalog();
    let evidence = evidence_modalities();

    let mut field_keys = HashSet::new();
    for row in &rows {
        assert!(
            (row.profile_axis == "base" && row.profile == "all")
                || axes.contains_key(&(row.profile_axis.clone(), row.profile.clone())),
            "unknown field profile: {}/{}",
            row.profile_axis,
            row.profile
        );
        assert!(
            field_keys.insert((
                row.field_id.clone(),
                row.profile_axis.clone(),
                row.profile.clone()
            )),
            "duplicate field/profile row: {}/{}/{}",
            row.field_id,
            row.profile_axis,
            row.profile
        );
        for ids in [&row.producer_evidence, &row.consumer_evidence] {
            for id in ids.split(';') {
                assert!(
                    evidence.contains_key(id),
                    "unknown evidence id {id} in field {}",
                    row.field_id
                );
            }
        }
        if row.physical_evidence != "MISSING_PHYSICAL" {
            for id in row.physical_evidence.split(';') {
                let modality = evidence.get(id).unwrap_or_else(|| {
                    panic!(
                        "unknown physical evidence id {id} in field {}",
                        row.field_id
                    )
                });
                assert_eq!(
                    modality, "physical",
                    "physical_evidence must reference physical captures only: field {} uses {id} ({modality})",
                    row.field_id
                );
            }
        }
        assert!(!row.code_symbol.is_empty());
        assert!(!row.test_symbol.is_empty());
        assert_eq!(row.semantic_status, "COMPLETE");
    }

    let mut owner_scope = BTreeSet::new();
    let mut axis_states: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for ((axis, state), profile) in &axes {
        axis_states.entry(axis).or_default().push(state);
        if profile.role == "owner" {
            owner_scope.extend(profile.bytes.iter().copied());
        }
    }

    let base_rows: Vec<_> = rows
        .iter()
        .filter(|row| row.profile_axis == "base" && row.profile == "all")
        .collect();
    assert!(!base_rows.is_empty(), "base field rows are required");

    let mut base_counts = vec![0u16; TOTAL_BYTES];
    for row in base_rows {
        assert_eq!(row.ownership, "owner", "base rows must own their bytes");
        for byte in row_bytes(row) {
            assert!(
                !owner_scope.contains(&byte),
                "base row {} overlaps a profile-owned scope at physical byte {byte}",
                row.field_id
            );
            base_counts[byte] += 1;
        }
    }
    for byte in 0..TOTAL_BYTES {
        let expected = usize::from(!owner_scope.contains(&byte));
        assert_eq!(
            base_counts[byte] as usize,
            expected,
            "base ownership gap/overlap at LBA{}+0x{:03x}",
            byte / 512,
            byte % 512
        );
    }

    for ((axis, state), profile) in &axes {
        let profile_rows: Vec<_> = rows
            .iter()
            .filter(|row| row.profile_axis == *axis && row.profile == *state)
            .collect();
        assert!(
            !profile_rows.is_empty(),
            "catalog has no rows for profile {axis}/{state}"
        );
        let expected_ownership = profile.role.as_str();
        let mut counts = vec![0u16; TOTAL_BYTES];
        for row in profile_rows {
            assert_eq!(
                row.ownership, expected_ownership,
                "profile row ownership disagrees with axis role: {axis}/{state}"
            );
            for byte in row_bytes(row) {
                assert!(
                    profile.bytes.contains(&byte),
                    "field {} escapes declared scope for {axis}/{state}",
                    row.field_id
                );
                counts[byte] += 1;
            }
        }
        for byte in 0..TOTAL_BYTES {
            let expected = usize::from(profile.bytes.contains(&byte));
            assert_eq!(
                counts[byte] as usize,
                expected,
                "{expected_ownership} gap/overlap for {axis}/{state} at LBA{}+0x{:03x}",
                byte / 512,
                byte % 512
            );
        }
    }

    let physical_union =
        owner_scope.len() + base_counts.iter().filter(|count| **count == 1).count();
    assert_eq!(
        physical_union, TOTAL_BYTES,
        "physical owner coverage must be exactly 6656 bytes"
    );
}

#[test]
fn field_catalog_keeps_semantic_types_and_evolution_machine_readable() {
    let rows = parse_catalog();
    let semantics: HashSet<_> = rows.iter().map(|row| row.semantic_type.as_str()).collect();
    for required in [
        "Scalar",
        "CString",
        "PackedStruct",
        "EncryptedRegion",
        "Checksum",
        "ProfileSelector",
        "OpaquePreserve",
        "UnownedBacking",
        "CompatibilityField",
        "Snapshot",
    ] {
        assert!(
            semantics.contains(required),
            "semantic type is unused: {required}"
        );
    }

    let evolution: HashSet<_> = rows.iter().map(|row| row.evolution_kind.as_str()).collect();
    for required in [
        "Stable",
        "AddedRemoved",
        "EncodingChanged",
        "OwnershipChanged",
        "ProducerChanged",
    ] {
        assert!(
            evolution.contains(required),
            "evolution kind is unused: {required}"
        );
    }
}

// COMPLETE links are opt-in, compile-checked references, never source-text matches.
// Phase 3: symbol_registry!(edpcli::protocol::lba0::parse_lba0) below when implemented.
// Phase 4: add #[test] behavior functions here (or imported test modules), then
// register the callable path below. Review must still establish field relevance.
macro_rules! symbol_registry {
    ($($symbol:path),* $(,)?) => {{
        $(let _ = $symbol;)*
        BTreeSet::<String>::from([$(stringify!($symbol).replace(" ", "")),*])
    }};
}

fn code_symbols() -> BTreeSet<String> {
    symbol_registry![
        edpcli::protocol::lba0::parse_lba0,
        edpcli::protocol::lba1::parse_lba1,
        edpcli::protocol::lba12::parse_lba12,
        edpcli::protocol::lba2::parse_lba2,
        edpcli::protocol::lba3::parse_lba3,
        edpcli::protocol::lba4::parse_lba4,
        edpcli::protocol::lba6::parse_lba6,
        edpcli::protocol::lba7::parse_lba7,
        edpcli::protocol::lba8::parse_lba8,
        edpcli::protocol::lba9::parse_lba9,
        edpcli::protocol::lba5::parse_lba5
    ]
}

fn behavior_test_symbols() -> BTreeSet<String> {
    symbol_registry![
        dept_behavior::dept_and_safe6_fields_replay_physical_profiles_without_losing_backing,
        dept_behavior::dept_join_seams_and_post_nul_backing_are_independent,
        dept_behavior::lba9_eetu_sapf_eppe_and_long_user_keep_distinct_ownership,
        edpf_behavior::lba7_packed_entries_and_pass_info_replay_all_physical_profiles,
        edpf_behavior::lba12_outer_cipher_and_wrapped_key_modes_keep_profile_axes_distinct,
        edpf_behavior::pass_info_storage_transform_is_exact_and_profile_axes_fail_closed,
        lba8_behavior::lba8_dynamic_prefix_and_identity_profiles_preserve_backing,
        lba8_behavior::lba8_profile_axes_fail_closed_without_coupling,
        lba4_behavior::lba4_separates_wire_reader_and_producer_for_both_encodings,
        lba4_behavior::lba4_overlays_are_explicit_and_do_not_select_encoding,
        gpt_behavior::gpt_virtual_header_and_entries_are_typed_and_crc_checked,
        gpt_behavior::gpt_absent_is_explicit_and_unknown_never_defaults_to_absent,
        basic_behavior::lba0_profiles_decode_mbr_and_preserve_backing,
        basic_behavior::opaque_sectors_preserve_every_byte_without_inventing_manufacturer_semantics,
        basic_behavior::basic_parsers_replay_all_committed_physical_gold
    ]
}

fn completion_link_valid(status: &str, symbol: &str, registered: &BTreeSet<String>) -> bool {
    match status {
        "PLANNED" => symbol == "UNIMPLEMENTED" || symbol.starts_with("planned:"),
        "COMPLETE" => !symbol.starts_with("planned:") && registered.contains(symbol),
        _ => false,
    }
}

#[test]
fn field_catalog_separates_semantics_implementation_and_behavior_tests() {
    let code = code_symbols();
    let behavior = behavior_test_symbols();
    let ownership =
        symbol_registry![field_catalog_has_exact_byte_ownership_for_every_profile_state];
    for row in parse_catalog() {
        assert_eq!(row.semantic_status, "COMPLETE");
        assert!(
            completion_link_valid(&row.implementation_status, &row.code_symbol, &code),
            "invalid implementation link for {}: {} / {}",
            row.field_id,
            row.implementation_status,
            row.code_symbol
        );
        assert!(
            completion_link_valid(&row.behavior_test_status, &row.test_symbol, &behavior),
            "invalid behavior-test link for {}: {} / {}",
            row.field_id,
            row.behavior_test_status,
            row.test_symbol
        );
        let local = row
            .ownership_test_symbol
            .strip_prefix("protocol_field_catalog::")
            .unwrap();
        assert!(
            ownership.contains(local),
            "unknown ownership test: {}",
            row.ownership_test_symbol
        );
        assert_ne!(
            row.test_symbol, row.ownership_test_symbol,
            "generic ownership coverage is not field behavior coverage"
        );
    }
}

#[test]
fn completion_gate_rejects_planned_missing_and_unregistered_symbols() {
    let real = symbol_registry![completion_link_valid];
    assert!(completion_link_valid(
        "PLANNED",
        "planned:protocol::lba0::parser",
        &real
    ));
    assert!(completion_link_valid("PLANNED", "UNIMPLEMENTED", &real));
    assert!(completion_link_valid(
        "COMPLETE",
        "completion_link_valid",
        &real
    ));
    for symbol in [
        "",
        "UNIMPLEMENTED",
        "planned:completion_link_valid",
        "does_not_exist",
    ] {
        assert!(
            !completion_link_valid("COMPLETE", symbol, &real),
            "accepted {symbol}"
        );
    }
    assert!(!completion_link_valid(
        "COMPLETE",
        "completion_link_valid",
        &BTreeSet::new()
    ));
    assert!(!completion_link_valid(
        "PLANNED",
        "completion_link_valid",
        &real
    ));
    assert!(!completion_link_valid("UNKNOWN", "UNIMPLEMENTED", &real));
}

#[path = "protocol_behavior/basic.rs"]
mod basic_behavior;
#[path = "protocol_behavior/core.rs"]
mod core_behavior;
#[path = "protocol_behavior/dept.rs"]
mod dept_behavior;
#[path = "protocol_behavior/edpf.rs"]
mod edpf_behavior;
#[path = "protocol_behavior/gpt.rs"]
mod gpt_behavior;
#[path = "protocol_behavior/lba4.rs"]
mod lba4_behavior;
#[path = "protocol_behavior/lba8.rs"]
mod lba8_behavior;
