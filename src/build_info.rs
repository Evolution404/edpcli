//! 编译时版本与目标平台信息。

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const TARGET: &str = env!("EDPCLI_BUILD_TARGET");
pub const TARGET_ARCH: &str = env!("EDPCLI_BUILD_ARCH");
pub const TARGET_OS: &str = env!("EDPCLI_BUILD_OS");
pub const PROFILE: &str = env!("EDPCLI_BUILD_PROFILE");
pub const BUILD_TIMESTAMP: &str = env!("EDPCLI_BUILD_TIMESTAMP");
pub const GIT_COMMIT: &str = env!("EDPCLI_BUILD_GIT");
pub const RUSTC: &str = env!("EDPCLI_BUILD_RUSTC");

pub fn display_arch() -> &'static str {
    match TARGET_ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        other => other,
    }
}

pub fn display_os() -> &'static str {
    match TARGET_OS {
        "macos" => "macOS",
        "linux" => "Linux",
        "windows" => "Windows",
        other => other,
    }
}

pub fn short() -> String {
    format!("edpcli {VERSION}")
}

pub fn local_build_timestamp() -> String {
    let Ok(utc) = time::OffsetDateTime::parse(
        BUILD_TIMESTAMP,
        &time::format_description::well_known::Rfc3339,
    ) else {
        return BUILD_TIMESTAMP.to_string();
    };
    let Ok(offset) = time::UtcOffset::local_offset_at(utc) else {
        return BUILD_TIMESTAMP.to_string();
    };
    utc.to_offset(offset)
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| BUILD_TIMESTAMP.to_string())
}

pub fn detailed() -> String {
    format!(
        "edpcli {VERSION}\n\
平台: {}\n\
架构: {} ({TARGET_ARCH})\n\
目标: {TARGET}\n\
构建时间: {}\n\
Git: {GIT_COMMIT}\n\
Rust: {RUSTC}\n\
构建类型: {PROFILE}",
        display_os(),
        display_arch(),
        local_build_timestamp()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_version_contains_release_diagnostics() {
        let text = detailed();
        assert!(text.starts_with(&format!("edpcli {VERSION}\n")));
        for field in [
            "平台:",
            "架构:",
            "目标:",
            "构建时间:",
            "Git:",
            "Rust:",
            "构建类型:",
        ] {
            assert!(text.contains(field), "missing {field}: {text}");
        }
        assert!(text.contains(&format!("构建时间: {}", local_build_timestamp())));
        assert!(BUILD_TIMESTAMP.ends_with('Z'));
        assert!(!GIT_COMMIT.is_empty());
    }
}
