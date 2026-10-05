use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

fn repo_file(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|error| panic!("read {path}: {error}"))
}

#[test]
fn physical_hil_scenarios_cover_release_critical_state_transitions() {
    let manifest = repo_file("audit/hil/physical_scenarios.tsv");
    let mut lines = manifest.lines();
    assert_eq!(
        lines.next().unwrap(),
        "scenario_id\tplatforms\tdestructive\tsource_state\toperation\texpected_layout\texpected_filesystem\texpected_crypto\texpected_readback\trequirements"
    );
    let rows = lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert!(rows.iter().all(|row| row.len() == 10));
    assert!(rows.iter().all(|row| row[1] == "macos,windows,linux"));
    assert!(rows.iter().all(|row| row[2] == "true"));

    let actual = rows.iter().map(|row| row[0]).collect::<BTreeSet<_>>();
    let expected = [
        "critical_write_delayed_exit",
        "identity_changed_reject",
        "metadata_restore_encrypted_exfat",
        "metadata_restore_fat32",
        "mode0_to_mode1",
        "plain_to_mode0",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn physical_hil_result_contract_only_allows_executed_outcomes() {
    let schema: serde_json::Value =
        serde_json::from_str(&repo_file("audit/hil/physical_result.schema.json")).unwrap();
    assert_eq!(schema["properties"]["schema_version"]["const"], 1);
    assert_eq!(
        schema["properties"]["platform"]["enum"],
        serde_json::json!(["macos", "windows", "linux"])
    );
    assert_eq!(
        schema["properties"]["status"]["enum"],
        serde_json::json!(["pass", "fail", "blocked"])
    );
    assert!(!schema.to_string().contains("not_run"));
    let required = schema["required"].as_array().unwrap();
    for field in ["scenario_id", "commit", "device", "assertions", "evidence"] {
        assert!(
            required.iter().any(|value| value == field),
            "missing {field}"
        );
    }
}
