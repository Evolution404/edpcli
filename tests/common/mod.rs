//! 测试公用: 真实协议夹具定位、金标(与 Python 版逐字一致)、临时目录、免密盘镜像合成。
//! 每个集成测试文件各自引入本模块, 未被该文件用到的项不算死代码。
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use edpcli::common::SECTOR;
use edpcli::sectors::convert;
use edpcli::sha256::sha256_hex;

pub const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/protocol");

/// (键, 备份文件名, device_id) — 覆盖三种型号, 含 BOT 带 &rev_ 的 aigo
pub fn fixture(key: &str) -> Option<(&'static str, &'static str)> {
    match key {
        "netac" => Some((
            "disk6_122880000_vid0dd8_pid2005_disk&ven_netac&prod_onlydisk_onlyid1402259934_20260910_172300.bin",
            "disk&ven_netac&prod_onlydisk",
        )),
        "lexar" => Some((
            "disk4_243625984_vid21c4_pid0cd1_disk&ven_lexar&prod_usb_flash_drive_onlyid3164177653_20260827_221910.bin",
            "disk&ven_lexar&prod_usb_flash_drive",
        )),
        "aigo" => Some((
            "disk4_245760000_vid3535_pid6300_disk&ven_aigo&prod_u335&rev_pmap_onlyid1987718388_20260827_191701.bin",
            "disk&ven_aigo&prod_u335&rev_pmap",
        )),
        _ => None,
    }
}

pub const KEYS: [&str; 3] = ["netac", "lexar", "aigo"];

/// 负 id 备份(仅 label_id 解析用): aigo hd806, onlyid=-1833210541
pub const AIGO_NEG_BIN: &str =
    "disk4_1953525168_vid174c_pid55aa_disk&ven_aigo&prod_hd806_onlyid-1833210541_20260903_121552.bin";

/// 13 扇区协议夹具的完整路径; 文件不存在返回 None(用例自行跳过)。
pub fn fixture_bin(key: &str) -> Option<PathBuf> {
    let (bin, _) = fixture(key)?;
    let p = PathBuf::from(FIXTURE_DIR).join(bin);
    p.exists().then_some(p)
}

pub fn neg_id_bin() -> Option<PathBuf> {
    let p = PathBuf::from(FIXTURE_DIR).join(AIGO_NEG_BIN);
    p.exists().then_some(p)
}

/// 整份 13 扇协议夹具 → 一张"盘"的 LBA0-12 镜像。
pub fn load_disk_image(key: &str) -> Option<Vec<u8>> {
    fs::read(fixture_bin(key)?).ok()
}

pub fn read_fn_of(
    data: &[u8],
) -> impl Fn(u32) -> Result<Vec<u8>, edpcli::common::EdpCliError> + '_ {
    move |lba| Ok(data[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec())
}

/// 金标 = 2026-09-16 拆包前单文件版对真实备份的实测输出(与 Python test_sectors.py 逐字一致)
pub struct Golden {
    pub share: u64,
    pub enc_start: u64,
    pub enc_size: u64,
    pub crc: u32,
    pub k0: u32,
    pub lba9_none: bool,
    pub lba0: &'static str,
    pub lba6: &'static str,
    pub lba7: &'static str,
    pub lba12: &'static str,
}

