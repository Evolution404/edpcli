//! Type-state raw-device safety transition shared by write application flows.
//!
//! A session starts read-only after the common USB/system-disk guard. Entering
//! `PreparedWrite` owns the platform write guard (unmount/lock), and entering
//! `WriteLocked` additionally proves the raw handle was reopened and the
//! caller-supplied pre-write identity check passed. The guard remains alive
//! until the locked session is dropped.

use std::io;
use std::time::Duration;

use crate::common::{EdpCliError, EdpCliResult, EXIT_TARGET};
use crate::platform::system;
use crate::platform::{HardwareProbe, WriteGuard};
use crate::ports::{CmdRunner, SectorDev};

use super::device::guard_usb_disk;

/// Read-only sessions have no transaction capability.
/// ```compile_fail
/// use edpcli::application::target_session::{TargetSession, ReadOnly};
/// use edpcli::diskio::WriteTransactionPlan;
/// fn write(session: &mut TargetSession<'_, ReadOnly>, plan: &WriteTransactionPlan) {
///     session.execute_transaction(plan).unwrap();
/// }
/// ```
pub struct ReadOnly;

pub struct PreparedWrite {
    _guard: WriteGuard,
}

/// The device remains exclusively borrowed until its locked session is released.
/// ```compile_fail
/// use edpcli::application::target_session::{TargetSession, PreparedWrite};
/// use edpcli::ports::SectorDev;
/// use std::time::Duration;
/// fn reuse(session: TargetSession<'_, PreparedWrite>, dev: &mut dyn SectorDev) {
///     let mut locked = session.reopen_and_verify(dev, Duration::ZERO,
///         |_| Ok::<(), ()>(())).unwrap();
///     dev.sync().unwrap();
///     let _ = &mut locked;
/// }
/// ```
pub struct WriteLocked<'d> {
    _guard: WriteGuard,
    dev: &'d mut dyn SectorDev,
}

// Explicit drop checking keeps the exclusive device borrow until guard release,
// even when the caller's last transaction use precedes the end of the scope.
impl Drop for WriteLocked<'_> {
    fn drop(&mut self) {}
}

pub struct TargetSession<'a, State> {
    runner: &'a dyn CmdRunner,
    disk: u32,
    state: State,
    geometry: Option<crate::domain::hardware::ObservedDeviceGeometry>,
}

#[derive(Debug)]
pub enum ReopenAndVerifyError<E = EdpCliError> {
    Reopen(io::Error),
    Verify(E),
    Geometry(EdpCliError),
}

impl<'a> TargetSession<'a, ReadOnly> {
    pub fn open_usb(runner: &'a dyn CmdRunner, disk: u32) -> EdpCliResult<Self> {
        guard_usb_disk(runner, disk)?;
        Ok(Self {
            runner,
            disk,
            state: ReadOnly,
            geometry: system::device_geometry(runner, disk),
        })
    }

    pub fn prepare_write(self) -> io::Result<TargetSession<'a, PreparedWrite>> {
        self.writable_geometry()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.msg))?;
        if system::device_geometry(self.runner, self.disk) != self.geometry {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "写准备前设备几何发生变化",
            ));
        }
        let guard = system::prepare_write(self.runner, self.disk)?;
        Ok(TargetSession {
            runner: self.runner,
            disk: self.disk,
            state: PreparedWrite { _guard: guard },
            geometry: self.geometry,
        })
    }
}

impl<'a> TargetSession<'a, PreparedWrite> {
    pub fn reopen_and_verify<'d, E>(
        self,
        dev: &'d mut dyn SectorDev,
        wait: Duration,
        verify: impl FnOnce(&mut dyn SectorDev) -> Result<(), E>,
    ) -> Result<TargetSession<'a, WriteLocked<'d>>, ReopenAndVerifyError<E>> {
        dev.reopen_rdwr(wait)
            .map_err(ReopenAndVerifyError::Reopen)?;
        if system::device_geometry(self.runner, self.disk) != self.geometry {
            return Err(ReopenAndVerifyError::Geometry(EdpCliError::new(
                EXIT_TARGET,
                "重开后设备几何发生变化，禁止写入",
            )));
        }
        verify(dev).map_err(ReopenAndVerifyError::Verify)?;
        let PreparedWrite { _guard } = self.state;
        Ok(TargetSession {
            runner: self.runner,
            disk: self.disk,
            state: WriteLocked { _guard, dev },
            geometry: self.geometry,
        })
    }
}

impl<State> TargetSession<'_, State> {
    pub fn writable_geometry(
        &self,
    ) -> EdpCliResult<crate::domain::hardware::ObservedDeviceGeometry> {
        let geometry = self
            .geometry
            .ok_or_else(|| EdpCliError::new(EXIT_TARGET, "无法观察设备几何，禁止写入"))?;
        geometry
            .writable_protocol_sectors()
            .map_err(|message| EdpCliError::new(EXIT_TARGET, message))?;
        Ok(geometry)
    }
    pub fn disk(&self) -> u32 {
        self.disk
    }

    pub fn total_sectors(&self) -> Option<u64> {
        system::disk_total_sectors(self.runner, self.disk)
    }

    pub fn hardware_probe(&self) -> Option<HardwareProbe> {
        self.runner
            .hardware_probe(self.disk)
            .or_else(|| crate::platform::fallback_hardware_probe(self.runner, self.disk))
    }

    pub fn hardware_serial(&self) -> Option<String> {
        self.runner.hardware_serial(self.disk)
    }
}

