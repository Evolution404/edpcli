//! Bounded, concurrent draining of stdout/stderr and owned process-tree cleanup.
use crate::ports::{CommandCompletion, CommandOutcome};
use std::{
    io::{self, Read},
    process::{Child, ChildStderr, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
const STDERR_LIMIT: usize = 64 * 1024;

#[cfg(windows)]
#[path = "process/windows_job.rs"]
mod windows_job;

trait Pipe: Read {
    #[cfg(unix)]
    fn raw(&self) -> std::os::fd::RawFd;
    #[cfg(windows)]
    fn raw(&self) -> std::os::windows::io::RawHandle;
}
macro_rules! pipe_impl {
    ($kind:ty) => {
        impl Pipe for $kind {
            #[cfg(unix)]
            fn raw(&self) -> std::os::fd::RawFd {
                use std::os::fd::AsRawFd;
                self.as_raw_fd()
            }
            #[cfg(windows)]
            fn raw(&self) -> std::os::windows::io::RawHandle {
                use std::os::windows::io::AsRawHandle;
                self.as_raw_handle()
            }
        }
    };
}
pipe_impl!(ChildStdout);
pipe_impl!(ChildStderr);
fn prepare_pipe(pipe: &impl Pipe) -> io::Result<()> {
    #[cfg(unix)]
    {
        let fd = pipe.raw();
        // SAFETY: the owned pipe is live; only its read flags are changed.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    #[cfg(windows)]
    {
        let _ = pipe;
    }
    Ok(())
}
fn read_available(pipe: &mut impl Pipe, buffer: &mut [u8]) -> io::Result<usize> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::{Foundation::ERROR_BROKEN_PIPE, System::Pipes::PeekNamedPipe};
        let mut available = 0;
        // SAFETY: live owned pipe, valid writable available count.
        if unsafe {
            PeekNamedPipe(
                pipe.raw(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        } == 0
        {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                return Ok(0);
            }
            return Err(error);
        }
        if available == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = buffer.len().min(available as usize);
        pipe.read(&mut buffer[..count])
    }
    #[cfg(unix)]
    {
        pipe.read(buffer)
    }
}
fn terminate(child: &mut Child) {
    #[cfg(unix)]
    {
        // SAFETY: the command was started in its own process group.
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
fn drain(
    pipe: &mut impl Pipe,
    eof: &mut bool,
    output: &mut Vec<u8>,
    limit: usize,
    total: &mut usize,
    truncated: &mut bool,
) -> io::Result<()> {
    if *eof {
        return Ok(());
    }
    let mut buffer = [0; 8192];
    match read_available(pipe, &mut buffer) {
        Ok(0) => *eof = true,
        Ok(count) => {
            *total = total.saturating_add(count);
            let retain = count.min(limit.saturating_sub(output.len()));
            output.extend_from_slice(&buffer[..retain]);
            *truncated |= retain < count;
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) => {}
        Err(error) => return Err(error),
    }
    Ok(())
}
pub(crate) fn run_command(cmd: &[&str], timeout: Duration) -> io::Result<CommandOutcome> {
    let Some(program) = cmd.first().filter(|value| !value.is_empty()) else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "命令不能为空"));
    };
    let start = Instant::now();
    let mut command = Command::new(program);
    command
        .args(&cmd[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_SUSPENDED);
    }
    #[cfg(windows)]
    let job = windows_job::ProcessJob::new()?;
    let mut child = command
        .spawn()
        .map_err(|error| io::Error::new(error.kind(), format!("无法启动 {program}: {error}")))?;
    #[cfg(windows)]
    if let Err(error) = job.assign_and_resume(&child) {
        terminate(&mut child);
        return Err(error);
    }
    let result = (|| {
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("missing stdout pipe"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("missing stderr pipe"))?;
        prepare_pipe(&stdout)?;
        prepare_pipe(&stderr)?;
        let (
            mut out,
            mut err,
            mut out_eof,
            mut err_eof,
            mut out_truncated,
            mut err_truncated,
            mut total,
        ) = (Vec::new(), Vec::new(), false, false, false, false, 0);
        let completion = loop {
            if start.elapsed() >= timeout {
                break CommandCompletion::TimedOut;
            }
            let previous_total = total;
            drain(
                &mut stdout,
                &mut out_eof,
                &mut out,
                OUTPUT_LIMIT,
                &mut total,
                &mut out_truncated,
            )?;
            drain(
                &mut stderr,
                &mut err_eof,
                &mut err,
                STDERR_LIMIT,
                &mut total,
                &mut err_truncated,
            )?;
            if total > OUTPUT_LIMIT {
                break CommandCompletion::OutputLimit;
            }
            if let Some(status) = child.try_wait()? {
                if out_eof && err_eof {
                    break CommandCompletion::Exited {
                        success: status.success(),
                        code: status.code(),
                    };
                }
            }
            // Draining full chunks needs no delay, but always revisits both streams/deadline.
            if total > previous_total {
                continue;
            }
            std::thread::sleep(
                Duration::from_millis(1).min(timeout.saturating_sub(start.elapsed())),
            );
        };
        Ok(CommandOutcome {
            completion,
            elapsed: start.elapsed(),
            stdout: String::from_utf8_lossy(&out).into_owned(),
            stderr: String::from_utf8_lossy(&err).into_owned(),
            stdout_truncated: out_truncated || total > OUTPUT_LIMIT,
            stderr_truncated: err_truncated,
        })
    })();
    #[cfg(windows)]
    drop(job); // KILL_ON_JOB_CLOSE also closes surviving descendants on success.
    terminate(&mut child);
    result
}
pub(crate) fn check_output(cmd: &[&str], timeout: Duration) -> io::Result<String> {
    let outcome = run_command(cmd, timeout)?;
    let (kind, reason) = match outcome.completion {
        CommandCompletion::Exited { success: true, .. } => return Ok(outcome.stdout),
        CommandCompletion::Exited { code, .. } => {
            (io::ErrorKind::Other, format!("退出码 {code:?}"))
        }
        CommandCompletion::TimedOut => (
            io::ErrorKind::TimedOut,
            format!("超时({}ms)", timeout.as_millis()),
        ),
        CommandCompletion::OutputLimit => (
            io::ErrorKind::Other,
            format!("输出超过 {OUTPUT_LIMIT} 字节预算"),
        ),
    };
    let diagnostic: String = outcome.stderr.chars().take(512).collect();
    Err(io::Error::new(
        kind,
        format!("{} {reason}; stderr: {diagnostic}", cmd[0]),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    fn shell(script: &str) -> Vec<&str> {
        vec!["/bin/sh", "-c", script]
    }
    #[cfg(windows)]
    fn shell(script: &str) -> Vec<&str> {
        vec![
            "powershell.exe",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ]
    }
    #[test]
    fn empty_command_is_invalid() {
        assert_eq!(
            run_command(&[], Duration::from_secs(2)).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    #[test]
    fn preserves_bounded_stderr_and_exit_status() {
        #[cfg(unix)]
        let script = "printf abc; printf denied >&2; exit 7";
        #[cfg(windows)]
        let script = "[Console]::Out.Write('abc'); [Console]::Error.Write('denied'); exit 7";
        let outcome = run_command(&shell(script), Duration::from_secs(15)).unwrap();
        assert_eq!(
            outcome.completion,
            CommandCompletion::Exited {
                success: false,
                code: Some(7)
            }
        );
        assert_eq!(outcome.stdout, "abc");
        assert_eq!(outcome.stderr, "denied");
        assert!(check_output(&shell(script), Duration::from_secs(15))
            .unwrap_err()
            .to_string()
            .contains("denied"));
    }
    #[test]
    fn stderr_only_output_cannot_block_or_exceed_retained_diagnostic_budget() {
        #[cfg(unix)]
        let script = "head -c 100000 /dev/zero >&2";
        #[cfg(windows)]
        let script = "[Console]::Error.Write(('x' * 100000))";
        let outcome = run_command(&shell(script), Duration::from_secs(15)).unwrap();
        assert!(matches!(
            outcome.completion,
            CommandCompletion::Exited { success: true, .. }
        ));
        assert_eq!(outcome.stderr.len(), STDERR_LIMIT);
        assert!(outcome.stderr_truncated);
    }
    #[cfg(unix)]
    #[test]
    fn deadline_covers_closed_stdout_and_inherited_pipe() {
        for script in ["exec 1>&-; sleep 2", "sleep 2", "sleep 2 & exit 0"] {
            let start = Instant::now();
            let error = check_output(&shell(script), Duration::from_millis(100)).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            assert!(start.elapsed() < Duration::from_secs(1));
        }
        assert!(check_output(&shell("yes x"), Duration::from_secs(10))
            .unwrap_err()
            .to_string()
            .contains("预算"));
    }
    #[cfg(windows)]
    #[test]
    fn windows_deadline_survives_closed_pipes_and_output_limit_is_typed() {
        let script = r#"Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class Pipes { [DllImport("kernel32.dll")] public static extern IntPtr GetStdHandle(int n); [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h); }'; [Console]::Out.Write('ready'); [Console]::Out.Flush(); [Pipes]::CloseHandle([Pipes]::GetStdHandle(-11)) | Out-Null; [Pipes]::CloseHandle([Pipes]::GetStdHandle(-12)) | Out-Null; Start-Sleep 60"#;
        let outcome = run_command(&shell(script), Duration::from_secs(10)).unwrap();
        assert_eq!(outcome.completion, CommandCompletion::TimedOut);
        assert_eq!(outcome.stdout, "ready");
        let outcome = run_command(
            &shell("[Console]::Out.Write(('x' * 9000000))"),
            Duration::from_secs(20),
        )
        .unwrap();
        assert_eq!(outcome.completion, CommandCompletion::OutputLimit);
        assert!(outcome.stdout_truncated);
    }
    #[cfg(windows)]
    #[test]
    fn windows_job_kills_inherited_child_tree_on_timeout() {
        for tail in ["Start-Sleep 60", "exit 0"] {
            let script = format!("$p = Start-Process powershell.exe -ArgumentList '-NoProfile','-Command','Start-Sleep 60' -PassThru -NoNewWindow; [Console]::Out.WriteLine($p.Id); {tail}");
            let outcome = run_command(&shell(&script), Duration::from_secs(5)).unwrap();
            assert_eq!(outcome.completion, CommandCompletion::TimedOut);
            let pid = outcome.stdout.trim().parse::<u32>().unwrap();
            use windows_sys::Win32::{
                Foundation::CloseHandle,
                System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
            };
            // SAFETY: the PID came from the child fixture; handle is queried and closed.
            let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
            if !handle.is_null() {
                assert_eq!(unsafe { WaitForSingleObject(handle, 2000) }, 0);
                unsafe {
                    CloseHandle(handle);
                }
            }
        }
    }
}
