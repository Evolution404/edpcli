//! Shared provisioning application service.
//!
//! This layer resolves a real USB target, creates the exact pure-domain write
//! plan, and owns the final raw-device safety transition. CLI/TUI must not
//! duplicate these checks or construct alternative raw-write patches.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

use crate::common::{EdpCliError, EdpCliResult, EXIT_IO, EXIT_TARGET, SECTOR};
use crate::diskio::{self, SectorDev};
use crate::identify::identify;
use crate::protocol::lba7_compat::locate_lba7_compatibility_extent_from_verified_usb_capacity;
use crate::provision::{
    build_official_provision_write_image, build_passwordless_conversion, wrap_file_key,
    wrap_legacy_lba7_file_key, FileKeyWrapMode, OfficialPartitionMode, OfficialPartitionSizes,
    OfficialProvisionPlan, OfficialProvisionWriteImage, OnlyId, PasswordlessConversionImage,
    ProvisionEntropy, ProvisionImage, ProvisionMetadata, ProvisionProfile, ProvisionSpec,
    TargetIdentity,
};
use crate::sysinfo::{self, CmdRunner};

use super::device::guard_usb_disk;
use super::write::{read_image, verify_reopened_snapshot};

const OPEN_WAIT: Duration = Duration::from_secs(10);

