//! macOS IOKit 原生硬件探测。
//!
//! 从 `diskN` 对应的 IOMedia 开始沿 `IOService` 父链读取 USB VID/PID、
//! UAS/BOT 传输类和 SCSI INQUIRY 属性。这里不启动 `ioreg` 子进程。

use std::collections::BTreeMap;
use std::ffi::CString;

use objc2_core_foundation::{CFDictionary, CFNumber, CFNumberType, CFRetained, CFString};
use objc2_io_kit::{
    kIOMainPortDefault, kIOReturnSuccess, kIOServicePlane, IOBSDNameMatching,
    IOObjectCopyClass, IOObjectRelease, IORegistryEntryCreateCFProperty,
    IORegistryEntryGetParentEntry, IOServiceGetMatchingService, IO_OBJECT_NULL,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub inquiry: Option<InquiryInfo>,
}

#[derive(Debug, Clone, Default)]
struct NodeSnapshot {
    class_name: String,
    strings: BTreeMap<String, String>,
    integers: BTreeMap<String, i64>,
}

struct IoObject(u32);

impl Drop for IoObject {
    fn drop(&mut self) {
        if self.0 != IO_OBJECT_NULL {
            let _ = IOObjectRelease(self.0);
        }
    }
}

impl IoObject {
    fn class_name(&self) -> Option<String> {
        IOObjectCopyClass(self.0).map(|value| value.to_string())
    }

    fn property_string(&self, key: &str) -> Option<String> {
        let key = CFString::from_str(key);
        let value = unsafe { IORegistryEntryCreateCFProperty(self.0, Some(&key), None, 0) }?;
        value.downcast_ref::<CFString>().map(ToString::to_string)
    }

    fn property_i64(&self, key: &str) -> Option<i64> {
        let key = CFString::from_str(key);
        let value = unsafe { IORegistryEntryCreateCFProperty(self.0, Some(&key), None, 0) }?;
        let number = value.downcast_ref::<CFNumber>()?;
        let mut out = 0i64;
        let ok = unsafe {
            number.value(
                CFNumberType::SInt64Type,
                std::ptr::addr_of_mut!(out).cast(),
            )
        };
        ok.then_some(out)
    }

    fn parent(&self) -> Option<Self> {
        let mut parent = IO_OBJECT_NULL;
        let rc = unsafe {
            IORegistryEntryGetParentEntry(
                self.0,
                kIOServicePlane.as_ptr().cast_mut().cast(),
                &mut parent,
            )
        };
        (rc == kIOReturnSuccess && parent != IO_OBJECT_NULL).then_some(Self(parent))
    }

    fn snapshot(&self) -> NodeSnapshot {
        const STRING_KEYS: [&str; 3] = [
            "Vendor Identification",
            "Product Identification",
            "Product Revision Level",
        ];
        const INTEGER_KEYS: [&str; 2] = ["idVendor", "idProduct"];

        let strings = STRING_KEYS
            .into_iter()
            .filter_map(|key| self.property_string(key).map(|value| (key.to_string(), value)))
            .collect();
        let integers = INTEGER_KEYS
            .into_iter()
            .filter_map(|key| self.property_i64(key).map(|value| (key.to_string(), value)))
            .collect();
        NodeSnapshot {
            class_name: self.class_name().unwrap_or_default(),
            strings,
            integers,
        }
    }
}

