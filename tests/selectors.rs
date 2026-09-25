use crate::common;

use std::fs;

use common::{fixture, fixture_bin, TmpDir};
use edpcli::cli::Prompter;
use edpcli::selectors::{BackupSelector, DeviceSelector};
use edpcli::sysinfo::ExtDisk;

struct Prompt {
    answers: Vec<String>,
}

impl Prompt {
    fn one(answer: &str) -> Self {
        Self {
            answers: vec![answer.to_string()],
        }
    }
}

impl Prompter for Prompt {
    fn prompt_line(&mut self, _msg: &str) -> String {
        self.answers.remove(0)
    }

    fn confirm_yes(&mut self, _msg: &str) -> bool {
        false
    }
}

fn disk(n: u32) -> ExtDisk {
    ExtDisk {
        n,
        size: 64_000_000_000,
        vid: "1234".into(),
        pid: format!("{n:04x}"),
        proto: "USB".into(),
    }
}

#[test]
fn device_selector_handles_explicit_single_and_interactive_targets() {
    let disks = vec![disk(4), disk(6)];

    let mut prompt = Prompt::one("1");
    assert_eq!(
        DeviceSelector::new(Some(6))
            .choose_from(&disks, &mut prompt)
            .unwrap(),
        6
    );

    let mut prompt = Prompt::one("unused");
    assert_eq!(
        DeviceSelector::new(None)
            .choose_from(&[disk(4)], &mut prompt)
            .unwrap(),
        4
    );

    let mut prompt = Prompt::one("2");
    assert_eq!(
        DeviceSelector::new(None)
            .choose_from(&disks, &mut prompt)
            .unwrap(),
        6
    );

    let mut prompt = Prompt::one("1");
    let error = DeviceSelector::new(Some(9))
        .choose_from(&disks, &mut prompt)
        .unwrap_err();
    assert_eq!(error.code, edpcli::common::EXIT_TARGET);
}

#[test]
fn device_selector_pins_native_selector_without_changing_command_shape() {
    let selector = DeviceSelector::new(Some(6));
    let mut argv = vec![
        "apply".to_string(),
        "--dry-run".to_string(),
        "--disk=6".to_string(),
    ];
    selector.pin_argv(&mut argv, 6);
    assert_eq!(
        argv,
        vec![
            "apply",
            "--dry-run",
            &format!("--disk={}", edpcli::platform::disk_selector_value(6))
        ]
    );
}

fn copied_backups() -> Option<(TmpDir, Vec<String>)> {
    let tmp = TmpDir::new("selectors");
    let mut names = Vec::new();
    for key in ["netac", "aigo", "lexar"] {
        let src = fixture_bin(key)?;
        let (name, _) = fixture(key)?;
        let bytes = fs::read(src).expect("read protocol fixture");
        let name = format!("{}.edpb", name.strip_suffix(".bin").unwrap());
        let meta = edpcli::diskio::parse_backup_name(&name).unwrap();
        edpcli::edpb::write_core_backup(
            &tmp.0.join(&name),
            &edpcli::edpb::CoreCapture {
                snapshot_id: name.clone(),
                created_epoch: 1_789_000_000,
                disk_number: Some(meta.disk),
                vid: meta.vid,
                pid: meta.pid,
                device_id: meta.device_id,
                onlyid: meta.onlyid,
                total_sectors: meta.secs,
                logical_sector_size: 512,
                edpcli_version: env!("CARGO_PKG_VERSION").into(),
                device_state: "encrypted".into(),
                lba0_12: &bytes,
            },
        )
        .expect("write selector EDPB fixture");
        names.push(name);
    }
    Some((tmp, names))
}

#[test]
fn backup_selector_uses_one_global_stable_numbering_for_list_and_actions() {
    let Some((tmp, names)) = copied_backups() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let selector = BackupSelector::load(&tmp.0);
    let numbered = selector.numbered();
    assert_eq!(numbered.len(), 3);
    assert_eq!(
        numbered[0].path.file_name().unwrap().to_string_lossy(),
        names[0]
    );

    let by_number = selector.resolve_one("1").unwrap();
    let by_name = selector.resolve_one(&names[0]).unwrap();
    assert_eq!(by_number.path, by_name.path);
}

#[test]
fn backup_selector_supports_ranges_and_internal_identity_filtering() {
    let Some((tmp, _)) = copied_backups() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let selector = BackupSelector::load(&tmp.0);
    let selected = selector.resolve_many(&["1,3".into()]).unwrap();
    assert_eq!(selected.len(), 2);

    let netac = selector.for_onlyid("1402259934");
    assert_eq!(netac.numbered().len(), 1);
    assert_eq!(
        netac.numbered()[0]
            .meta
            .as_ref()
            .and_then(|meta| meta.onlyid.as_deref()),
        Some("1402259934")
    );
    assert!(netac.resolve_one("2").is_err());

    let global = selector.numbered();
    let aigo_global_index = global
        .iter()
        .position(|entry| {
            entry.meta.as_ref().and_then(|meta| meta.onlyid.as_deref()) == Some("1987718388")
        })
        .map(|index| index + 1)
        .expect("aigo backup");
    let aigo = selector.for_onlyid("1987718388");
    let selected = aigo
        .resolve_one(&aigo_global_index.to_string())
        .expect("filtered restore selector must preserve global numbering");
    assert_eq!(
        selected
            .meta
            .as_ref()
            .and_then(|meta| meta.onlyid.as_deref()),
        Some("1987718388")
    );
}
