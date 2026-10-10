//! Dedicated full-native-block device port. Never implements SectorDev:
//! 512B protocol projections cannot be promoted into 4Kn writers.
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::time::{Duration, Instant};

use super::native_transaction::NativeBlockDevice;
use crate::domain::hardware::NativeReadGeometry;

pub struct NativeRawBlockDevice {
    path: String,
    file: File,
    geometry: NativeReadGeometry,
    writable: bool,
    reopened_after_lease: bool,
    raw: bool,
}

impl NativeRawBlockDevice {
    /// Starts read-only. Non-device fixtures must be proven ordinary files.
    pub fn open_readonly(path: &str, geometry: NativeReadGeometry) -> io::Result<Self> {
        if !crate::domain::hardware::valid_native_sector_bytes(geometry.logical_sector_bytes)
            || geometry.native_sector_count == 0
            || geometry
                .native_sector_count
                .checked_mul(u64::from(geometry.logical_sector_bytes))
                != Some(geometry.capacity_bytes)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "原生设备逻辑块大小必须为512B正整数倍且总容量与块数匹配",
            ));
        }
        let raw = crate::platform::is_raw_device_path(path);
        let file = File::open(path)?;
        if !raw {
            let metadata = file.metadata()?;
            if !metadata.is_file() || metadata.len() != geometry.capacity_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "普通文件模拟设备必须是精确匹配已验证几何的完整普通文件",
                ));
            }
        }
        Ok(Self {
            path: path.into(),
            file,
            geometry,
            writable: false,
            reopened_after_lease: false,
            raw,
        })
    }

    pub fn geometry(&self) -> NativeReadGeometry {
        self.geometry
    }
    pub fn is_writable(&self) -> bool {
        self.writable
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    fn offset(&self, lba: u64) -> io::Result<u64> {
        self.geometry
            .byte_offset(lba)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
    }

    /// Only the TargetSession type-state transition may call this once the
    /// shared platform lease and unmount have been acquired.
    pub(crate) fn reopen_after_lease(&mut self, wait: Duration) -> io::Result<()> {
        if self.writable {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "原生设备已进入写态",
            ));
        }
        let deadline = Instant::now() + wait;
        let next = loop {
            match OpenOptions::new().read(true).write(true).open(&self.path) {
                Ok(file) => break file,
                Err(error)
                    if crate::platform::raw_busy_error(&error) && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(200))
                }
                Err(error) => return Err(error),
            }
        };
        if !crate::platform::same_open_file_identity(&self.file, &next)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "原生设备重新打开后句柄身份变化",
            ));
        }
        if !self.raw {
            let metadata = next.metadata()?;
            if !metadata.is_file() || metadata.len() != self.geometry.capacity_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "普通文件模拟设备的类型或容量在重新打开后发生变化",
                ));
            }
        }
        self.file = next;
        self.reopened_after_lease = true;
        // Reopening is NOT authorization. The caller must prove identity
        // through a read-only verification view before arming writes.
        self.writable = false;
        Ok(())
    }

    pub(crate) fn verification_view(&mut self) -> NativeVerificationView<'_> {
        NativeVerificationView(self)
    }

    /// The only production caller is TargetSession after the post-reopen
    /// physical identity/geometry checks and verification callback succeed.
    pub(crate) fn arm_verified_write(&mut self) -> io::Result<()> {
        if !self.reopened_after_lease || self.writable {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "原生写句柄尚未安全重开，或已授权",
            ));
        }
        self.writable = true;
        Ok(())
    }

    fn read_from(file: &mut File, offset: u64, bytes: u32) -> io::Result<Vec<u8>> {
        let mut result = vec![0u8; bytes as usize];
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut result)?;
        Ok(result)
    }

    /// Independent read-only handle; detects stale read caches and swapped
    /// device paths. Retains the writable handle for possible rollback.
    fn fresh_read(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        let offset = self.offset(lba)?;
        let mut fresh = File::open(&self.path)?;
        if !crate::platform::same_open_file_identity(&self.file, &fresh)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "原生设备回读时身份变化",
            ));
        }
        Self::read_from(&mut fresh, offset, self.geometry.logical_sector_bytes)
    }
}

