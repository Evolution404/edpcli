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

/// Validated, byte-accurate geometry for reading native logical disk blocks.
/// This is intentionally separate from the existing 512B physical write grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeReadGeometry {
    pub capacity_bytes: u64,
    pub logical_sector_bytes: u32,
    pub native_sector_count: u64,
}

impl NativeReadGeometry {
    pub fn byte_offset(self, lba: u64) -> Result<u64, &'static str> {
        if lba >= self.native_sector_count {
            return Err("原生 LBA 超出设备容量");
        }
        lba.checked_mul(u64::from(self.logical_sector_bytes))
            .ok_or("原生 LBA 字节偏移溢出")
    }
}

impl ObservedDeviceGeometry {
    /// Accept block geometries for READ ONLY use. Writes are independently gated.
    pub fn native_read_geometry(self) -> Result<NativeReadGeometry, &'static str> {
        let logical = self.logical_sector_bytes.ok_or("设备逻辑扇区大小未知")?;
        if !(512..=65_536).contains(&logical) || !logical.is_power_of_two() {
            return Err("设备逻辑扇区大小无效或不受支持");
        }
        if self.capacity_bytes == 0 || !self.capacity_bytes.is_multiple_of(u64::from(logical)) {
            return Err("设备容量与逻辑扇区大小不一致");
        }
        Ok(NativeReadGeometry {
            capacity_bytes: self.capacity_bytes,
            logical_sector_bytes: logical,
            native_sector_count: self.capacity_bytes / u64::from(logical),
        })
    }

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
    fn native_read_geometry_preserves_block_units_and_rejects_invalid_values() {
        for (logical, blocks) in [(512, 500_000), (1024, 250_000), (4096, 62_500)] {
            let geometry = ObservedDeviceGeometry {
                capacity_bytes: 256_000_000,
                logical_sector_bytes: Some(logical),
                physical_sector_bytes: Some(4096),
            };
            let native = geometry.native_read_geometry().unwrap();
            assert_eq!(native.native_sector_count, blocks);
            assert_eq!(native.byte_offset(11).unwrap(), 11 * u64::from(logical));
            assert!(native.byte_offset(native.native_sector_count).is_err());
            // Preserve 512-only write gate during read-path migration.
            assert_eq!(geometry.writable_protocol_sectors().is_ok(), logical == 512);
        }
        for logical in [None, Some(0), Some(511), Some(513), Some(131_072)] {
            assert!(ObservedDeviceGeometry {
                capacity_bytes: 256_000_000,
                logical_sector_bytes: logical,
                physical_sector_bytes: None,
            }
            .native_read_geometry()
            .is_err());
        }
        let invalid_capacity = ObservedDeviceGeometry {
            capacity_bytes: 4097,
            logical_sector_bytes: Some(4096),
            physical_sector_bytes: Some(4096),
        };
        assert!(invalid_capacity.native_read_geometry().is_err());
    }

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

impl InquiryInfo {
    /// SCSI INQUIRY permits blank Vendor or Product independently; reject only
    /// a completely missing model identity (Revision alone is insufficient).
    pub fn has_model_identity(&self) -> bool {
        !self.vendor.trim().is_empty() || !self.product.trim().is_empty()
    }
}

#[cfg(test)]
mod inquiry_identity_tests {
    use super::InquiryInfo;

    #[test]
    fn blank_vendor_and_nonempty_product_is_valid_but_revision_alone_is_not() {
        for (vendor, product, expected) in [
            ("", "HIKSEMI S500", true),
            ("AIGO", "", true),
            ("AIGO", "U335", true),
            ("", "", false),
            ("   ", "  ", false),
        ] {
            let inquiry = InquiryInfo {
                vendor: vendor.into(),
                product: product.into(),
                revision: "0206".into(),
            };
            assert_eq!(inquiry.has_model_identity(), expected);
        }
    }
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
