//! device_id 识别(真备份当"盘") + 外接盘一览(罐头 diskutil/ioreg) + list CLI 冒烟。

use std::process::Command;

#[cfg(target_os = "macos")]
use crate::common::*;
#[cfg(target_os = "macos")]
use edpcli::application::support::SECTOR;
#[cfg(target_os = "macos")]
use edpcli::cli::{print_disk_table, scan_disks};
#[cfg(target_os = "macos")]
use edpcli::platform::identity::identify;

#[cfg(target_os = "macos")]
#[test]
fn identify_picks_edpf_verified_candidate() {
    let data = load_disk_image("netac");
    let runner = netac_runner(99);
    // 传输模式未知 → 候选序 [长(BOT带rev), 短(UAS)]; 长的不解 EDPF, 落到短的
    let r = identify(&runner, 99, &data[7 * SECTOR..8 * SECTOR]);
    assert_eq!(r.device_id.as_deref(), Some("disk&ven_netac&prod_onlydisk"));
    assert_eq!(r.crc, Some(0xF1A78819));
    assert_eq!(r.k0, Some(0x79BE));
}

#[cfg(target_os = "macos")]
#[test]
fn identify_no_candidate_matches() {
    let data = load_disk_image("netac");
    let runner = netac_runner(99).canned_remove_scsi(); // ioreg 失败 → 无候选
    let r = identify(&runner, 99, &data[7 * SECTOR..8 * SECTOR]);
    assert!(r.device_id.is_none() && r.crc.is_none() && r.k0.is_none());

    let mut m = std::collections::HashMap::new();
    m.insert(
        "ioreg -r -c IOSCSITargetDevice -l".to_string(),
        ioreg_scsi(99, "Bogus", "Flash", "1.0"),
    );
    let runner2 = FakeRunner { canned: m };
    let r2 = identify(&runner2, 99, &data[7 * SECTOR..8 * SECTOR]);
    assert!(r2.device_id.is_none());
}

