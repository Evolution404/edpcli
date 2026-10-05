//! Shared Unix account lookup and elevation command construction.
use std::ffi::{CStr, CString, OsStr};
use std::io;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn sudo_user() -> Option<String> {
    let user = std::env::var("SUDO_USER").ok()?;
    let valid = !user.is_empty()
        && user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    valid.then_some(user)
}

fn user_account(name: &str) -> Option<(PathBuf, libc::uid_t, libc::gid_t)> {
    let name = CString::new(name).ok()?;
    let mut pwd = MaybeUninit::<libc::passwd>::uninit();
    let mut result = std::ptr::null_mut();
    let mut buf = vec![0u8; 4096];
    loop {
        let rc = unsafe {
            libc::getpwnam_r(
                name.as_ptr(),
                pwd.as_mut_ptr(),
                buf.as_mut_ptr().cast(),
                buf.len(),
                &mut result,
            )
        };
        if rc == 0 {
            if result.is_null() {
                return None;
            }
            let pwd = unsafe { pwd.assume_init() };
            if pwd.pw_dir.is_null() {
                return None;
            }
            let bytes = unsafe { CStr::from_ptr(pwd.pw_dir) }.to_bytes();
            return Some((
                PathBuf::from(OsStr::from_bytes(bytes)),
                pwd.pw_uid,
                pwd.pw_gid,
            ));
        }
        if rc != libc::ERANGE || buf.len() >= 1024 * 1024 {
            return None;
        }
        buf.resize(buf.len() * 2, 0);
    }
}

pub(super) fn invoking_user_home() -> Option<PathBuf> {
    if let Some(user) = sudo_user() {
        user_account(&user).map(|(home, _, _)| home)
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

pub(super) fn has_elevation_origin() -> bool {
    sudo_user().is_some()
}

pub(super) fn run_elevated(exe: &Path, argv: &[String], sentinel: &str) -> io::Result<i32> {
    let mut cmd = elevation_command(exe, argv, sentinel);
    let status = cmd.status()?;
    Ok(status.code().unwrap_or(crate::common::EXIT_IO))
}

fn elevation_command(exe: &Path, argv: &[String], sentinel: &str) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.arg(exe);
    for arg in argv {
        if arg != sentinel {
            cmd.arg(arg);
        }
    }
    cmd.arg(sentinel);
    cmd
}

/// Preserve owner-only diagnostic access when a sudo worker creates the file.
pub(super) fn own_invoking_user_file(file: &std::fs::File) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    if unsafe { libc::geteuid() } != 0 {
        return Ok(());
    }
    let Some(user) = sudo_user() else {
        return Ok(());
    };
    let (_, uid, gid) =
        user_account(&user).ok_or_else(|| io::Error::other("invoking account lookup failed"))?;
    if unsafe { libc::fchown(file.as_raw_fd(), uid, gid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn elevation_preserves_arguments_and_appends_one_sentinel() {
        let args = vec![
            "tui".into(),
            "--internal-root".into(),
            "a b".into(),
            "--internal-root".into(),
        ];
        let command = elevation_command(Path::new("/tmp/a b/edpcli"), &args, "--internal-root");
        let actual: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(actual, ["/tmp/a b/edpcli", "tui", "a b", "--internal-root"]);
    }
    #[test]
    fn account_lookup_rejects_missing_or_embedded_nul_names() {
        assert!(user_account("edpcli-no-such-account-99243").is_none());
        assert!(user_account("root\0other").is_none());
        assert!(user_account("root").unwrap().0.is_absolute());
    }
}