pub fn golden(key: &str) -> Golden {
    match key {
        "netac" => Golden {
            share: 116707265,
            enc_start: 116707328,
            enc_size: 3143761920,
            crc: 0xF1A78819,
            k0: 0x79BE,
            lba9_none: false,
            lba0: "78a2b41a827c97efe9914a7afe5e92d6711c4ad79d60f081752c3e7e8abe92ad",
            lba6: "dd7dd0487f2888112e9c0df398835eda6bac9dbf5b8fbd2bc1330eba4a90e69c",
            lba7: "bb9c3408edabe217be11bf6fba62a2951de0588e5c4bb215562e300a896358b9",
            lba12: "2b3ae3c06962ba2b30d5deac2c7740cac95aa233fb31b4bc2217fe754a887df2",
        },
        "lexar" => Golden {
            share: 231423937,
            enc_start: 231424000,
            enc_size: 6234963968,
            crc: 0x6BBAEEFB,
            k0: 0x8541,
            lba9_none: false,
            lba0: "093f6dc8af363b092c91df79d709983c6f103b9cfb90deb23837d0dbafdc29a5",
            lba6: "cc66cd15d26a56cc38f8232441655f377d31920cb4b4e5c7b616866ca9ad300a",
            lba7: "615023be42f14182e6a9d13568bbc158d09571e5978455959d8c216efe9f6adf",
            lba12: "feaa4b2ca4d56f6300eb3276f9ec64ee9e5eacdcf74fa784464355a52c8055be",
        },
        "aigo" => Golden {
            share: 243115997,
            enc_start: 243116060,
            enc_size: 1340720640,
            crc: 0x2EEB4CE1,
            k0: 0x620A,
            lba9_none: true,
            lba0: "6b85db3f0029e1396f480ad9a2efbada62464ce151f67b46eb789533d55970cf",
            lba6: "931b1924baca6e39933e4b81379f4e47f6c1af9d7cda4a62fdfe3fc3f371a7e8",
            lba7: "44f5167f9fd8f7cacda5912e64b46d09bceefb708fa211a14161c6bcaae94238",
            lba12: "b319aa7a013477fe8778c51dcab719c6db95803992db5bb9df568abae06888b6",
        },
        "aigo_size50" => Golden {
            share: 97656248,
            enc_start: 243116060,
            enc_size: 0,
            crc: 0,
            k0: 0,
            lba9_none: true,
            lba0: "090b9c91e0970a04bd32d4cd8e307ed6fceefdce2e1e18ba1ac29bdc67253606",
            lba6: "931b1924baca6e39933e4b81379f4e47f6c1af9d7cda4a62fdfe3fc3f371a7e8",
            lba7: "7e0ddd83d9d340af6ca5b9bc1cfb2acdaef981bdb37a9c0c215f731b27345e97",
            lba12: "cf1832eaf98505a9cfcdbb7500d2b527a385d5a2f7629cb270fa2c6a55c04327",
        },
        _ => panic!("未知金标键 {}", key),
    }
}

/// 合成免密盘镜像(原备份 + 改造后 LBA0/6/7/12, LBA9 清零) → (bytes, device_id)。
pub fn converted_image(key: &str) -> Option<(Vec<u8>, String)> {
    let data = load_disk_image(key)?;
    let (_, did) = fixture(key)?;
    let r = convert(&read_fn_of(&data), did, None, false).ok()?;
    let mut conv = data.clone();
    for (lba, sector) in [
        (0usize, &r.lba0),
        (6, &r.lba6),
        (7, &r.lba7),
        (12, &r.lba12),
    ] {
        conv[lba * SECTOR..(lba + 1) * SECTOR].copy_from_slice(sector);
    }
    conv[9 * SECTOR..10 * SECTOR].fill(0);
    Some((conv, did.to_string()))
}

/// 临时目录 guard(Drop 清理)。
pub struct TmpDir(pub PathBuf);

static COUNTER: AtomicU32 = AtomicU32::new(0);

impl TmpDir {
    pub fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!(
            "edpcli_test_{}_{}_{tag}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&d).unwrap();
        TmpDir(d)
    }
}

impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn sha256(b: &[u8]) -> String {
    sha256_hex(b)
}

// ══════════════════════════════════════════════════════════════════
// 进程内 CLI 测试假件
// ══════════════════════════════════════════════════════════════════
use std::collections::HashMap;
use std::time::Duration;

/// 罐头 CmdRunner: 完整命令行(空格连接) → 预置输出。
pub struct FakeRunner {
    pub canned: HashMap<String, String>,
}

impl edpcli::sysinfo::CmdRunner for FakeRunner {
    fn check_output(&self, cmd: &[&str], _t: Duration) -> std::io::Result<String> {
        self.canned
            .get(&cmd.join(" "))
            .cloned()
            .ok_or_else(|| std::io::Error::other(format!("无罐头: {}", cmd.join(" "))))
    }
}

/// ioreg SCSI 类罐头输出(真实树形格式: 竖线前缀 + @0 节点名; vendor/product 决定 device_id 候选)。
/// 根节点与被查询类同名(查询 IOSCSITargetDevice 时 ioreg -r -c 即返回此形状)。
pub fn ioreg_scsi(disk: u32, vendor: &str, product: &str, rev: &str) -> String {
    format!(
        "+-o IOSCSITargetDevice@0  <class IOSCSITargetDevice, id 0x100002b0e, registered, matched, active, retain 8>\n  |   \"IOPropertyMatch\" = \"x\"\n  +-o IOSCSILogicalUnitNub@0  <class IOSCSILogicalUnitNub, id 0x100002b11, retain 10>\n    |   \"Vendor Identification\" = \"{v}\"\n    |   \"Product Identification\" = \"{p}\"\n    |   \"Product Revision Level\" = \"{r}\"\n    +-o IOBlockStorageServices  <class IOBlockStorageServices, id 0x100002b14>\n      +-o {v} {p} Media  <class IOMedia, id 0x100002b17, registered, matched, active, retain 12>\n        |   \"BSD Name\" = \"disk{d}\"\n",
        d = disk, v = vendor, p = product, r = rev
    )
}

