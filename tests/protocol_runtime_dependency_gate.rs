use std::{
    fs,
    path::{Path, PathBuf},
};

fn collect_files(root: &Path, out: &mut Vec<PathBuf>) {
    if !root.exists() {
        return;
    }
    for entry in fs::read_dir(root).expect("read directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn forbidden_runtime_roots() -> Vec<String> {
    vec![
        ["/Users/", "zhangyuxi", "/Desktop/", "u_disk"].concat(),
        ["Desktop/", "u_disk"].concat(),
        ["/private/", "tmp"].concat(),
    ]
}

#[test]
fn protocol_has_no_external_runtime_fixture_dependency() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();

    for relative in ["src", "examples", "scripts/protocol", "tests"] {
        collect_files(&repo.join(relative), &mut files);
    }

    let forbidden = forbidden_runtime_roots();
    let mut violations = Vec::new();

    for path in files {
        let relative = path.strip_prefix(repo).expect("repo-relative path");
        let relative_text = relative.to_string_lossy();

        if relative_text == "tests/protocol_runtime_dependency_gate.rs"
            || relative_text == "tests/protocol_documentation_contract.rs"
        {
            // The gate contains the deny-list itself. The documentation contract
            // intentionally pins historical provenance strings, not runtime inputs.
            continue;
        }

        let extension = path.extension().and_then(|value| value.to_str());
        let in_rust_source = relative_text.starts_with("tests/")
            || relative_text.starts_with("src/")
            || relative_text.starts_with("examples/");
        let in_research_scripts = relative_text.starts_with("scripts/protocol/");
        let relevant = (in_rust_source && extension == Some("rs"))
            || (in_research_scripts && extension == Some("py"));
        if !relevant {
            continue;
        }

        let text = fs::read_to_string(&path).expect("protocol source must be UTF-8");
        for needle in &forbidden {
            if text.contains(needle) {
                violations.push(format!(
                    "{} contains forbidden runtime root {needle:?}",
                    relative.display()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "protocol validation must be clean-clone reproducible; external runtime dependencies found:\n{}",
        violations.join("\n")
    );
}
