use std::ffi::{CStr, CString, OsStr};
use std::fs::File;
use std::io;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::path::Path;
use std::process::Command;

use super::{HardwareProbe, PlatformKind};

pub(super) const fn kind() -> PlatformKind {
    PlatformKind::MacOS
}

pub(super) fn raw_disk_path(disk: u32) -> String {
    format!("/dev/rdisk{disk}")
}

pub(super) fn is_elevated() -> bool {
    unsafe { libc::geteuid() == 0 }
}

fn sudo_user() -> Option<String> {
    let user = std::env::var("SUDO_USER").ok()?;
    let valid = !user.is_empty()
        && user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    valid.then_some(user)
}

fn user_home(name: &str) -> Option<PathBuf> {
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
            return Some(PathBuf::from(OsStr::from_bytes(bytes)));
        }
        if rc != libc::ERANGE || buf.len() >= 1024 * 1024 {
            return None;
        }
        buf.resize(buf.len() * 2, 0);
    }
}

pub(super) fn invoking_user_home() -> Option<PathBuf> {
    if let Some(user) = sudo_user() {
        user_home(&user)
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

pub(super) fn is_raw_device_path(path: &str) -> bool {
    path.starts_with("/dev/rdisk")
}

pub(super) fn raw_busy_error(error: &io::Error) -> bool {
    error.raw_os_error() == Some(16)
}

pub(super) fn sync_raw_device(file: &File) -> io::Result<()> {
    use std::os::raw::c_ulong;
    extern "C" {
        fn ioctl(fd: i32, request: c_ulong, ...) -> i32;
    }
    const DKIOCSYNCHRONIZECACHE: c_ulong = 0x2000_6416;
    let rc = unsafe { ioctl(file.as_raw_fd(), DKIOCSYNCHRONIZECACHE) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub(super) fn hardware_probe(disk: u32) -> Option<HardwareProbe> {
    crate::native_probe::probe_disk(disk)
}

pub(super) const fn elevation_label() -> &'static str {
    "sudo"
}

pub(super) fn run_elevated(exe: &Path, argv: &[String], sentinel: &str) -> io::Result<i32> {
    let mut cmd = Command::new("sudo");
    cmd.arg(exe);
    for arg in argv {
        if arg != sentinel {
            cmd.arg(arg);
        }
    }
    cmd.arg(sentinel);
    let status = cmd.status()?;
    Ok(status.code().unwrap_or(crate::common::EXIT_IO))
}
