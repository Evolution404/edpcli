//! device_id 识别(真备份当"盘") + 外接盘一览(罐头 diskutil/ioreg) + list CLI 冒烟。

mod common;

use std::process::Command;

use common::*;
use nopwd::cli::{print_disk_table, scan_disks};
use nopwd::common::SECTOR;
use nopwd::identify::identify;
use nopwd::sysinfo::CmdRunner;

#[test]
fn identify_picks_edpf_verified_candidate() {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let runner = netac_runner(99);
    // 传输模式未知 → 候选序 [长(BOT带rev), 短(UAS)]; 长的不解 EDPF, 落到短的
    let r = identify(&runner, 99, &data[7 * SECTOR..8 * SECTOR]);
    assert_eq!(r.device_id.as_deref(), Some("disk&ven_netac&prod_onlydisk"));
    assert_eq!(r.crc, Some(0xF1A78819));
    assert_eq!(r.k0, Some(0x79BE));
}

#[test]
fn identify_no_candidate_matches() {
    let Some(data) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
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

#[test]
fn scan_and_print_all_row_kinds() {
    let Some(netac) = load_disk_image("netac") else {
        eprintln!("跳过: 真实备份不可用");
        return;
    };
    let mut m = std::collections::HashMap::new();
    m.insert("diskutil list -plist".to_string(), diskutil_list_plist(&["disk4", "disk4s1", "disk6", "disk7"]));
    m.insert("diskutil info -plist disk4".to_string(), diskutil_info_plist(64_000_000_000));
    m.insert("diskutil info -plist disk4s1".to_string(), diskutil_info_plist(64_000_000_000));
    m.insert("diskutil info -plist disk6".to_string(), diskutil_info_plist(62_914_560_000));
    m.insert(
        "diskutil info -plist disk7".to_string(),
        "<plist version=\"1.0\"><dict><key>WholeDisk</key><true/><key>Internal</key><false/><key>BusProtocol</key><string>Thunderbolt</string><key>TotalSize</key><integer>500107862016</integer></dict></plist>".to_string(),
    );
    // disk6 = netac cems 盘; disk4 = 假 vendor → 非cems
    m.insert("ioreg -r -c IOSCSITargetDevice -l".to_string(), ioreg_scsi(6, "Netac  ", "OnlyDisk", "1.00"));
    m.insert("ioreg -r -c IOUSBHostDevice -l".to_string(), ioreg_usb(6, 0x0DD8, 0x2005));
    let runner = FakeRunner { canned: m };

    let read_ok = |disk: u32, lba: u32| -> std::io::Result<Vec<u8>> {
        // disk6 读 netac 夹具; disk4 也读 netac(ioreg 是 Bogus → 识别不出, 与数据无关)
        let _ = disk;
        Ok(netac[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec())
    };
    let bak = TmpDir::new("scan_bak");
    let rows = scan_disks(&runner, &bak.0, &read_ok);
    let out = print_disk_table(&rows);
    assert!(out.contains("外接盘 3 个:"), "{}", out);
    assert!(out.contains("disk4") && out.contains("非cems盘"), "{}", out);
    assert!(out.contains("disk6") && out.contains("cems盘") && out.contains("无备份"), "{}", out);
    assert!(out.contains("disk7") && out.contains("非USB"), "{}", out);
    // 原盘数据: 非免密 + EDPF 3 条(含 Boot/Share/Encrypt)
    let row6 = rows.iter().find(|r| r.disk == 6).unwrap();
    assert!(!row6.is_nopwd);
    let parts = row6.partitions.as_ref().unwrap();
    assert_eq!(parts.len(), 3);
    assert!(out.contains("└─ EDPF:"), "{}", out);

    // 免密盘镜像: [免密] 标记 + EDPF 2 条
    let (conv, _) = converted_image("netac").unwrap();
    let read_conv = |_disk: u32, lba: u32| -> std::io::Result<Vec<u8>> {
        Ok(conv[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec())
    };
    let rows2 = scan_disks(&runner, &bak.0, &read_conv);
    let out2 = print_disk_table(&rows2);
    assert!(out2.contains("[免密]"), "{}", out2);
    let row6b = rows2.iter().find(|r| r.disk == 6).unwrap();
    assert!(row6b.is_nopwd);
    assert_eq!(row6b.partitions.as_ref().unwrap().len(), 2);

    // 读盘全被拒(未 sudo) → denied 降级行
    let read_denied = |_disk: u32, _lba: u32| -> std::io::Result<Vec<u8>> {
        Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"))
    };
    let rows2 = scan_disks(&runner, &bak.0, &read_denied);
    let out2 = print_disk_table(&rows2);
    assert!(out2.contains("sudo 可识别"), "{}", out2);
}

#[test]
fn list_cli_smoke_exit_zero() {
    // 本机 diskutil 真跑: 无论有没有插盘, 都应正常退出并给出可辨认输出
    let out = Command::new(env!("CARGO_BIN_EXE_nopwd"))
        .arg("list")
        .output()
        .expect("运行 nopwd 二进制");
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("外接盘") || stdout.contains("未检测到外接盘"), "{}", stdout);
}

// FakeRunner 辅助
trait RemoveScsi {
    fn canned_remove_scsi(self) -> FakeRunner;
}
impl RemoveScsi for FakeRunner {
    fn canned_remove_scsi(mut self) -> FakeRunner {
        self.canned.remove("ioreg -r -c IOSCSITargetDevice -l");
        self
    }
}
