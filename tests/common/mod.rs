//! 测试公用: 真实协议夹具定位、金标(与 Python 版逐字一致)、临时目录、mode1 盘镜像合成。
//! 每个集成测试文件各自引入本模块, 未被该文件用到的项不算死代码。
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use edpcli::application::support::SECTOR;
use edpcli::edpb::sha256_hex;
use edpcli::platform::{HardwareProbe, InquiryInfo, NativeTransport};
use edpcli::provision::{
    generate_image, OnlyId, ProvisionEntropy, ProvisionMetadata, ProvisionProfile, ProvisionSpec,
    TargetIdentity,
};

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

/// Required protocol fixtures fail closed on missing, truncated or corrupt evidence.
pub fn fixture_bin(key: &str) -> PathBuf {
    let (name, _) = fixture(key).expect("unknown required fixture key");
    let path = PathBuf::from(FIXTURE_DIR).join(name);
    required_image(&path);
    path
}
pub fn neg_id_bin() -> PathBuf {
    let path = PathBuf::from(FIXTURE_DIR).join(AIGO_NEG_BIN);
    required_image(&path);
    path
}
pub fn required_image(path: &std::path::Path) -> Vec<u8> {
    let data = fs::read(path)
        .unwrap_or_else(|error| panic!("required fixture {}: {error}", path.display()));
    assert_eq!(
        data.len(),
        edpcli::application::support::METADATA_IMAGE_LEN,
        "required fixture length: {}",
        path.display()
    );
    let hash_path = PathBuf::from(format!("{}.sha256", path.display()));
    let expected = fs::read_to_string(&hash_path)
        .unwrap_or_else(|error| panic!("required fixture hash {}: {error}", hash_path.display()));
    assert_eq!(
        sha256_hex(&data),
        expected
            .split_whitespace()
            .next()
            .expect("empty fixture hash"),
        "required fixture SHA-256: {}",
        path.display()
    );
    data
}
pub fn load_disk_image(key: &str) -> Vec<u8> {
    required_image(&fixture_bin(key))
}

pub fn read_fn_of(
    data: &[u8],
) -> impl Fn(u32) -> Result<Vec<u8>, edpcli::application::support::EdpCliError> + '_ {
    move |lba| Ok(data[lba as usize * SECTOR..(lba as usize + 1) * SECTOR].to_vec())
}

