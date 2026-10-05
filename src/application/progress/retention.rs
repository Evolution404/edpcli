//! Bounded in-memory diagnostics; overflow stays available in a per-operation log.
use crate::application::progress::{LogPolicy, ProgressDelivery, ProgressEvent, Severity};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

const EVENT_BUDGET: usize = 256;
const DETAIL_BUDGET: usize = 256 * 1024;

#[derive(Default)]
pub(crate) struct ProgressRetention {
    sent: usize,
    bytes: usize,
    overflow: usize,
    file: Option<File>,
    path: Option<PathBuf>,
    failure: Option<String>,
    latest: Option<ProgressEvent>,
    has_error: bool,
    has_warning: bool,
}

impl ProgressRetention {
    pub(crate) fn retain(&mut self, event: &ProgressEvent) -> bool {
        let bytes = event.detail.as_ref().map_or(0, String::len);
        if self.overflow == 0
            && self.sent < EVENT_BUDGET
            && self.bytes.saturating_add(bytes) <= DETAIL_BUDGET
        {
            self.sent += 1;
            self.bytes += bytes;
            return true;
        }
        self.overflow += 1;
        self.has_error |= event.severity == Severity::Error;
        self.has_warning |= event.severity == Severity::Warning;
        if self.file.is_none() && self.failure.is_none() {
            if let Err(error) = self.open_log() {
                self.failure = Some(error.to_string());
            }
        }
        if let Some(file) = &mut self.file {
            if let Err(error) = writeln!(file, "{event:?}") {
                self.failure = Some(error.to_string());
                self.file = None;
            }
        }
        // Keep the final state even if the diagnostic filesystem fails.
        let mut latest = event.clone();
        if let Some(detail) = &mut latest.detail {
            let mut end = detail.len().min(4096);
            while !detail.is_char_boundary(end) {
                end -= 1;
            }
            detail.truncate(end);
        }
        self.latest = Some(latest);
        false
    }

    fn open_log(&mut self) -> std::io::Result<()> {
        let base = std::env::temp_dir();
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).map_err(std::io::Error::other)?;
        let path = base.join(format!(
            "edpcli-progress-{}-{}.log",
            std::process::id(),
            crate::common::hex_lower(&nonce)
        ));
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path)?;
        if let Err(error) = crate::platform::own_invoking_user_file(&file) {
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        self.file = Some(file);
        self.path = Some(path);
        Ok(())
    }

    pub(crate) fn finish(&mut self) -> Option<ProgressEvent> {
        let mut event = self.latest.take()?;
        if let Some(file) = &self.file {
            if let Err(error) = file.sync_all() {
                self.failure = Some(error.to_string());
            }
        }
        let location = self
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let detail = if let Some(error) = &self.failure {
            format!(
                "诊断日志保存失败: {error}；{} 条后续事件未完整保存；最后诊断: {}",
                self.overflow,
                event.detail.as_deref().unwrap_or("—")
            )
        } else {
            format!(
                "{} 条后续进度/诊断已转存到 {location}；最后诊断: {}",
                self.overflow,
                event.detail.as_deref().unwrap_or("—")
            )
        };
        event.detail = Some(detail);
        event.work = None;
        event.delivery = ProgressDelivery::Reliable;
        event.log_policy = LogPolicy::Append;
        event.severity = if self.has_error || self.failure.is_some() {
            Severity::Error
        } else if self.has_warning {
            Severity::Warning
        } else {
            Severity::Info
        };
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::progress::{Phase, Step};

    #[test]
    fn overflow_is_bounded_and_all_diagnostics_survive_on_disk() {
        let mut retention = ProgressRetention::default();
        let mut delivered = 0;
        for n in 0..10_000 {
            let mut event = ProgressEvent::new(Phase::Transaction, Step::ProtocolWrite, 0, 1);
            event.detail = Some(format!("diagnostic-{n}"));
            event.severity = if n == 9999 {
                Severity::Error
            } else {
                Severity::Warning
            };
            delivered += usize::from(retention.retain(&event));
        }
        assert_eq!(delivered, EVENT_BUDGET);
        let notice = retention.finish().unwrap();
        assert_eq!(notice.severity, Severity::Error);
        let path = retention.path.take().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 10_000 - EVENT_BUDGET);
        assert!(text.contains("diagnostic-256"));
        assert!(text.contains("diagnostic-9999"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn unavailable_log_is_an_explicit_error_not_silent_loss() {
        let mut retention = ProgressRetention {
            sent: EVENT_BUDGET,
            failure: Some("disk full".into()),
            ..Default::default()
        };
        let event = ProgressEvent::new(Phase::Transaction, Step::ProtocolWrite, 0, 1);
        assert!(!retention.retain(&event));
        let notice = retention.finish().unwrap();
        assert_eq!(notice.severity, Severity::Error);
        assert!(notice.detail.unwrap().contains("disk full"));
    }
}
