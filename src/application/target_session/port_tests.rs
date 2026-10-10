use super::*;
use crate::domain::hardware::ObservedDeviceGeometry;
use std::cell::Cell;
use std::rc::Rc;

struct Facts {
    geometry: Rc<Cell<Option<ObservedDeviceGeometry>>>,
    system: bool,
    usb: bool,
}
impl DeviceObserver for Facts {
    fn is_system_disk(&self, _: u32) -> bool {
        self.system
    }
    fn is_external_usb_whole(&self, _: u32) -> bool {
        self.usb
    }
    fn device_geometry(&self, _: u32) -> Option<ObservedDeviceGeometry> {
        self.geometry.get()
    }
    fn total_sectors(&self, _: u32) -> Option<u64> {
        Some(8)
    }
    fn hardware_probe(&self, _: u32) -> Option<HardwareProbe> {
        None
    }
    fn hardware_serial(&self, _: u32) -> Option<String> {
        None
    }
}
struct Lease(Rc<Cell<bool>>);
impl WriteLease for Lease {}
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
struct Leases(Rc<Cell<bool>>);
impl WriteLeaseProvider for Leases {
    fn acquire_write_lease(&self, _: u32) -> io::Result<Box<dyn WriteLease>> {
        if self.0.replace(true) {
            return Err(io::Error::other("already leased"));
        }
        Ok(Box::new(Lease(self.0.clone())))
    }
}
struct MemoryDev {
    facts: Rc<Cell<Option<ObservedDeviceGeometry>>>,
    swap: bool,
}
impl SectorDev for MemoryDev {
    fn read_sector(&mut self, _: u32) -> io::Result<Vec<u8>> {
        Ok(vec![0; 512])
    }
    fn write_sector(&mut self, _: u32, _: &[u8]) -> io::Result<()> {
        Ok(())
    }
    fn sync(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn reopen_rdwr(&mut self, _: Duration) -> io::Result<()> {
        if self.swap {
            self.facts.set(None);
        }
        Ok(())
    }
}
fn facts() -> Facts {
    Facts {
        geometry: Rc::new(Cell::new(Some(ObservedDeviceGeometry {
            capacity_bytes: 4096,
            logical_sector_bytes: Some(512),
            physical_sector_bytes: Some(4096),
        }))),
        system: false,
        usb: true,
    }
}

#[test]
fn narrow_ports_preserve_guard_geometry_and_exclusive_lease_transitions() {
    let mut facts = facts();
    let leases = Leases(Rc::new(Cell::new(false)));
    facts.system = true;
    assert!(TargetSession::open_with_ports(&facts, &leases, 6).is_err());
    facts.system = false;
    facts.usb = false;
    assert!(TargetSession::open_with_ports(&facts, &leases, 6).is_err());
    facts.usb = true;
    let session = TargetSession::open_with_ports(&facts, &leases, 6).unwrap();
    facts.geometry.set(None);
    assert!(session.prepare_write().is_err());
    assert!(!leases.0.get());

    let facts = self::facts();
    let session = TargetSession::open_with_ports(&facts, &leases, 6)
        .unwrap()
        .prepare_write()
        .unwrap();
    assert!(leases.0.get());
    assert!(TargetSession::open_with_ports(&facts, &leases, 6)
        .unwrap()
        .prepare_write()
        .is_err());
    let mut dev = MemoryDev {
        facts: facts.geometry.clone(),
        swap: false,
    };
    let verified = Cell::new(false);
    let locked = session
        .reopen_and_verify(&mut dev, Duration::ZERO, |_| {
            verified.set(true);
            Ok::<_, ()>(())
        })
        .unwrap();
    assert!(verified.get());
    assert!(leases.0.get());
    drop(locked);
    assert!(!leases.0.get());
}

#[test]
fn swapped_geometry_and_failed_identity_release_lease_without_authorizing_write() {
    for swap in [true, false] {
        let facts = facts();
        let leases = Leases(Rc::new(Cell::new(false)));
        let session = TargetSession::open_with_ports(&facts, &leases, 6)
            .unwrap()
            .prepare_write()
            .unwrap();
        let mut dev = MemoryDev {
            facts: facts.geometry.clone(),
            swap,
        };
        let verified = Cell::new(false);
        let result = session.reopen_and_verify(&mut dev, Duration::ZERO, |_| {
            verified.set(true);
            Err::<(), _>("identity mismatch")
        });
        assert!(if swap {
            matches!(result, Err(ReopenAndVerifyError::Geometry(_)))
        } else {
            matches!(result, Err(ReopenAndVerifyError::Verify(_)))
        });
        drop(result);
        assert_eq!(verified.get(), !swap);
        assert!(!leases.0.get());
    }
}

#[test]
fn native_4kn_typestate_commits_only_after_identity_pin_with_durable_journal() {
    use crate::diskio::{NativeBlockDevice, NativeRawBlockDevice};
    use crate::filesystem::{NativeFilesystemWrite, NativeVirtualDiskPlan};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let facts = facts();
    facts.geometry.set(Some(ObservedDeviceGeometry {
        capacity_bytes: 16 * 4096,
        logical_sector_bytes: Some(4096),
        physical_sector_bytes: Some(4096),
    }));
    let leases = Leases(Rc::new(Cell::new(false)));
    let geo = facts
        .geometry
        .get()
        .unwrap()
        .writable_native_4kn_geometry()
        .unwrap();
    let file = std::env::temp_dir().join(format!(
        "edp-typed-native-{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let journal = file.with_extension("wal");
    fs::write(&file, vec![0xa5; 16 * 4096]).unwrap();
    let mut dev = NativeRawBlockDevice::open_readonly(file.to_str().unwrap(), geo).unwrap();
    assert!(dev.write_block(0, &[7; 4096]).is_err());
    let session = TargetSession::open_with_ports(&facts, &leases, 6)
        .unwrap()
        .prepare_native_4kn_write()
        .unwrap();
    assert!(leases.0.get());
    let mut verified = false;
    let mut locked = session
        .reopen_native_and_verify(&mut dev, Duration::ZERO, |reader| {
            assert_eq!(reader.geometry(), geo);
            assert_eq!(reader.read_block_fresh(0).unwrap(), vec![0xa5; 4096]);
            verified = true;
            Ok::<(), ()>(())
        })
        .unwrap();
    assert!(verified);
    let plan = NativeVirtualDiskPlan {
        sector_bytes: 4096,
        total_sectors: 16,
        writes: vec![
            NativeFilesystemWrite {
                relative_lba: 1,
                data: vec![3; 4096],
            },
            NativeFilesystemWrite {
                relative_lba: 0,
                data: vec![8; 4096],
            },
        ],
    };
    locked
        .execute_native_transaction_with_journal(&plan, &journal, "fixture:disk6:4Kn")
        .unwrap();
    assert!(leases.0.get());
    drop(locked);
    assert!(!leases.0.get());
    assert_eq!(&fs::read(&file).unwrap()[..4096], &[8; 4096]);
    assert!(fs::read(&journal).unwrap().ends_with(
        b"COMMITTED_SYNC_AND_READBACK_OK
"
    ));
    fs::remove_file(file).unwrap();
    fs::remove_file(journal).unwrap();
}

#[test]
fn native_4kn_reopen_pin_failure_leaves_device_unarmed_and_releases_lease() {
    use crate::diskio::{NativeBlockDevice, NativeRawBlockDevice};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let facts = facts();
    facts.geometry.set(Some(ObservedDeviceGeometry {
        capacity_bytes: 16 * 4096,
        logical_sector_bytes: Some(4096),
        physical_sector_bytes: Some(4096),
    }));
    let leases = Leases(Rc::new(Cell::new(false)));
    let geo = facts
        .geometry
        .get()
        .unwrap()
        .writable_native_4kn_geometry()
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "edp-typed-pin-{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, vec![0x5a; 16 * 4096]).unwrap();
    let mut dev = NativeRawBlockDevice::open_readonly(path.to_str().unwrap(), geo).unwrap();
    let session = TargetSession::open_with_ports(&facts, &leases, 6)
        .unwrap()
        .prepare_native_4kn_write()
        .unwrap();
    let result =
        session.reopen_native_and_verify(&mut dev, Duration::ZERO, |_| Err::<(), _>("pin changed"));
    assert!(matches!(result, Err(ReopenAndVerifyError::Verify(_))));
    drop(result);
    assert!(!leases.0.get());
    assert!(!dev.is_writable());
    assert!(dev.write_block(0, &[1; 4096]).is_err());
    fs::remove_file(path).unwrap();
}