/// 用当前正式 provisioning builder 合成一张结构完整的 mode1 元数据镜像。
/// 这类样本供 mode1 协议识别、备份与恢复测试使用。
pub fn mode1_fixture_image(key: &str) -> Option<(Vec<u8>, String)> {
    let (vid, pid, total_sectors, transport, vendor, product, revision, onlyid) = match key {
        "netac" => (
            0x0dd8,
            0x2005,
            122_880_000,
            NativeTransport::Uas,
            "Netac",
            "OnlyDisk",
            "",
            "1402259934",
        ),
        "lexar" => (
            0x21c4,
            0x0cd1,
            243_625_984,
            NativeTransport::Uas,
            "Lexar",
            "USB Flash Drive",
            "",
            "3164177653",
        ),
        "aigo" => (
            0x3535,
            0x6300,
            245_760_000,
            NativeTransport::Bot,
            "aigo",
            "U335",
            "PMAP",
            "1987718388",
        ),
        _ => return None,
    };
    let probe = HardwareProbe {
        vid: Some(vid),
        pid: Some(pid),
        transport,
        windows_pnp_instance_id: None,
        inquiry: Some(InquiryInfo {
            vendor: vendor.into(),
            product: product.into(),
            revision: revision.into(),
        }),
    };
    let target = TargetIdentity::from_probe(&probe, total_sectors).ok()?;
    let device_id = target.device_id().to_string();
    let metadata = ProvisionMetadata::new(
        OnlyId::parse(onlyid).ok()?,
        "测试",
        "测试",
        "江苏电力!SAFE6",
    )
    .ok()?;
    let spec = ProvisionSpec::new(
        target,
        metadata,
        ProvisionProfile::canonical_v1().with_force_change_password(true),
    )
    .ok()?;
    let image = generate_image(&spec, &ProvisionEntropy::new([0u8; 252])).ok()?;
    Some((image.into_bytes().to_vec(), device_id))
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

impl edpcli::ports::CmdRunner for FakeRunner {
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
        "<plist version=\"1.0\"><dict><key>DiskSize</key><integer>{s}</integer><key>DeviceBlockSize</key><integer>512</integer><key>TotalSize</key><integer>{s}</integer><key>WholeDisk</key><true/><key>Internal</key><false/><key>BusProtocol</key><string>USB</string></dict></plist>",
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

/// Upgrade a hand-written test row to the same canonical media-identity shape
/// that production device scanning provides.
pub fn confirm_row_identity(row: &mut edpcli::cli::Row) {
    use edpcli::application::media_identity::{
        DerivedProtocolEvidence, HardwareIdentityEvidence, IdentityObservation, MediaIdentityPin,
        MediaIdentitySnapshot, ProtocolIdentityEvidence,
    };

    let hardware = HardwareIdentityEvidence {
        total_sectors: Some(row.size / edpcli::application::support::SECTOR as u64),
        logical_sector_size: Some(edpcli::application::support::SECTOR as u32),
        ..HardwareIdentityEvidence::default()
    };
    let snapshot = if row.provision_kind == edpcli::provision::DiskProvisionKind::Plain {
        MediaIdentitySnapshot::plain(
            hardware,
            DerivedProtocolEvidence::default(),
            IdentityObservation::default(),
        )
    } else {
        MediaIdentitySnapshot {
            hardware,
            protocol: ProtocolIdentityEvidence {
                device_id: row.device_id.clone(),
                onlyid: row.onlyid.clone(),
                provision_kind: Some(row.provision_kind),
                lba4_identity_digest: None,
            },
            derived: DerivedProtocolEvidence::default(),
            observation: IdentityObservation::default(),
        }
    };
    row.identity_pin = Some(MediaIdentityPin::new(
        snapshot,
        &vec![0; edpcli::application::support::METADATA_IMAGE_LEN],
    ));
}

/// Explicit EDP inspect context for TUI tests that exercise the protocol tree.
pub fn edp_inspect_context(total_sectors: u64) -> edpcli::inspect::InspectDiskContext {
    use edpcli::application::backup::{Lba7CompatibilityGeometry, PartitionGeometry};

    let mut context = edpcli::inspect::InspectDiskContext::new_with_partition_table(
        vec![0; edpcli::application::support::METADATA_IMAGE_LEN],
        Some("disk&ven_test&prod_test".into()),
        total_sectors,
        Some(edpcli::provision::DiskProvisionKind::Mode0),
        None,
        None,
    );
    let partition = |index, partition_type, start_sector, sector_count| PartitionGeometry {
        index,
        partition_type,
        partition_count: 3,
        need_disturb: 0,
        need_encrypt: u32::from(partition_type != 1),
        start_sector,
        sector_size: edpcli::application::support::SECTOR as u64,
        partition_size: sector_count * edpcli::application::support::SECTOR as u64,
        sector_count,
        user_key_crc: 0,
        file_key_crc: 0,
        encrypt_mode: if partition_type == 1 { 0 } else { 2 },
    };
    context.partitions = vec![
        partition(0, 1, 63, 37),
        partition(1, 2, 2_048, 200),
        partition(2, 4, 2_300, 200),
    ];
    context.lce = Some(Lba7CompatibilityGeometry {
        start_lba: total_sectors.saturating_sub(1_500),
        sector_count: 6,
        lba7_pointer_entries: Vec::new(),
        official_partition_mode: None,
        chs_expected_start_lba: None,
    });
    context
}
