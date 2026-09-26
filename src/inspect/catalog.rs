use super::*;
use crate::diskio::BackupMeta;

impl InspectMeta {
    pub fn from_backup_meta(meta: &BackupMeta) -> Self {
        Self {
            device_id: Some(meta.device_id.clone()),
            vid: Some(meta.vid.clone()),
            pid: Some(meta.pid.clone()),
            size_bytes: meta.secs.and_then(|s| s.checked_mul(SECTOR as u64)),
            onlyid: meta.onlyid.clone(),
        }
    }
}