fn err(code: i32, message: impl Into<String>) -> EdpCliError {
    EdpCliError::new(code, message)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewProvisionRequest {
    pub mode: u8,
    pub boot_mib: Option<u64>,
    pub share_mib: Option<u64>,
    pub encrypt_mib: Option<u64>,
    pub label_id: String,
    pub user: String,
    pub dept: String,
    pub label: String,
    pub password: String,
    pub volume_label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedNewProvision {
    pub disk: u32,
    pub device_id: String,
    pub mode: OfficialPartitionMode,
    pub lce_start_lba: u64,
    pub write_image: OfficialProvisionWriteImage,
    expected_probe: crate::platform::HardwareProbe,
    expected_lba3: Option<[u8; SECTOR]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedPasswordlessConversion {
    pub disk: u32,
    pub device_id: String,
    pub source_metadata: Vec<u8>,
    pub conversion: PasswordlessConversionImage,
    pub patch: BTreeMap<u32, Vec<u8>>,
}

fn random_array<const N: usize>() -> EdpCliResult<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|error| err(EXIT_IO, format!("错误: 系统随机数生成失败: {error}")))?;
    Ok(bytes)
}

fn mode(value: u8) -> EdpCliResult<OfficialPartitionMode> {
    match value {
        0 => Ok(OfficialPartitionMode::DefaultThreePartition),
        1 => Ok(OfficialPartitionMode::BootShareCombined),
        2 => Ok(OfficialPartitionMode::WholeDiskEncrypted),
        3 => Ok(OfficialPartitionMode::IntranetExtranetDualPartition),
        _ => Err(err(
            EXIT_TARGET,
            format!("错误: 不支持的官方制盘模式 {value}"),
        )),
    }
}

fn sizes(request: &NewProvisionRequest) -> OfficialPartitionSizes {
    // Unused fields are ignored by the official mode; keep a non-zero sentinel
    // so domain validation cannot accidentally turn an unused value into a
    // zero-size emitted partition if a mode definition changes later.
    OfficialPartitionSizes::new(
        request.boot_mib.unwrap_or(1),
        request.share_mib.unwrap_or(1),
        request.encrypt_mib.unwrap_or(1),
    )
}

pub fn prepare_new_provision(
    runner: &dyn CmdRunner,
    disk: u32,
    request: &NewProvisionRequest,
) -> EdpCliResult<PreparedNewProvision> {
    guard_usb_disk(runner, disk)?;
    let total_sectors = sysinfo::disk_total_sectors(runner, disk)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘总扇区数"))?;
    let probe = runner
        .hardware_probe(disk)
        .or_else(|| crate::platform::fallback_hardware_probe(runner, disk))
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘 USB/SCSI 硬件身份"))?;
    let target = TargetIdentity::from_probe(&probe, total_sectors)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 目标硬件身份不完整: {message}")))?;
    let device_id = target.device_id().to_string();
    let compatibility =
        locate_lba7_compatibility_extent_from_verified_usb_capacity(total_sectors, SECTOR as u32)
            .ok_or_else(|| {
            err(
                EXIT_TARGET,
                "错误: 当前目标不符合已验证的 512B/255x63 USB LCE 几何",
            )
        })?;

    let metadata = ProvisionMetadata::new(
        OnlyId::parse(&request.label_id)
            .map_err(|message| err(EXIT_TARGET, format!("错误: 标签标识无效: {message}")))?,
        request.user.clone(),
        request.dept.clone(),
        request.label.clone(),
    )
    .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘身份字段无效: {message}")))?;
    let spec = ProvisionSpec::new(target, metadata, ProvisionProfile::canonical_v1())
        .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘元数据无法编码: {message}")))?;

    let file_key = random_array::<16>()?;
    let legacy_file_key = random_array::<8>()?;
    let entropy = ProvisionEntropy::new(random_array::<252>()?);
    let current_key = wrap_file_key(request.password.as_bytes(), file_key, FileKeyWrapMode::Sm4);
    let legacy_key = wrap_legacy_lba7_file_key(request.password.as_bytes(), legacy_file_key);
    let selected_mode = mode(request.mode)?;
    let plan = OfficialProvisionPlan::new(
        selected_mode,
        sizes(request),
        compatibility,
        legacy_key,
        current_key,
    )
    .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘布局无效: {message}")))?;
    let logical_count = plan
        .logical_partitions(SECTOR as u64)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 制盘布局无效: {message}")))?
        .len();
    let mut serials = Vec::with_capacity(logical_count);
    for _ in 0..logical_count {
        serials.push(u32::from_le_bytes(random_array::<4>()?));
    }
    let write_image = build_official_provision_write_image(
        &spec,
        &entropy,
        &plan,
        &file_key,
        &request.volume_label,
        &serials,
    )
    .map_err(|message| err(EXIT_TARGET, format!("错误: 无法构造制盘镜像: {message}")))?;

    Ok(PreparedNewProvision {
        disk,
        device_id,
        mode: selected_mode,
        lce_start_lba: compatibility.start_lba,
        write_image,
        expected_probe: probe,
        expected_lba3: None,
    })
}

pub fn capture_manufacturer_lba3(
    dev: &mut dyn SectorDev,
    prepared: &mut PreparedNewProvision,
) -> EdpCliResult<()> {
    let raw = dev
        .read_sector(3)
        .map_err(|error| err(EXIT_IO, format!("错误: 读取目标 LBA3 失败: {error}")))?;
    let lba3: [u8; SECTOR] = raw.try_into().map_err(|raw: Vec<u8>| {
        err(
            EXIT_IO,
            format!("错误: 目标 LBA3 长度为 {}B，预期 {SECTOR}B", raw.len()),
        )
    })?;
    prepared.write_image.patch.insert(3, lba3.to_vec());
    prepared.expected_lba3 = Some(lba3);
    Ok(())
}

fn build_conversion_patch(
    conversion: &PasswordlessConversionImage,
) -> EdpCliResult<BTreeMap<u32, Vec<u8>>> {
    let mut patch = BTreeMap::new();
    patch.insert(0, conversion.lba0.to_vec());
    patch.insert(7, conversion.lba7.to_vec());
    patch.insert(12, conversion.lba12.to_vec());
    for (&relative, sector) in conversion.front_filesystem.sectors() {
        let absolute = conversion
            .plan
            .front_start_lba
            .checked_add(relative)
            .ok_or_else(|| err(EXIT_TARGET, "错误: 前部文件系统 LBA 溢出"))?;
        if absolute >= conversion.plan.encrypt_start_lba || absolute > u32::MAX as u64 {
            return Err(err(
                EXIT_TARGET,
                format!("错误: 前部文件系统越过 type4 边界: LBA{absolute}"),
            ));
        }
        patch.insert(absolute as u32, sector.to_vec());
    }
    Ok(patch)
}

pub fn prepare_passwordless_conversion(
    runner: &dyn CmdRunner,
    disk: u32,
    dev: &mut dyn SectorDev,
) -> EdpCliResult<PreparedPasswordlessConversion> {
    guard_usb_disk(runner, disk)?;
    let source_metadata = read_image(dev)?;
    let identity = identify(runner, disk, &source_metadata[7 * SECTOR..8 * SECTOR]);
    let device_id = identity.device_id.ok_or_else(|| {
        err(
            EXIT_TARGET,
            "错误: 无法从现有官方盘识别 device_id，拒绝生成免密转换",
        )
    })?;
    let source = ProvisionImage::from_bytes(source_metadata.clone())
        .map_err(|message| err(EXIT_TARGET, format!("错误: 源协议镜像无效: {message}")))?;
    let volume_serial = u32::from_le_bytes(random_array::<4>()?);
    let conversion = build_passwordless_conversion(&source, &device_id, volume_serial, "SAFE6")
        .map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 无法生成严格免密转换: {message}"),
            )
        })?;
    let patch = build_conversion_patch(&conversion)?;
    Ok(PreparedPasswordlessConversion {
        disk,
        device_id,
        source_metadata,
        conversion,
        patch,
    })
}

pub fn commit_passwordless_conversion(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedPasswordlessConversion,
) -> EdpCliResult<()> {
    let _guard = sysinfo::prepare_write(runner, prepared.disk).map_err(|error| {
        err(
            EXIT_IO,
            format!("错误: 无法卸载/锁定 disk{}: {error}", prepared.disk),
        )
    })?;
    dev.reopen_rdwr(OPEN_WAIT)
        .map_err(|error| err(EXIT_IO, format!("错误: 无法以读写方式重开目标盘: {error}")))?;
    verify_reopened_snapshot(dev, &prepared.source_metadata)?;
    diskio::atomic_write_passwordless_conversion_sectors(
        dev,
        &prepared.patch,
        prepared.conversion.plan.encrypt_start_lba,
    )
}