fn summarize_nodes(nodes: &[NodeSnapshot]) -> HardwareProbe {
    let mut vid = None;
    let mut pid = None;
    let mut has_uas = false;
    let mut has_bot = false;
    let mut inquiry_by_class: BTreeMap<&str, InquiryInfo> = BTreeMap::new();

    for node in nodes {
        match node.class_name.as_str() {
            "IOUSBHostDevice" => {
                vid = node
                    .integers
                    .get("idVendor")
                    .and_then(|value| u16::try_from(*value).ok())
                    .or(vid);
                pid = node
                    .integers
                    .get("idProduct")
                    .and_then(|value| u16::try_from(*value).ok())
                    .or(pid);
            }
            "IOUSBMassStorageUASDriver" => has_uas = true,
            "IOUSBMassStorageInterfaceNub" | "IOUSBMassStorageDriver" => has_bot = true,
            "IOSCSITargetDevice" | "IOSCSILogicalUnitNub" | "IOSCSIPeripheralDeviceNub" => {
                if let Some(vendor) = node
                    .strings
                    .get("Vendor Identification")
                    .filter(|value| !value.is_empty())
                {
                    inquiry_by_class.insert(
                        node.class_name.as_str(),
                        InquiryInfo {
                            vendor: vendor.clone(),
                            product: node
                                .strings
                                .get("Product Identification")
                                .cloned()
                                .unwrap_or_default(),
                            revision: node
                                .strings
                                .get("Product Revision Level")
                                .cloned()
                                .unwrap_or_default(),
                        },
                    );
                }
            }
            _ => {}
        }
    }

    let inquiry = [
        "IOSCSITargetDevice",
        "IOSCSILogicalUnitNub",
        "IOSCSIPeripheralDeviceNub",
    ]
    .into_iter()
    .find_map(|class| inquiry_by_class.get(class).cloned());
    let transport = if has_uas {
        NativeTransport::Uas
    } else if has_bot {
        NativeTransport::Bot
    } else {
        NativeTransport::Unknown
    };
    HardwareProbe {
        vid,
        pid,
        transport,
        inquiry,
    }
}

/// 查询一个 BSD whole-disk 的 IOKit 祖先链。找不到设备返回 `None`。
pub fn probe_disk(disk: u32) -> Option<HardwareProbe> {
    let name = CString::new(format!("disk{disk}")).ok()?;
    let main_port = unsafe { kIOMainPortDefault };
    let matching = unsafe { IOBSDNameMatching(main_port, 0, name.as_ptr()) }?;
    let matching: CFRetained<CFDictionary> = CFRetained::from(&*matching);
    let service = unsafe { IOServiceGetMatchingService(main_port, Some(matching)) };
    if service == IO_OBJECT_NULL {
        return None;
    }

    let mut current = Some(IoObject(service));
    let mut nodes = Vec::new();
    // 正常 IOService 祖先深度远低于 64；上限防止异常 registry 形成无界循环。
    for _ in 0..64 {
        let Some(object) = current.take() else {
            break;
        };
        nodes.push(object.snapshot());
        current = object.parent();
    }
    Some(summarize_nodes(&nodes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(class_name: &str) -> NodeSnapshot {
        NodeSnapshot {
            class_name: class_name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn summary_prefers_uas_and_target_inquiry() {
        let mut usb = node("IOUSBHostDevice");
        usb.integers.insert("idVendor".into(), 0x3535);
        usb.integers.insert("idProduct".into(), 0x6300);
        let mut target = node("IOSCSITargetDevice");
        target.strings.insert("Vendor Identification".into(), "AIGO    ".into());
        target.strings.insert("Product Identification".into(), "U335".into());
        target.strings.insert("Product Revision Level".into(), "PMAP".into());
        let mut lun = node("IOSCSILogicalUnitNub");
        lun.strings.insert("Vendor Identification".into(), "WRONG".into());

        let summary = summarize_nodes(&[
            usb,
            node("IOUSBMassStorageInterfaceNub"),
            node("IOUSBMassStorageUASDriver"),
            lun,
            target,
        ]);
        assert_eq!(summary.vid, Some(0x3535));
        assert_eq!(summary.pid, Some(0x6300));
        assert_eq!(summary.transport, NativeTransport::Uas);
        assert_eq!(summary.inquiry.unwrap().vendor, "AIGO    ");
    }

    #[test]
    fn nonexistent_disk_is_a_clean_miss() {
        assert!(probe_disk(u32::MAX).is_none());
    }
}
