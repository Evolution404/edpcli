use std::fs;
use std::path::Path;

const BUSINESS_SOURCES: &[&str] = &[
    "src/backup_catalog.rs",
    "src/backup_cli.rs",
    "src/cli.rs",
    "src/cli_args.rs",
    "src/completion.rs",
    "src/crypto.rs",
    "src/disk_scan.rs",
    "src/diskio.rs",
    "src/elevate.rs",
    "src/identify.rs",
    "src/inspect.rs",
    "src/inspect/model.rs",
    "src/inspect/lba_adapter.rs",
    "src/inspect/lba_early.rs",
    "src/inspect/lba_middle.rs",
    "src/inspect/lba_late.rs",
    "src/inspect/render.rs",
    "src/inspect_cli.rs",
    "src/metainfo.rs",
    "src/metainfo_cli.rs",
    "src/sectors.rs",
];

const FORBIDDEN_OS_DETAILS: &[&str] = &[
    "diskutil",
    "ioreg",
    "sudo",
    "/dev/rdisk",
    "/dev/disk",
    "/sys/",
    "physicaldrive",
    "sysfs",
    "powershell",
    "systemdrive",
];

#[test]
fn business_layer_has_no_platform_specific_command_or_path_details() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();

    for relative in BUSINESS_SOURCES {
        let text = fs::read_to_string(root.join(relative)).expect("read business source");
        let lower = text.to_ascii_lowercase();
        for forbidden in FORBIDDEN_OS_DETAILS {
            if lower.contains(forbidden) {
                violations.push(format!("{relative}: {forbidden}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "OS-specific details must stay behind src/platform/:\n{}",
        violations.join("\n")
    );
}