#[cfg(target_os = "macos")]
#[test]
fn scan_and_print_all_row_kinds() {
    let netac = load_disk_image("netac");
    let mut m = std::collections::HashMap::new();
    m.insert(
        "diskutil list -plist".to_string(),
        diskutil_list_plist(&["disk4", "disk4s1", "disk6", "disk7"]),
    );
    m.insert(
        "diskutil info -plist disk4".to_string(),
        diskutil_info_plist(64_000_000_000),
    );
    m.insert(
        "diskutil info -plist disk4s1".to_string(),
        diskutil_info_plist(64_000_000_000),
    );
    m.insert(
        "diskutil info -plist disk6".to_string(),
        diskutil_info_plist(62_914_560_000),
    );
    m.insert(
        "diskutil info -plist disk7".to_string(),
        "<plist version=\"1.0\"><dict><key>WholeDisk</key><true/><key>Internal</key><false/><key>BusProtocol</key><string>Thunderbolt</string><key>DeviceBlockSize</key><integer>512</integer><key>TotalSize</key><integer>500107862016</integer></dict></plist>".to_string(),
    );
    // disk6 = netac cems 盘; disk4 = 假 vendor → 非cems
    m.insert(
        "ioreg -r -c IOSCSITargetDevice -l".to_string(),
        ioreg_scsi(6, "Netac  ", "OnlyDisk", "1.00"),
    );
    m.insert(
        "ioreg -r -c IOUSBHostDevice -l".to_string(),
        ioreg_usb(6, 0x0DD8, 0x2005),
    );
    let runner = FakeRunner { canned: m };

    let mut plain = vec![0u8; 13 * SECTOR];
    let plain_total_sectors = 64_000_000_000u64 / SECTOR as u64;
    let entry = 0x1be;
    plain[entry + 4] = 0x07;
    plain[entry + 8..entry + 12].copy_from_slice(&2048u32.to_le_bytes());
    plain[entry + 12..entry + 16].copy_from_slice(
        &u32::try_from(plain_total_sectors - 2048)
            .unwrap()
            .to_le_bytes(),
    );
    plain[510..512].copy_from_slice(&[0x55, 0xaa]);

    let read_calls = std::cell::RefCell::new(Vec::<(u32, u32)>::new());
    let read_ok = |disk: u32, lba: u32| -> std::io::Result<Vec<u8>> {
        read_calls.borrow_mut().push((disk, lba));
        let source = if disk == 4 {
            plain.as_slice()
        } else {
            netac.as_slice()
        };
        let start = lba as usize * SECTOR;
        let end = start + SECTOR;
        if let Some(bytes) = source.get(start..end) {
            Ok(bytes.to_vec())
        } else {
            Ok(vec![0; SECTOR])
        }
    };
    let bak = TmpDir::new("scan_bak");
    let rows = scan_disks(&runner, &bak.0, &read_ok);
    let out = print_disk_table(&rows);
    assert!(out.contains("外接盘 3 个:"), "{}", out);
    assert!(out.contains("disk4") && out.contains("普通盘"), "{}", out);
    assert!(
        out.contains("disk6") && out.contains("mode0 · 缺省三分区") && out.contains("无备份"),
        "{}",
        out
    );
    assert!(out.contains("disk7") && out.contains("非 USB"), "{}", out);
    // 原盘数据: 非mode1 + EDPF 3 条(含 Boot/Share/Encrypt)
    let row6 = rows.iter().find(|r| r.disk == 6).unwrap();
    assert_eq!(row6.user.as_deref(), Some("宋旭琳"));
    assert_eq!(row6.label.as_deref(), Some("江苏电力!SAFE6"));
    assert_eq!(row6.force_change_password, Some(true));
    assert!(
        row6.dept
            .as_deref()
            .unwrap_or_default()
            .contains("泰州供电公司"),
        "dept={:?}",
        row6.dept
    );
    assert!(out.contains("姓名") && out.contains("部门") && out.contains("宋旭琳"));
    let parts = row6.partitions.as_ref().unwrap();
    assert_eq!(parts.len(), 3);
    assert!(out.contains("└─ EDPF:"), "{}", out);
    assert_eq!(
        read_calls
            .borrow()
            .iter()
            .filter(|&&(disk, lba)| disk == 6 && lba == 12)
            .count(),
        1,
        "list 同一次设备扫描不应重复读取 LBA12"
    );

    // 旧mode1 盘镜像应统一显示官方模式1盘型。
    let (conv, _) = mode1_fixture_image("netac").unwrap();
    let converted_read_calls = std::cell::RefCell::new(Vec::<(u32, u32)>::new());
    let read_conv = |disk: u32, lba: u32| -> std::io::Result<Vec<u8>> {
        converted_read_calls.borrow_mut().push((disk, lba));
        let source = if disk == 4 {
            plain.as_slice()
        } else {
            conv.as_slice()
        };
        let start = lba as usize * SECTOR;
        let end = start + SECTOR;
        if let Some(bytes) = source.get(start..end) {
            Ok(bytes.to_vec())
        } else {
            Ok(vec![0; SECTOR])
        }
    };
    let rows2 = scan_disks(&runner, &bak.0, &read_conv);
    let out2 = print_disk_table(&rows2);
    assert!(out2.contains("mode1 · 二合一"), "{}", out2);
    let row6b = rows2.iter().find(|r| r.disk == 6).unwrap();
    assert_eq!(row6b.label.as_deref(), Some("江苏电力!SAFE6"));
    assert_eq!(row6b.force_change_password, Some(true));
    assert_eq!(row6b.partitions.as_ref().unwrap().len(), 2);
    assert_eq!(
        converted_read_calls
            .borrow()
            .iter()
            .filter(|&&(disk, lba)| disk == 6 && lba == 12)
            .count(),
        1,
        "mode1 盘 list 扫描也不应为状态判断和分区展示重复读取 LBA12"
    );

    // 读盘全被拒（权限不足）→ denied 降级行
    let read_denied = |_disk: u32, _lba: u32| -> std::io::Result<Vec<u8>> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ))
    };
    let rows2 = scan_disks(&runner, &bak.0, &read_denied);
    let out2 = print_disk_table(&rows2);
    assert!(
        out2.contains("管理员权限") && out2.contains("识别"),
        "{}",
        out2
    );

    // 抽象读层若意外返回短扇区，list 也必须降级为不可读，不能切片 panic。
    let read_short = |_disk: u32, lba: u32| -> std::io::Result<Vec<u8>> {
        if lba == 4 {
            Ok(vec![0u8; 8])
        } else {
            Ok(netac[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec())
        }
    };
    let rows3 = scan_disks(&runner, &bak.0, &read_short);
    let row6c = rows3.iter().find(|r| r.disk == 6).unwrap();
    assert!(!row6c.denied, "短读不是权限错误，不应误导用户去提权");
    assert!(row6c
        .probe_error
        .as_deref()
        .unwrap_or("")
        .contains("预期 512B"));
    let out3 = print_disk_table(&rows3);
    assert!(out3.contains("读取异常"), "{}", out3);
}

