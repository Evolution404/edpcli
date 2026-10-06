use std::env;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout)
        .trim_end_matches(['\r', '\n'])
        .to_string();
    (!text.is_empty()).then_some(text)
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    // Howard Hinnant's civil-from-days algorithm, epoch 1970-01-01.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = (yoe + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (mp + if mp < 10 { 3 } else { -9 }) as u32;
    year += i32::from(month <= 2);
    (year, month, day)
}

fn rfc3339_utc(timestamp: u64) -> String {
    let days = (timestamp / 86_400) as i64;
    let seconds = timestamp % 86_400;
    let hour = seconds / 3_600;
    let minute = (seconds % 3_600) / 60;
    let second = seconds % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn build_timestamp() -> String {
    let timestamp = env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before Unix epoch")
                .as_secs()
        });
    rfc3339_utc(timestamp)
}

fn main() {
    // Git resolves linked worktrees, detached HEAD and packed refs correctly.
    // Watch parents too: refs/files can be removed or packed after this build.
    let mut watched = std::collections::BTreeSet::new();
    for name in ["HEAD", "index", "packed-refs", "refs"] {
        if let Some(path) = command_output("git", &["rev-parse", "--git-path", name]) {
            let path = std::path::PathBuf::from(path);
            if path.exists() {
                watched.insert(path.clone());
            }
            if let Some(parent) = path.parent().filter(|p| p.exists()) {
                watched.insert(parent.to_path_buf());
            }
        }
    }
    if let Some(path) = command_output("git", &["symbolic-ref", "-q", "HEAD"])
        .and_then(|reference| command_output("git", &["rev-parse", "--git-path", &reference]))
    {
        let path = std::path::PathBuf::from(path);
        if path.exists() {
            watched.insert(path.clone());
        }
        if let Some(parent) = path.parent().filter(|p| p.exists()) {
            watched.insert(parent.to_path_buf());
        }
    }
    if let Some(paths) = command_output("git", &["ls-files", "-z"]) {
        for name in paths.split('\0').filter(|name| !name.is_empty()) {
            let path = std::path::PathBuf::from(name);
            // A deleted tracked file must stay watched until it reappears.
            // Missing files may rerun builds; omitting them would hide restoration.
            watched.insert(path.clone());
            if let Some(parent) = path.parent().filter(|p| p.exists()) {
                watched.insert(parent.to_path_buf());
            }
        }
    } else {
        // Source archives do not contain Git metadata.
        watched.extend(["build.rs", "src", "Cargo.toml", "Cargo.lock"].map(Into::into));
    }
    for path in watched {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".into());
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| "unknown".into());
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| "unknown".into());
    let profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".into());
    let git_commit = command_output("git", &["rev-parse", "--short=12", "HEAD"])
        .unwrap_or_else(|| "unknown".into());
    let git_dirty = command_output(
        "git",
        &[
            "--no-optional-locks",
            "status",
            "--porcelain",
            "--untracked-files=no",
        ],
    )
    .is_some_and(|output| !output.is_empty());
    let rustc = env::var("RUSTC")
        .ok()
        .and_then(|rustc| command_output(&rustc, &["--version"]))
        .unwrap_or_else(|| "rustc unknown".into());

    println!("cargo:rustc-env=EDPCLI_BUILD_TARGET={target}");
    println!("cargo:rustc-env=EDPCLI_BUILD_ARCH={target_arch}");
    println!("cargo:rustc-env=EDPCLI_BUILD_OS={target_os}");
    println!("cargo:rustc-env=EDPCLI_BUILD_PROFILE={profile}");
    println!(
        "cargo:rustc-env=EDPCLI_BUILD_TIMESTAMP={}",
        build_timestamp()
    );
    println!(
        "cargo:rustc-env=EDPCLI_BUILD_GIT={}{}",
        git_commit,
        if git_dirty { "+dirty" } else { "" }
    );
    println!("cargo:rustc-env=EDPCLI_BUILD_RUSTC={rustc}");
}