pub fn commit_new_provision(
    runner: &dyn CmdRunner,
    dev: &mut dyn SectorDev,
    prepared: &PreparedNewProvision,
) -> EdpCliResult<()> {
    guard_usb_disk(runner, prepared.disk)?;
    let _guard = sysinfo::prepare_write(runner, prepared.disk).map_err(|error| {
        err(
            EXIT_IO,
            format!("错误: 无法卸载/锁定 disk{}: {error}", prepared.disk),
        )
    })?;
    dev.reopen_rdwr(OPEN_WAIT)
        .map_err(|error| err(EXIT_IO, format!("错误: 无法以读写方式重开目标盘: {error}")))?;
    let fresh_probe = runner
        .hardware_probe(prepared.disk)
        .or_else(|| crate::platform::fallback_hardware_probe(runner, prepared.disk))
        .ok_or_else(|| err(EXIT_TARGET, "错误: 重开后无法复核目标硬件身份"))?;
    let fresh_total = sysinfo::disk_total_sectors(runner, prepared.disk)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 重开后无法复核目标容量"))?;
    if fresh_probe != prepared.expected_probe || fresh_total != prepared.write_image.total_sectors {
        return Err(err(
            EXIT_TARGET,
            "错误: 制盘确认/卸载期间目标硬件身份或容量发生变化，疑似换盘，拒绝写入",
        ));
    }
    let expected_lba3 = prepared.expected_lba3.ok_or_else(|| {
        err(
            EXIT_TARGET,
            "错误: 制盘写入前未捕获制造商 LBA3，拒绝覆盖不透明制造商元数据",
        )
    })?;
    let fresh_lba3 = dev
        .read_sector(3)
        .map_err(|error| err(EXIT_IO, format!("错误: 重开后复核 LBA3 失败: {error}")))?;
    if fresh_lba3.as_slice() != expected_lba3 {
        return Err(err(
            EXIT_TARGET,
            "错误: 制盘确认/卸载期间制造商 LBA3 发生变化，拒绝写入",
        ));
    }
    diskio::atomic_write_official_provision_sectors(
        dev,
        &prepared.write_image.patch,
        prepared.write_image.total_sectors,
    )
}

pub fn export_sparse_provision_image(
    path: &Path,
    prepared: &PreparedNewProvision,
) -> EdpCliResult<()> {
    let byte_len = prepared
        .write_image
        .total_sectors
        .checked_mul(SECTOR as u64)
        .ok_or_else(|| err(EXIT_IO, "错误: 镜像长度溢出"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| {
            err(
                EXIT_IO,
                format!("错误: 无法创建 {}: {error}", path.display()),
            )
        })?;
    file.set_len(byte_len)
        .map_err(|error| err(EXIT_IO, format!("错误: 无法设置镜像长度: {error}")))?;
    for (&lba, sector) in &prepared.write_image.patch {
        file.seek(SeekFrom::Start(u64::from(lba) * SECTOR as u64))
            .and_then(|_| file.write_all(sector))
            .map_err(|error| err(EXIT_IO, format!("错误: 写入镜像 LBA{lba} 失败: {error}")))?;
    }
    file.sync_all()
        .map_err(|error| err(EXIT_IO, format!("错误: 镜像同步失败: {error}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct Lba3Dev([u8; SECTOR]);

    impl SectorDev for Lba3Dev {
        fn read_sector(&mut self, lba: u32) -> io::Result<Vec<u8>> {
            if lba != 3 {
                return Err(io::Error::other("unexpected read"));
            }
            Ok(self.0.to_vec())
        }

        fn write_sector(&mut self, _lba: u32, _data: &[u8]) -> io::Result<()> {
            Err(io::Error::other("read-only test device"))
        }
    }

    #[test]
    fn manufacturer_lba3_is_copied_verbatim_into_the_write_plan() {
        let metadata = ProvisionImage::from_bytes(vec![0; 13 * SECTOR]).unwrap();
        let mut patch = BTreeMap::new();
        patch.insert(3, vec![0; SECTOR]);
        let probe = crate::platform::HardwareProbe {
            vid: Some(0x0dd8),
            pid: Some(0x2005),
            transport: crate::platform::NativeTransport::Uas,
            inquiry: None,
        };
        let mut prepared = PreparedNewProvision {
            disk: 4,
            device_id: "disk&ven_netac&prod_onlydisk".into(),
            mode: OfficialPartitionMode::BootShareCombined,
            lce_start_lba: 900,
            write_image: OfficialProvisionWriteImage {
                metadata,
                total_sectors: 1024,
                patch,
            },
            expected_probe: probe,
            expected_lba3: None,
        };
        let expected = [0xa5; SECTOR];
        let mut dev = Lba3Dev(expected);
        capture_manufacturer_lba3(&mut dev, &mut prepared).unwrap();
        assert_eq!(prepared.write_image.patch.get(&3).unwrap(), &expected);
        assert_eq!(prepared.expected_lba3, Some(expected));
    }
}
