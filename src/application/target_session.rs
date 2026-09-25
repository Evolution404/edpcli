//! Type-state raw-device safety transition shared by write application flows.
//!
//! A session starts read-only after the common USB/system-disk guard. Entering
//! `PreparedWrite` owns the platform write guard (unmount/lock), and entering
//! `WriteLocked` additionally proves the raw handle was reopened and the
//! caller-supplied pre-write identity check passed. The guard remains alive
//! until the locked session is dropped.

use std::io;
use std::time::Duration;

use crate::common::{EdpCliError, EdpCliResult};
use crate::diskio::SectorDev;
use crate::platform::{HardwareProbe, WriteGuard};
use crate::sysinfo::{self, CmdRunner};

use super::device::guard_usb_disk;

pub struct ReadOnly;

pub struct PreparedWrite {
    _guard: WriteGuard,
}

pub struct WriteLocked {
    _guard: WriteGuard,
}

pub struct TargetSession<'a, State> {
    runner: &'a dyn CmdRunner,
    disk: u32,
    state: State,
}

#[derive(Debug)]
pub enum ReopenAndVerifyError {
    Reopen(io::Error),
    Verify(EdpCliError),
}

impl<'a> TargetSession<'a, ReadOnly> {
    pub fn open_usb(runner: &'a dyn CmdRunner, disk: u32) -> EdpCliResult<Self> {
        guard_usb_disk(runner, disk)?;
        Ok(Self {
            runner,
            disk,
            state: ReadOnly,
        })
    }

    pub fn prepare_write(self) -> io::Result<TargetSession<'a, PreparedWrite>> {
        let guard = sysinfo::prepare_write(self.runner, self.disk)?;
        Ok(TargetSession {
            runner: self.runner,
            disk: self.disk,
            state: PreparedWrite { _guard: guard },
        })
    }
}

impl<'a> TargetSession<'a, PreparedWrite> {
    pub fn reopen_and_verify(
        self,
        dev: &mut dyn SectorDev,
        wait: Duration,
        verify: impl FnOnce(&mut dyn SectorDev) -> EdpCliResult<()>,
    ) -> Result<TargetSession<'a, WriteLocked>, ReopenAndVerifyError> {
        dev.reopen_rdwr(wait)
            .map_err(ReopenAndVerifyError::Reopen)?;
        verify(dev).map_err(ReopenAndVerifyError::Verify)?;
        let PreparedWrite { _guard } = self.state;
        Ok(TargetSession {
            runner: self.runner,
            disk: self.disk,
            state: WriteLocked { _guard },
        })
    }
}

impl<State> TargetSession<'_, State> {
    pub fn disk(&self) -> u32 {
        self.disk
    }

    pub fn total_sectors(&self) -> Option<u64> {
        sysinfo::disk_total_sectors(self.runner, self.disk)
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