/// Read-only verification interface. Cannot write, arm, or reopen the handle.
pub struct NativeVerificationView<'a>(&'a mut NativeRawBlockDevice);
impl NativeVerificationView<'_> {
    pub fn geometry(&self) -> NativeReadGeometry {
        self.0.geometry()
    }
    pub fn read_block(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        self.0.read_block(lba)
    }
    pub fn read_block_fresh(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        self.0.read_block_fresh(lba)
    }
}

impl NativeBlockDevice for NativeRawBlockDevice {
    fn total_sectors(&self) -> u64 {
        self.geometry.native_sector_count
    }
    fn sector_bytes(&self) -> u32 {
        self.geometry.logical_sector_bytes
    }

    fn read_block(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        let offset = self.offset(lba)?;
        Self::read_from(&mut self.file, offset, self.geometry.logical_sector_bytes)
    }

    fn read_block_fresh(&mut self, lba: u64) -> io::Result<Vec<u8>> {
        self.fresh_read(lba)
    }

    fn write_block(&mut self, lba: u64, full_block: &[u8]) -> io::Result<()> {
        if !self.writable {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "未取得原生设备写租约",
            ));
        }
        if full_block.len() != self.geometry.logical_sector_bytes as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "拒绝非完整原生逻辑块写入",
            ));
        }
        let offset = self.offset(lba)?;
        self.file.seek(SeekFrom::Start(offset))?;
        // A short physical write is not a complete atomic native-sector IO.
        // Fail immediately, allowing the transaction to attempt rollback.
        let written = self.file.write(full_block)?;
        if written != full_block.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                format!("原生块写入不完整: {written}/{}B", full_block.len()),
            ));
        }
        Ok(())
    }

    fn sync_blocks(&mut self) -> io::Result<()> {
        if self.raw {
            crate::platform::sync_raw_device(&self.file)
        } else {
            self.file.sync_all()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn fixture() -> (std::path::PathBuf, NativeReadGeometry) {
        let path = std::env::temp_dir().join(format!(
            "edp_native_port_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, vec![0x5a; 4096 * 4]).unwrap();
        (
            path,
            NativeReadGeometry {
                capacity_bytes: 4096 * 4,
                logical_sector_bytes: 4096,
                native_sector_count: 4,
            },
        )
    }

    #[test]
    fn readonly_port_rejects_writes_partial_blocks_and_out_of_range() {
        let (path, geo) = fixture();
        let mut port = NativeRawBlockDevice::open_readonly(path.to_str().unwrap(), geo).unwrap();
        assert_eq!(port.read_block(0).unwrap(), vec![0x5a; 4096]);
        assert_eq!(
            port.write_block(0, &[0x42; 4096]).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(port.read_block(4).is_err());
        port.reopen_after_lease(Duration::ZERO).unwrap();
        assert_eq!(
            port.write_block(1, &[3; 4096]).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        port.arm_verified_write().unwrap();
        assert!(port.write_block(1, &[1; 512]).is_err());
        assert!(port.write_block(4, &[2; 4096]).is_err());
        port.write_block(1, &[3; 4096]).unwrap();
        port.sync_blocks().unwrap();
        assert_eq!(port.read_block_fresh(1).unwrap(), vec![3; 4096]);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn ordinary_file_native_port_wal_commit_and_recovery_all_512_multiples() {
        use crate::diskio::native_journal::execute_native_transaction_with_journal;
        use crate::diskio::native_journal_recovery::{
            inspect_native_journal, recover_native_journal_locked, NativeJournalState,
        };
        use crate::filesystem::{NativeFilesystemWrite, NativeVirtualDiskPlan};
        for bytes in [512u32, 1024, 1536, 2048, 2560, 3072, 4096, 8192] {
            let path = std::env::temp_dir().join(format!(
                "edp-native-regular-file-{}-{}-{}",
                std::process::id(),
                bytes,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let original = vec![0x5au8; bytes as usize * 16];
            fs::write(&path, &original).unwrap();
            let wal = path.with_extension("wal");
            let geo = NativeReadGeometry {
                capacity_bytes: original.len() as u64,
                logical_sector_bytes: bytes,
                native_sector_count: 16,
            };
            let mut dev = NativeRawBlockDevice::open_readonly(path.to_str().unwrap(), geo).unwrap();
            assert_eq!(dev.read_block(7).unwrap(), vec![0x5a; bytes as usize]);
            assert_eq!(
                dev.write_block(7, &vec![0x44; bytes as usize])
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::PermissionDenied
            );
            dev.reopen_after_lease(Duration::ZERO).unwrap();
            assert_eq!(
                dev.verification_view().read_block_fresh(7).unwrap(),
                vec![0x5a; bytes as usize]
            );
            assert!(!dev.is_writable());
            dev.arm_verified_write().unwrap();
            assert_eq!(
                dev.write_block(7, &vec![0x44; bytes as usize - 1])
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
            let plan = NativeVirtualDiskPlan {
                total_sectors: 16,
                sector_bytes: bytes,
                writes: vec![
                    NativeFilesystemWrite {
                        relative_lba: 7,
                        data: vec![0x77; bytes as usize],
                    },
                    NativeFilesystemWrite {
                        relative_lba: 0,
                        data: vec![0x33; bytes as usize],
                    },
                ],
            };
            execute_native_transaction_with_journal(
                &mut dev,
                &plan,
                &wal,
                "virtual-file:authenticated-test",
            )
            .unwrap();
            assert_eq!(
                inspect_native_journal(&wal).unwrap().state,
                NativeJournalState::Committed
            );
            assert_eq!(dev.read_block_fresh(7).unwrap(), vec![0x77; bytes as usize]);
            assert_eq!(dev.read_block_fresh(0).unwrap(), vec![0x33; bytes as usize]);
            // Simulate interruption after commit, before result marker persists.
            let mut walbytes = fs::read(&wal).unwrap();
            let marker = b"COMMITTED_SYNC_AND_READBACK_OK\n";
            assert!(walbytes.ends_with(marker));
            walbytes.truncate(walbytes.len() - marker.len());
            fs::write(&wal, walbytes).unwrap();
            recover_native_journal_locked(&mut dev, &wal, "virtual-file:authenticated-test")
                .unwrap();
            assert_eq!(
                inspect_native_journal(&wal).unwrap().state,
                NativeJournalState::RecoveryVerified
            );
            assert_eq!(dev.read_block_fresh(0).unwrap(), vec![0x5a; bytes as usize]);
            assert_eq!(dev.read_block_fresh(7).unwrap(), vec![0x5a; bytes as usize]);
            assert!(recover_native_journal_locked(
                &mut dev,
                &wal,
                "virtual-file:authenticated-test"
            )
            .is_err());
            drop(dev);
            assert_eq!(fs::read(&path).unwrap(), original);
            fs::remove_file(wal).unwrap();
            fs::remove_file(path).unwrap();
        }
    }
    #[test]
    fn reopen_rejects_swapped_path_and_retains_readonly_handle() {
        let (path, geo) = fixture();
        let mut port = NativeRawBlockDevice::open_readonly(path.to_str().unwrap(), geo).unwrap();
        let old = path.with_extension("old");
        fs::rename(&path, &old).unwrap();
        fs::write(&path, vec![0; 4096 * 4]).unwrap();
        assert!(port.reopen_after_lease(Duration::ZERO).is_err());
        assert!(!port.is_writable());
        assert_eq!(port.read_block(0).unwrap(), vec![0x5a; 4096]);
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(old);
    }
}