pub fn ioreg_usb(disk: u32, vid: i64, pid: i64) -> String {
    format!(
        "+-o IOUSBHostDevice  <class IOUSBHostDevice, id 0x100002af0, registered, matched, active, busy 0, retain 15>\n  |   \"idVendor\" = {v}\n  |   \"idProduct\" = {p}\n  |   \"USB Product Name\" = \"Mass Storage\"\n  +-o IOUSBMassStorageInterfaceNub  <class IOUSBMassStorageInterfaceNub>\n    +-o IOUSBMassStorageDriverNub  <class IOUSBMassStorageDriverNub>\n      +-o IOUSBMassStorageDriver  <class IOUSBMassStorageDriver>\n        +-o IOSCSILogicalUnitNub@0  <class IOSCSILogicalUnitNub>\n          +-o {v} {p} Media  <class IOMedia>\n            |   \"BSD Name\" = \"disk{d}\"\n",
        v = vid, p = pid, d = disk
    )
}

pub fn diskutil_info_plist(total_size: i64) -> String {
    format!(
        "<plist version=\"1.0\"><dict><key>DiskSize</key><integer>{s}</integer><key>TotalSize</key><integer>{s}</integer><key>WholeDisk</key><true/><key>Internal</key><false/><key>BusProtocol</key><string>USB</string></dict></plist>",
        s = total_size
    )
}

pub fn diskutil_list_plist(disks: &[&str]) -> String {
    let items: String = disks
        .iter()
        .map(|d| format!("<string>{}</string>", d))
        .collect();
    format!(
        "<plist version=\"1.0\"><dict><key>AllDisks</key><array>{}</array></dict></plist>",
        items
    )
}

pub fn diskutil_root_plist(physical_disk: u32) -> String {
    format!(
        "<plist version=\"1.0\"><dict><key>FilesystemType</key><string>apfs</string><key>APFSPhysicalStores</key><array><dict><key>APFSPhysicalStore</key><string>disk{}s2</string></dict></array></dict></plist>",
        physical_disk
    )
}

/// netac 盘的罐头环境(识别/容量/VID:PID 都指向 netac 事实)。
pub fn netac_runner(disk: u32) -> FakeRunner {
    let mut m = HashMap::new();
    let disk_name = format!("disk{}", disk);
    m.insert(
        "diskutil info -plist /".into(),
        diskutil_root_plist(if disk == 1 { 1 } else { 0 }),
    );
    m.insert(
        "diskutil list -plist".into(),
        diskutil_list_plist(&[disk_name.as_str()]),
    );
    m.insert(
        format!("diskutil info -plist disk{}", disk),
        diskutil_info_plist(62_914_560_000),
    );
    m.insert(
        format!("diskutil unmountDisk force disk{}", disk),
        "Unmount of all volumes on disk was successful".into(),
    );
    m.insert(
        "ioreg -r -c IOSCSITargetDevice -l".into(),
        ioreg_scsi(disk, "Netac  ", "OnlyDisk", "1.00"),
    );
    m.insert(
        "ioreg -r -c IOUSBHostDevice -l".into(),
        ioreg_usb(disk, 0x0DD8, 0x2005),
    );
    FakeRunner { canned: m }
}

/// 脚本化提示器: 依次吐出预置输入。
pub struct ScriptPrompter {
    pub inputs: Vec<String>,
    pub idx: usize,
}

impl ScriptPrompter {
    pub fn yes() -> Self {
        ScriptPrompter {
            inputs: vec!["YES".into()],
            idx: 0,
        }
    }
}

impl edpcli::cli::Prompter for ScriptPrompter {
    fn prompt_line(&mut self, _msg: &str) -> String {
        let s = self.inputs.get(self.idx).cloned().unwrap_or_default();
        self.idx += 1;
        s
    }
    fn confirm_yes(&mut self, _msg: &str) -> bool {
        self.prompt_line("").trim() == "YES"
    }
}

/// 设置文件 mtime(备份排序测试用)。
pub fn set_mtime(path: &std::path::Path, epoch: i64) {
    let f = fs::OpenOptions::new().write(true).open(path).unwrap();
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(epoch as u64);
    f.set_times(std::fs::FileTimes::new().set_modified(t))
        .unwrap();
}
