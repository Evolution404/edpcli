//! Platform command fixtures for cross-layer refresh tests.
use crate::ports::CmdRunner;
use std::io;
pub(crate) struct MultiUsbRunner;
impl CmdRunner for MultiUsbRunner {
    fn check_output(&self, cmd: &[&str], _: std::time::Duration) -> io::Result<String> {
        if cmd == ["diskutil", "list", "-plist"] {
            return Ok("<plist><dict><key>AllDisks</key><array><string>disk4</string><string>disk6</string></array></dict></plist>".into());
        }
        if matches!(cmd, ["diskutil", "info", "-plist", "disk4" | "disk6"]) {
            return Ok("<plist><dict><key>DeviceBlockSize</key><integer>512</integer><key>TotalSize</key><integer>16777216</integer><key>WholeDisk</key><true/><key>Internal</key><false/><key>BusProtocol</key><string>USB</string></dict></plist>".into());
        }
        Err(io::Error::other("not a fixture command"))
    }
    fn hardware_probe(&self, _: u32) -> Option<crate::platform::HardwareProbe> {
        Some(crate::platform::HardwareProbe {
            vid: Some(0x2bdf),
            pid: Some(0x0300),
            transport: crate::platform::NativeTransport::Bot,
            windows_pnp_instance_id: None,
            inquiry: Some(crate::platform::InquiryInfo {
                vendor: "TEST".into(),
                product: "USB".into(),
                revision: "1.0".into(),
            }),
        })
    }
}
