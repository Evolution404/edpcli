use crate::ports::Clock;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_epoch(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
    fn fmt_ts(&self, epoch: i64) -> String {
        let (y, mo, d, h, mi, s) = local_parts(epoch);
        format!("{y:04}{mo:02}{d:02}_{h:02}{mi:02}{s:02}")
    }
    fn fmt_human(&self, epoch: i64) -> String {
        let (y, mo, d, h, mi, _) = local_parts(epoch);
        format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}")
    }
}

type UtcParts = (i64, u32, u32, u32, u32, u32);

fn local_parts(epoch: i64) -> UtcParts {
    let Ok(utc) = time::OffsetDateTime::from_unix_timestamp(epoch) else {
        return utc_parts(epoch);
    };
    let offset = time::UtcOffset::local_offset_at(utc).unwrap_or(time::UtcOffset::UTC);
    let local = utc.to_offset(offset);
    (
        local.year() as i64,
        u8::from(local.month()) as u32,
        local.day() as u32,
        local.hour() as u32,
        local.minute() as u32,
        local.second() as u32,
    )
}

/// Howard Hinnant civil_from_days: epoch → (年,月,日,时,分,秒) (UTC)。
pub(crate) fn utc_parts(epoch: i64) -> UtcParts {
    let days = epoch.div_euclid(86400);
    let secs = epoch.rem_euclid(86400);
    let (h, mi, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32, h as u32, mi as u32, s as u32)
}
