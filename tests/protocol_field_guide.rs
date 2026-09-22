#[path = "../scripts/protocol/generate_field_guide.rs"]
mod generator;

#[test]
fn field_guide_is_the_exact_catalog_and_axes_projection() {
    assert_eq!(
        include_str!("../docs/EDP_LBA0_12_FIELD_GUIDE.md"),
        generator::render(),
        "regenerate the field guide after changing canonical TSVs"
    );
}

#[test]
fn guide_covers_every_field_in_sector_order_with_rules_evidence_and_status() {
    let guide = generator::render();
    let sector_headers: Vec<_> = guide
        .lines()
        .filter(|line| line.starts_with("## LBA"))
        .collect();
    assert_eq!(
        sector_headers,
        (0..13)
            .map(|lba| format!("## LBA{lba}"))
            .collect::<Vec<_>>()
    );
    let catalog = include_str!("../audit/protocol/field_catalog.tsv");
    let mut lines = catalog.lines();
    let header: Vec<_> = lines.next().unwrap().split('\t').collect();
    for line in lines {
        let cols: Vec<_> = line.split('\t').collect();
        let get = |key| cols[header.iter().position(|name| *name == key).unwrap()];
        let heading = format!(
            "#### {} · {} / {}\n",
            get("field_id"),
            get("profile_axis"),
            get("profile")
        );
        assert_eq!(
            guide.matches(&heading).count(),
            1,
            "missing/duplicate {heading}"
        );
        let block = guide
            .split_once(&heading)
            .unwrap()
            .1
            .split("\n###")
            .next()
            .unwrap();
        for label in [
            "Offset",
            "Semantic type",
            "Meaning",
            "Ownership",
            "Decode rule",
            "Encode rule",
            "Profile",
            "Evolution kind",
            "Producer evidence",
            "Consumer evidence",
            "Physical evidence",
            "Implementation provenance",
            "Semantic status",
            "Implementation status",
            "Behavior-test status",
            "Code symbol",
            "Test symbol",
            "Ownership test",
        ] {
            assert!(
                block.contains(&format!("| {label} |")),
                "missing {label}: {heading}"
            );
        }
    }
}
