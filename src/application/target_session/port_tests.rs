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
