//! 原子写入三态(成功/中途失败回滚/读回不符回滚) + 短写循环。
//! 全部跑在文件镜像上, 不碰真盘。故障注入经 FlakyDev 包装器(Rust 无 monkeypatch)。

mod common;

use std::collections::BTreeMap;
use std::fs;

use common::*;
use nopwd::common::{SECTOR, EXIT_INTERMEDIATE, EXIT_ROLLED_BACK};
use nopwd::diskio::{atomic_write_sectors, pwrite_loop, FileDev, SectorDev};

struct Image {
    #[allow(dead_code)] // 仅为 Drop 清理保留
    tmp: TmpDir,
    path: std::path::PathBuf,
    base: Vec<u8>,
    patch: BTreeMap<u32, Vec<u8>>,
}

fn setup() -> Option<Image> {
    let base = load_disk_image("netac")?;
    let tmp = TmpDir::new("atomic");
    let path = tmp.0.join("disk.img");
    fs::write(&path, &base).unwrap();
    let patch: BTreeMap<u32, Vec<u8>> = [
        (6u32, vec![0x11u8; SECTOR]),
        (7, vec![0x22; SECTOR]),
        (12, vec![0x33; SECTOR]),
        (0, vec![0x44; SECTOR]),
    ]
    .into_iter()
    .collect();
    Some(Image { tmp, path, base, patch })
}

fn img_bytes(path: &std::path::Path) -> Vec<u8> {
    fs::read(path).unwrap()
}

/// 包装 FileDev: 第 n 次 write_sector 注入失败。
struct FlakyDev {
    inner: FileDev,
    fail_on: Vec<u32>, // 按调用序号(1 起)注入 Err
    calls: u32,
}

impl SectorDev for FlakyDev {
    fn read_sector(&mut self, lba: u32) -> std::io::Result<Vec<u8>> {
        self.inner.read_sector(lba)
    }
    fn write_sector(&mut self, lba: u32, data: &[u8]) -> std::io::Result<()> {
        self.calls += 1;
        if self.fail_on.contains(&self.calls) {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "注入的写入失败"));
        }
        self.inner.write_sector(lba, data)
    }
}

/// 包装 FileDev: 写入扇区数达到阈值后, LBA12 的读回返回全零(模拟"读回与写入不符")。
struct TamperReadDev {
    inner: FileDev,
    written: u32,
    threshold: u32,
    tampered: bool,
}

impl SectorDev for TamperReadDev {
    fn read_sector(&mut self, lba: u32) -> std::io::Result<Vec<u8>> {
        if self.written >= self.threshold && lba == 12 && !self.tampered {
            self.tampered = true;
            return Ok(vec![0u8; SECTOR]); // 模拟读回与写入不符
        }
        self.inner.read_sector(lba)
    }
    fn write_sector(&mut self, lba: u32, data: &[u8]) -> std::io::Result<()> {
        self.written += 1;
        self.inner.write_sector(lba, data)
    }
}

#[test]
fn success_writes_all_and_verifies() {
    let Some(im) = setup() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let mut dev = FileDev::open_rdwr(im.path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    atomic_write_sectors(&mut dev, &im.patch).unwrap();
    let got = img_bytes(&im.path);
    assert_eq!(&got[0..SECTOR], &im.patch[&0][..]);
    for lba in [6u32, 7, 12] {
        assert_eq!(&got[lba as usize * SECTOR..(lba as usize + 1) * SECTOR], &im.patch[&lba][..]);
    }
    // 未列入的扇区一律不动
    assert_eq!(&got[SECTOR..6 * SECTOR], &im.base[SECTOR..6 * SECTOR]);
    assert_eq!(&got[8 * SECTOR..12 * SECTOR], &im.base[8 * SECTOR..12 * SECTOR]);
}

#[test]
fn midway_failure_rolls_back() {
    let Some(im) = setup() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let inner = FileDev::open_rdwr(im.path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let mut dev = FlakyDev { inner, fail_on: vec![3], calls: 0 }; // 第 3 次写(LBA12)失败
    let e = atomic_write_sectors(&mut dev, &im.patch).unwrap_err();
    assert_eq!(e.code, EXIT_ROLLED_BACK, "{}", e.msg);
    assert!(e.msg.contains("回滚"), "{}", e.msg);
    assert_eq!(img_bytes(&im.path), im.base); // 逐字节回到写前
}

#[test]
fn verify_failure_rolls_back() {
    let Some(im) = setup() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let inner = FileDev::open_rdwr(im.path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    // 校验阶段(4 扇已写完)对 LBA12 返回错误数据
    let mut dev = TamperReadDev { inner, written: 0, threshold: 4, tampered: false };
    let e = atomic_write_sectors(&mut dev, &im.patch).unwrap_err();
    assert_eq!(e.code, EXIT_ROLLED_BACK, "{}", e.msg);
    assert!(e.msg.contains("回滚"), "{}", e.msg);
    assert_eq!(img_bytes(&im.path), im.base); // 回滚后逐字节写前状态
}

#[test]
fn rollback_failure_reports_intermediate() {
    let Some(im) = setup() else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    // 正写与回滚写全部失败 → 中间态(退出码 6)
    let inner = FileDev::open_rdwr(im.path.to_str().unwrap(), std::time::Duration::from_secs(1)).unwrap();
    let mut dev = FlakyDev { inner, fail_on: vec![1, 2, 3, 4], calls: 0 };
    let e = atomic_write_sectors(&mut dev, &im.patch).unwrap_err();
    assert_eq!(e.code, EXIT_INTERMEDIATE, "{}", e.msg);
    assert!(e.msg.contains("中间状态"), "{}", e.msg);
}

#[test]
fn pwrite_loop_handles_short_writes() {
    // 每次只写一半: 循环必须把 512B 写满(Python TestPwriteFull 等价)
    let tmp = TmpDir::new("pwrite");
    let p = tmp.0.join("x.bin");
    fs::write(&p, vec![0u8; 14 * SECTOR]).unwrap();
    {
        use std::fs::OpenOptions;
        use std::os::unix::fs::FileExt;
        let f = OpenOptions::new().write(true).open(&p).unwrap();
        let mut pos: usize = 0;
        pwrite_loop(
            |buf, _off| {
                let n = (buf.len() / 2).max(1);
                f.write_at(&buf[..n], 6 * SECTOR as u64 + pos as u64)?;
                pos += n;
                Ok(n)
            },
            &vec![0xAA; SECTOR],
            6 * SECTOR as u64,
        )
        .unwrap();
    }
    let data = fs::read(&p).unwrap();
    assert_eq!(&data[6 * SECTOR..7 * SECTOR], &vec![0xAA; SECTOR][..]);
}
