//! Bounded command execution, including the interval after stdout closes.
use std::{
    io::{self, Read},
    process::{Child, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

#[cfg(unix)]
fn prepare_pipe(pipe: &ChildStdout) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let fd = pipe.as_raw_fd();
    // SAFETY: fd is an owned live stdout pipe; these flags only alter its read mode.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
#[cfg(windows)]
fn prepare_pipe(_: &ChildStdout) -> io::Result<()> {
    Ok(())
}

fn read_available(pipe: &mut ChildStdout, buffer: &mut [u8]) -> io::Result<usize> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::ERROR_BROKEN_PIPE, System::Pipes::PeekNamedPipe};
        let mut available = 0;
        // SAFETY: valid child stdout handle; no peek buffer, available is writable.
        if unsafe {
            PeekNamedPipe(
                pipe.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        } == 0
        {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                return Ok(0);
            }
            return Err(err);
        }
        if available == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = buffer.len().min(available as usize);
        pipe.read(&mut buffer[..count])
    }
    #[cfg(not(windows))]
    {
        pipe.read(buffer)
    }
}

fn terminate(child: &mut Child) {
    #[cfg(unix)]
    {
        // SAFETY: child was started in its own process group; never the caller's group.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(1) {
        if !matches!(child.try_wait(), Ok(None)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(crate) fn check_output(cmd: &[&str], timeout: Duration) -> io::Result<String> {
    let Some(program) = cmd.first().filter(|program| !program.is_empty()) else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "命令不能为空"));
    };
    let start = Instant::now();
    let mut command = Command::new(program);
    command
        .args(&cmd[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| io::Error::new(error.kind(), format!("无法启动 {program}: {error}")))?;
    let result = (|| {
        let mut pipe = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("未提供 stdout 管道"))?;
        prepare_pipe(&pipe)?;
        let mut output = Vec::new();
        let mut buffer = [0_u8; 8192];
        let mut eof = false;
        loop {
            if start.elapsed() >= timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("{program} 超时({}ms)", timeout.as_millis()),
                ));
            }
            if !eof {
                match read_available(&mut pipe, &mut buffer) {
                    Ok(0) => eof = true,
                    Ok(count) => {
                        if output.len().saturating_add(count) > OUTPUT_LIMIT {
                            return Err(io::Error::other(format!(
                                "{program} 输出超过 {OUTPUT_LIMIT} 字节预算"
                            )));
                        }
                        output.extend_from_slice(&buffer[..count]);
                        continue;
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) => {}
                    Err(error) => return Err(error),
                }
            }
            if let Some(status) = child.try_wait()? {
                if !status.success() {
                    return Err(io::Error::other(format!(
                        "{program} 退出码 {:?}",
                        status.code()
                    )));
                }
                if eof {
                    return Ok(String::from_utf8_lossy(&output).into_owned());
                }
            }
            std::thread::sleep(
                Duration::from_millis(5).min(timeout.saturating_sub(start.elapsed())),
            );
        }
    })();
    if result.is_err() {
        terminate(&mut child);
    }
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn deadline_covers_closed_stdout_and_inherited_pipe() {
        for script in ["exec 1>&-; sleep 2", "sleep 2", "sleep 2 & exit 0"] {
            let start = Instant::now();
            let error =
                check_output(&["/bin/sh", "-c", script], Duration::from_millis(100)).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            assert!(start.elapsed() < Duration::from_secs(1));
        }
    }
    #[test]
    fn validates_output_status_and_budget() {
        assert_eq!(
            check_output(&["/bin/sh", "-c", "printf abc"], Duration::from_secs(2)).unwrap(),
            "abc"
        );
        assert!(check_output(&["/bin/sh", "-c", "exit 7"], Duration::from_secs(2)).is_err());
        assert_eq!(
            check_output(&[], Duration::from_secs(2))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(
            check_output(&["/bin/sh", "-c", "yes x"], Duration::from_secs(2))
                .unwrap_err()
                .to_string()
                .contains("预算")
        );
    }
}
