//! Platform-neutral device observation values. Native adapters populate these models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformKind {
    MacOS,
    Linux,
    Windows,
}

/// Observed device blocks, distinct from EDP's fixed 512-byte address unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedDeviceGeometry {
    pub capacity_bytes: u64,
    pub logical_sector_bytes: Option<u32>,
    pub physical_sector_bytes: Option<u32>,
}

impl ObservedDeviceGeometry {
    pub fn writable_protocol_sectors(self) -> Result<u64, &'static str> {
        if self.logical_sector_bytes != Some(512) {
            return Err("设备逻辑扇区大小未知或不是 512B，禁止写入");
        }
        if self.capacity_bytes == 0 || !self.capacity_bytes.is_multiple_of(512) {
            return Err("设备容量为零或不能按 512B 整除，禁止写入");
        }
        let sectors = self.capacity_bytes / 512;
        if sectors > u64::from(u32::MAX) + 1 {
            return Err("设备容量超出当前 u32 LBA 写入范围，禁止写入");
        }
        Ok(sectors)
    }
}

#[cfg(test)]
mod geometry_tests {
    use super::*;
    #[test]
    fn writable_geometry_distinguishes_protocol_units_and_native_blocks() {
        let mut geometry = ObservedDeviceGeometry {
            capacity_bytes: (u64::from(u32::MAX) + 1) * 512,
            logical_sector_bytes: Some(512),
            physical_sector_bytes: Some(4096),
        };
        assert_eq!(
            geometry.writable_protocol_sectors(),
            Ok(u64::from(u32::MAX) + 1)
        );
        geometry.capacity_bytes += 512;
        assert!(geometry.writable_protocol_sectors().is_err());
        for logical in [None, Some(0), Some(4096)] {
            geometry.capacity_bytes = 4096;
            geometry.logical_sector_bytes = logical;
            assert!(geometry.writable_protocol_sectors().is_err());
        }
        geometry.logical_sector_bytes = Some(512);
        for capacity in [0, 511, 513] {
            geometry.capacity_bytes = capacity;
            assert!(geometry.writable_protocol_sectors().is_err());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NativeTransport {
    Uas,
    Bot,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InquiryInfo {
    pub vendor: String,
    pub product: String,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareProbe {
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub transport: NativeTransport,
    /// Native Windows disk PnP InstanceId when the platform can provide it exactly.
    /// macOS/Linux leave this empty and the identity layer reconstructs the Windows form
    /// from SCSI inquiry data.
    pub windows_pnp_instance_id: Option<String>,
    pub inquiry: Option<InquiryInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtDisk {
    pub n: u32,
    pub size: u64,
    pub vid: String,
    pub pid: String,
    pub proto: String,
}