impl TargetSession<'_, WriteLocked<'_>> {
    pub(crate) fn device(&mut self) -> &mut dyn SectorDev {
        self.state.dev
    }

    /// Execute only against the device borrowed by this locked session.
    pub fn execute_transaction(
        &mut self,
        plan: &crate::diskio::WriteTransactionPlan,
    ) -> EdpCliResult<()> {
        crate::diskio::execute_write_transaction(self.state.dev, plan)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::domain::hardware::ObservedDeviceGeometry;
    use std::cell::Cell;

    struct Runner {
        geometry: Cell<Option<ObservedDeviceGeometry>>,
        unmounts: Cell<usize>,
    }
    impl CmdRunner for Runner {
        fn check_output(&self, cmd: &[&str], _: Duration) -> io::Result<String> {
            if cmd == ["diskutil", "list", "-plist"] {
                return Ok("<plist><dict><key>AllDisks</key><array><string>disk6</string></array></dict></plist>".into());
            }
            if cmd == ["diskutil", "info", "-plist", "/"] {
                return Ok(
                    "<plist><dict><key>ParentWholeDisk</key><string>disk0</string></dict></plist>"
                        .into(),
                );
            }
            if cmd == ["diskutil", "info", "-plist", "disk6"] {
                return Ok("<plist><dict><key>TotalSize</key><integer>4096</integer><key>WholeDisk</key><true/><key>Internal</key><false/><key>BusProtocol</key><string>USB</string></dict></plist>".into());
            }
            if cmd == ["diskutil", "unmountDisk", "force", "disk6"] {
                self.unmounts.set(self.unmounts.get() + 1);
                return Ok(String::new());
            }
            Err(io::Error::other("unexpected command"))
        }
        fn device_geometry(&self, _: u32) -> Option<ObservedDeviceGeometry> {
            self.geometry.get()
        }
    }
    struct Dev<'a> {
        runner: &'a Runner,
        change_on_reopen: bool,
        writes: usize,
    }
    impl SectorDev for Dev<'_> {
        fn read_sector(&mut self, _: u32) -> io::Result<Vec<u8>> {
            Ok(vec![0; 512])
        }
        fn write_sector(&mut self, _: u32, _: &[u8]) -> io::Result<()> {
            self.writes += 1;
            Ok(())
        }
        fn reopen_rdwr(&mut self, _: Duration) -> io::Result<()> {
            if self.change_on_reopen {
                let mut geometry = self.runner.geometry.get().unwrap();
                geometry.logical_sector_bytes = Some(4096);
                self.runner.geometry.set(Some(geometry));
            }
            Ok(())
        }

        // Explicit test device synchronization contract.
        fn sync(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn geometry(logical: Option<u32>, capacity: u64) -> ObservedDeviceGeometry {
        ObservedDeviceGeometry {
            capacity_bytes: capacity,
            logical_sector_bytes: logical,
            physical_sector_bytes: Some(4096),
        }
    }
    #[test]
    fn unsupported_geometry_never_unmounts_or_enters_write_transition() {
        for value in [
            None,
            Some(geometry(None, 4096)),
            Some(geometry(Some(4096), 4096)),
            Some(geometry(Some(512), 4097)),
        ] {
            let runner = Runner {
                geometry: Cell::new(value),
                unmounts: Cell::new(0),
            };
            let session = TargetSession::<ReadOnly>::open_usb(&runner, 6).unwrap();
            assert!(session.prepare_write().is_err());
            assert_eq!(runner.unmounts.get(), 0);
        }
    }
    #[test]
    fn geometry_change_after_reopen_cannot_reach_writes_or_identity_authorization() {
        let runner = Runner {
            geometry: Cell::new(Some(geometry(Some(512), 4096))),
            unmounts: Cell::new(0),
        };
        let mut dev = Dev {
            runner: &runner,
            change_on_reopen: true,
            writes: 0,
        };
        let session = TargetSession::<ReadOnly>::open_usb(&runner, 6)
            .unwrap()
            .prepare_write()
            .unwrap();
        let verified = Cell::new(false);
        let result = session.reopen_and_verify(&mut dev, Duration::ZERO, |_| {
            verified.set(true);
            Ok::<_, EdpCliError>(())
        });
        assert!(matches!(result, Err(ReopenAndVerifyError::Geometry(_))));
        drop(result);
        assert!(!verified.get());
        assert_eq!(dev.writes, 0);
        assert_eq!(runner.unmounts.get(), 1);
    }
}
