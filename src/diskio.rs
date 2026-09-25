//! 真盘 IO、原子写入、备份/还原与快照读取。
//! 扇区设备抽象为 SectorDev — 这就是 Python 版 `_raw_path` 的 mock 点(升为参数)。

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::common::{
    EdpCliError, EdpCliResult, EXIT_BACKUP, EXIT_INTERMEDIATE, EXIT_IO, EXIT_ROLLED_BACK, SECTOR,
};
use crate::sectors::looks_nopwd;
use crate::sha256::sha256_hex;

mod backup_catalog;
mod backup_config;
mod backup_create;
mod device;
mod transaction;

pub use backup_catalog::*;
pub use backup_config::*;
pub use backup_create::*;
pub use device::*;
pub use transaction::*;

#[cfg(test)]
fn utc_parts(epoch: i64) -> (i64, u32, u32, u32, u32, u32) {
    backup_config::utc_parts(epoch)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CountingSectorDev {
        reads: usize,
    }

    impl SectorDev for CountingSectorDev {
        fn reopen_rdwr(&mut self, _wait: Duration) -> io::Result<()> {
            Err(io::Error::other("not used"))
        }

        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            self.reads += 1;
            Ok(vec![lba as u8; SECTOR])
        }

        fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
            Err(io::Error::other("not used"))
        }

        fn sync(&mut self) -> io::Result<()> {
            Err(io::Error::other("not used"))
        }
    }

    #[test]
    fn inspect_u64_reader_accepts_last_sector_rejects_eof_and_offset_overflow() {
        let path = std::env::temp_dir().join(format!(
            "edpcli_inspect_u64_{}_{}.bin",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut bytes = vec![0u8; 2 * SECTOR];
        bytes[SECTOR..].fill(0x5a);
        fs::write(&path, bytes).unwrap();

        let mut dev = FileDev::open_rdonly(path.to_str().unwrap()).unwrap();
        assert_eq!(dev.read_sector_u64(1).unwrap(), vec![0x5a; SECTOR]);

        let eof = dev.read_sector_u64(2).unwrap_err();
        assert_eq!(eof.kind(), io::ErrorKind::UnexpectedEof);
        assert!(eof.to_string().contains("LBA2"));

        let overflow = dev.read_sector_u64(u64::MAX).unwrap_err();
        assert_eq!(overflow.kind(), io::ErrorKind::InvalidInput);
        assert!(overflow.to_string().contains("偏移溢出"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn sector_read_cache_reuses_info_prefetch_during_summary() {
        let mut dev = CountingSectorDev { reads: 0 };
        {
            let mut cache = SectorReadCache::new(&mut dev);
            // physical info 会先为 device_id / onlyid 预读 7、4，随后 summary
            // 请求 0、4、6、7、8、11、12。预读扇区不得再次访问底层。
            assert_eq!(cache.read_sector(7).unwrap(), vec![7u8; SECTOR]);
            assert_eq!(cache.read_sector(4).unwrap(), vec![4u8; SECTOR]);
            for lba in [0, 4, 6, 7, 8, 11, 12] {
                assert_eq!(cache.read_sector(lba).unwrap(), vec![lba as u8; SECTOR]);
            }
        }
        assert_eq!(
            dev.reads, 7,
            "info 预读 + summary 共涉及 7 个唯一 LBA，不应重复访问 LBA4/LBA7"
        );
    }

    #[test]
    fn read_only_disk_pool_opens_each_disk_once() {
        use std::cell::Cell;

        struct DiskDev(u32);
        impl SectorDev for DiskDev {
            fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
                Ok(vec![(self.0 + lba) as u8; SECTOR])
            }

            fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
                Err(io::Error::other("read-only test device"))
            }
        }

        let opens = Cell::new(0usize);
        let mut pool = ReadOnlyDiskPool::new(|disk| {
            opens.set(opens.get() + 1);
            Ok(DiskDev(disk))
        });

        assert_eq!(pool.read_sector(6, 7).unwrap()[0], 13);
        assert_eq!(pool.read_sector(6, 12).unwrap()[0], 18);
        assert_eq!(pool.read_sector(7, 7).unwrap()[0], 14);
        assert_eq!(pool.read_sector(6, 4).unwrap()[0], 10);
        assert_eq!(opens.get(), 2, "同一物理盘在一次 list 会话内只应打开一次");
    }

    #[test]
    fn label_id_positive_negative_malformed() {
        let pos: Vec<u8> = b"$$$1402259934$$$"
            .iter()
            .chain([0u8; 18].iter())
            .copied()
            .collect();
        assert_eq!(lba4_label_id_from(&pos), Some("1402259934".to_string()));
        let neg: Vec<u8> = b"$$$-1833210541$$$"
            .iter()
            .chain([0u8; 15].iter())
            .copied()
            .collect();
        assert_eq!(lba4_label_id_from(&neg), Some("-1833210541".to_string()));
        assert_eq!(lba4_label_id_from(b""), None);
        assert_eq!(lba4_label_id_from(&[0u8; 32]), None);
        assert_eq!(lba4_label_id_from(b"@@@1@@@"), None);
        assert_eq!(lba4_label_id_from(b"$$$-$$$"), None); // '-' 后无数字
        assert_eq!(lba4_label_id_from(b"$$$$$$"), None); // 无数字
    }

    #[test]
    fn backup_dir_priority_and_conf_parse() {
        let cwd = PathBuf::from("/w");
        // 四级优先: 旗标 > env > conf > CWD 兜底
        assert_eq!(
            resolve_backup_dir_impl(
                Some("/f"),
                Some("/e".into()),
                Some("/c".into()),
                cwd.clone()
            ),
            PathBuf::from("/f")
        );
        assert_eq!(
            resolve_backup_dir_impl(None, Some("/e".into()), Some("/c".into()), cwd.clone()),
            PathBuf::from("/e")
        );
        assert_eq!(
            resolve_backup_dir_impl(None, None, Some("/c".into()), cwd.clone()),
            PathBuf::from("/c")
        );
        assert_eq!(
            resolve_backup_dir_impl(None, None, None, cwd.clone()),
            PathBuf::from("/w/backup")
        );
        // env 空串视同未设 → conf 兜底
        assert_eq!(
            resolve_backup_dir_impl(None, Some(String::new()), Some("/c".into()), cwd.clone()),
            PathBuf::from("/c")
        );
        // 相对值按 cwd 绝对化
        assert_eq!(
            resolve_backup_dir_impl(None, Some("bk".into()), None, cwd.clone()),
            PathBuf::from("/w/bk")
        );
        // conf 解析: 注释/坏行/空值/未知键
        assert_eq!(
            parse_conf_backup_dir("# 注释\nbackup_dir = /Users/x/.edpcli-backup\n"),
            Some("/Users/x/.edpcli-backup".to_string())
        );
        assert_eq!(
            parse_conf_backup_dir("backup_dir=/a/b"),
            Some("/a/b".to_string())
        );
        assert_eq!(parse_conf_backup_dir("backup_dir =   \n"), None); // 空值
        assert_eq!(parse_conf_backup_dir("other = 1\nnoise\n"), None);
        assert_eq!(parse_conf_backup_dir(""), None);
    }

    #[test]
    fn platform_user_home_is_available_for_current_session() {
        assert!(crate::platform::invoking_user_home().is_some());
    }

    #[test]
    fn backup_dir_argv_suffix_bridges_env() {
        // 提权边界不依赖环境继承：env 值须转为显式旗标（绝对路径）随 argv 过界。
        let absolute_dir = std::env::temp_dir().join("edpcli-absolute-backup");
        let abs = backup_dir_argv_suffix(Some(absolute_dir.to_string_lossy().into_owned()));
        assert_eq!(abs.len(), 2);
        assert_eq!(abs[0], "--backup-dir");
        assert_eq!(PathBuf::from(&abs[1]), absolute_dir);
        // 相对值按 CWD 绝对化
        let rel = backup_dir_argv_suffix(Some("bk".into()));
        assert_eq!(rel.len(), 2);
        let rel_path = PathBuf::from(&rel[1]);
        assert!(rel_path.is_absolute(), "{}", rel[1]);
        assert_eq!(
            rel_path.file_name().and_then(|name| name.to_str()),
            Some("bk")
        );
        // 未设/空值 → 不追加
        assert!(backup_dir_argv_suffix(None).is_empty());
        assert!(backup_dir_argv_suffix(Some(String::new())).is_empty());
    }

    #[test]
    fn utc_parts_known_date() {
        // 以 Python datetime(UTC) 校准: 2026-09-17 00:00:00 UTC = 1789603200
        assert_eq!(utc_parts(1789603200), (2026, 9, 17, 0, 0, 0));
        assert_eq!(utc_parts(1789660800), (2026, 9, 17, 16, 0, 0));
        assert_eq!(utc_parts(0), (1970, 1, 1, 0, 0, 0));
    }

    struct FixedClock;
    impl Clock for FixedClock {
        fn now_epoch(&self) -> i64 {
            1789660800
        }
        fn fmt_ts(&self, epoch: i64) -> String {
            format!("fixed_{}", epoch)
        }
        fn fmt_human(&self, epoch: i64) -> String {
            format!("h_{}", epoch)
        }
    }

    #[test]
    fn clock_fmt_uses_local_time_without_system_date_process() {
        let c = SystemClock;
        let ts = c.fmt_ts(1789660800);
        assert_eq!(ts.len(), 15, "{}", ts); // YYYYmmdd_HHMMSS
        let h = c.fmt_human(1789660800);
        assert_eq!(h.len(), 16, "{}", h); // YYYY-mm-dd HH:MM
        let _ = FixedClock.fmt_ts(1);
    }
}