#[cfg(target_os = "macos")]
#[test]
fn scan_prefers_live_plain_filesystem_over_stale_edp_protocol_fields() {
    let mut stale = load_disk_image("netac");
    const TOTAL: u64 = 122_880_000;
    stale[..SECTOR].fill(0);
    let entry = 0x1be;
    stale[entry + 4] = 0x07;
    stale[entry + 8..entry + 12].copy_from_slice(&2_048u32.to_le_bytes());
    stale[entry + 12..entry + 16]
        .copy_from_slice(&u32::try_from(TOTAL - 2_048).unwrap().to_le_bytes());
    stale[510..512].copy_from_slice(&[0x55, 0xaa]);
    let fs = edpcli::application::filesystem::build_empty_exfat(
        2_048,
        TOTAL - 2_048,
        0x1234_5678,
        "PLAIN",
    )
    .unwrap();
    let boot = fs.sectors().get(&0).unwrap().to_vec();

    let mut m = std::collections::HashMap::new();
    m.insert(
        "diskutil list -plist".to_string(),
        diskutil_list_plist(&["disk6"]),
    );
    m.insert(
        "diskutil info -plist disk6".to_string(),
        diskutil_info_plist(i64::try_from(TOTAL * SECTOR as u64).unwrap()),
    );
    m.insert(
        "ioreg -r -c IOSCSITargetDevice -l".to_string(),
        ioreg_scsi(6, "Netac  ", "OnlyDisk", "1.00"),
    );
    m.insert(
        "ioreg -r -c IOUSBHostDevice -l".to_string(),
        ioreg_usb(6, 0x0DD8, 0x2005),
    );
    let runner = FakeRunner { canned: m };
    let read = |_disk: u32, lba: u32| -> std::io::Result<Vec<u8>> {
        if lba == 2_048 {
            return Ok(boot.clone());
        }
        if lba < 13 {
            let start = lba as usize * SECTOR;
            return Ok(stale[start..start + SECTOR].to_vec());
        }
        Ok(vec![0u8; SECTOR])
    };
    let bak = TmpDir::new("stale_edp_plain_scan");
    let rows = scan_disks(&runner, &bak.0, &read);
    let row = rows.iter().find(|row| row.disk == 6).unwrap();

    assert_eq!(
        row.provision_kind,
        edpcli::provision::DiskProvisionKind::Plain
    );
    assert_eq!(row.device_id, None);
    assert_eq!(row.onlyid, None);
    assert_eq!(row.dept, None);
    assert_eq!(row.user, None);
    assert_eq!(row.label, None);
    assert!(row.partitions.is_none());
    assert_eq!(row.force_change_password, None);
    assert!(row.partition_table.is_some());
    assert_eq!(
        row.identity_pin
            .as_ref()
            .and_then(|pin| pin.snapshot.protocol.provision_kind),
        Some(edpcli::provision::DiskProvisionKind::Plain)
    );
}

#[test]
fn list_cli_smoke_exit_zero() {
    // 本机平台探测真跑：无论有没有插盘，都应正常退出并给出可辨认输出。
    let out = Command::new(env!("CARGO_BIN_EXE_edpcli"))
        .arg("list")
        // 测试不能弹 sudo/UAC；内部哨兵只阻止自动提权重入，不改变只读扫描。
        .arg("--_elevated")
        .output()
        .expect("运行 edpcli 二进制");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("外接盘") || stdout.contains("未检测到外接盘"),
        "{}",
        stdout
    );
}

// FakeRunner 辅助
#[cfg(target_os = "macos")]
trait RemoveScsi {
    fn canned_remove_scsi(self) -> FakeRunner;
}
#[cfg(target_os = "macos")]
impl RemoveScsi for FakeRunner {
    fn canned_remove_scsi(mut self) -> FakeRunner {
        self.canned.remove("ioreg -r -c IOSCSITargetDevice -l");
        self
    }
}
