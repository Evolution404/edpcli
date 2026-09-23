#[test]
fn repository_pre_commit_hook_autoformats_staged_rust_files() {
    let hook = include_str!("../.githooks/pre-commit");
    assert!(hook.contains("rustfmt --edition 2021"));
    assert!(hook.contains("git diff --cached --name-only"));
    assert!(hook.contains("git add --"));
    assert!(hook.contains("partially staged"));
    assert!(hook.contains("$HOME/.cargo/env"));
    assert!(hook.contains(". \"$HOME/.cargo/env\""));
}

#[test]
fn repository_documents_hook_installation_for_unix_and_windows() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("scripts/install-git-hooks.sh"));
    assert!(readme.contains("scripts/install-git-hooks.ps1"));
    assert!(readme.contains("core.hooksPath"));
}

#[test]
fn agent_policy_requires_formatting_before_remote_commits() {
    let policy = include_str!("../AGENTS.md");
    assert!(policy.contains("cargo fmt --all"));
    assert!(policy.contains("before every commit"));
}
