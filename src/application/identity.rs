//! Shared, read-only identity projection for device and backup workspaces.

use crate::provision::DiskProvisionKind;

use super::BackupWorkspaceItem;

pub const IDENTITY_HEADINGS: [&str; 7] =
    ["容量", "VID:PID", "型号", "onlyid", "姓名", "部门", "盘型"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceIdentity {
    pub size_bytes: Option<u64>,
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub device_id: Option<String>,
    pub onlyid: Option<String>,
    pub user: Option<String>,
    pub dept: Option<String>,
    /// None means the provision kind was not established by a completed probe.
    pub provision_kind: Option<DiskProvisionKind>,
}

impl WorkspaceIdentity {
    pub fn from_device(row: &crate::disk_scan::Row) -> Self {
        Self {
            size_bytes: Some(row.size),
            vid: Some(row.vid.clone()),
            pid: Some(row.pid.clone()),
            device_id: row.device_id.clone(),
            onlyid: row.onlyid.clone(),
            user: row.user.clone(),
            dept: row.dept.clone(),
            provision_kind: (row.proto == "USB"
                && !row.denied
                && row.probe_error.is_none()
                && row.device_id.is_some())
            .then_some(row.provision_kind),
        }
    }

    pub fn from_backup(backup: &BackupWorkspaceItem) -> Self {
        Self {
            size_bytes: backup.size_bytes,
            vid: backup.vid.clone(),
            pid: backup.pid.clone(),
            device_id: backup.device_id.clone(),
            onlyid: backup.onlyid.clone(),
            user: backup.user.clone(),
            dept: backup.dept.clone(),
            provision_kind: (backup.integrity_status
                == crate::diskio::BackupIntegrityStatus::Verified
                && backup.size_ok)
                .then_some(backup.provision_kind),
        }
    }

    pub fn vid_pid(&self) -> String {
        fn hex_word(value: Option<&str>) -> Option<String> {
            let value = value?
                .trim()
                .trim_start_matches("0x")
                .trim_start_matches("0X");
            let parsed = u16::from_str_radix(value, 16).ok()?;
            Some(format!("{parsed:04x}"))
        }
        match (hex_word(self.vid.as_deref()), hex_word(self.pid.as_deref())) {
            (Some(vid), Some(pid)) => format!("{vid}:{pid}"),
            _ => "—".into(),
        }
    }

    pub fn model(&self) -> String {
        let Some(device_id) = self.device_id.as_deref() else {
            return "—".into();
        };
        let mut ven = None;
        let mut prod = None;
        for part in device_id.split('&') {
            ven = ven.or_else(|| part.strip_prefix("ven_"));
            prod = prod.or_else(|| part.strip_prefix("prod_"));
        }
        match (
            ven.filter(|v| !v.is_empty()),
            prod.filter(|v| !v.is_empty()),
        ) {
            (Some(ven), Some(prod)) => format!("{ven}_{prod}"),
            _ => "—".into(),
        }
    }

    pub fn display_cells(&self) -> [String; 7] {
        let known = |value: &Option<String>| {
            value
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or("—")
                .to_string()
        };
        [
            self.size_bytes
                .map(crate::common::fmt_gb)
                .unwrap_or_else(|| "—".into()),
            self.vid_pid(),
            self.model(),
            known(&self.onlyid),
            known(&self.user),
            known(&self.dept),
            self.provision_kind
                .map(|kind| kind.short_name().to_string())
                .unwrap_or_else(|| "—".into()),
        ]
    }

    pub fn search_text(&self) -> String {
        let mut values = self.display_cells().to_vec();
        values.push(self.device_id.clone().unwrap_or_default());
        values.push(self.vid.clone().unwrap_or_default());
        values.push(self.pid.clone().unwrap_or_default());
        values.join(" ")
    }
}
