//! 外接盘发现与 cems 只读探测。
//!
//! 这里集中平台设备信息 + LBA4/7/12 的只读探测逻辑，顶层 CLI 只负责路由。

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;

use crate::common::SECTOR;
use crate::identify::identify;
use crate::metainfo;
use crate::platform::system;
use crate::ports::CmdRunner;
use crate::protocol::sectors::{parse_lba12, EdpfPartition};
use crate::protocol::semantic::SemanticContext;
use crate::provision::{DiskProvisionKind, ExistingPartition, ExistingProvisionProfile};

struct ScanPartitionReader<'a> {
    disk: u32,
    start_lba: u64,
    sector_count: u64,
    read_disk: &'a dyn Fn(u32, u32) -> io::Result<Vec<u8>>,
}

impl crate::filesystem::FilesystemReader for ScanPartitionReader<'_> {
    fn sector_size(&self) -> u32 {
        SECTOR as u32
    }

    fn sector_count(&self) -> u64 {
        self.sector_count
    }

    fn read_sector(
        &mut self,
        relative_lba: u64,
    ) -> Result<[u8; SECTOR], crate::filesystem::FilesystemError> {
        if relative_lba >= self.sector_count {
            return Err(crate::filesystem::FilesystemError::new(
                crate::filesystem::FilesystemErrorKind::ReadFailure,
                "文件系统读取超出分区范围",
            ));
        }
        let absolute = self.start_lba.checked_add(relative_lba).ok_or_else(|| {
            crate::filesystem::FilesystemError::new(
                crate::filesystem::FilesystemErrorKind::ReadFailure,
                "文件系统读取 LBA 溢出",
            )
        })?;
        let lba = u32::try_from(absolute).map_err(|_| {
            crate::filesystem::FilesystemError::new(
                crate::filesystem::FilesystemErrorKind::ReadFailure,
                format!("LBA{absolute} 超出当前扫描器 u32 范围"),
            )
        })?;
        let bytes = (self.read_disk)(self.disk, lba).map_err(|error| {
            crate::filesystem::FilesystemError::new(
                crate::filesystem::FilesystemErrorKind::ReadFailure,
                error.to_string(),
            )
        })?;
        bytes.try_into().map_err(|bytes: Vec<u8>| {
            crate::filesystem::FilesystemError::new(
                crate::filesystem::FilesystemErrorKind::ReadFailure,
                format!("LBA{absolute} 读取 {}B，预期 {SECTOR}B", bytes.len()),
            )
        })
    }
}

fn scan_partition_volume_label(
    disk: u32,
    start_lba: u64,
    sector_count: u64,
    kind: crate::filesystem::FilesystemKind,
    read_disk: &dyn Fn(u32, u32) -> io::Result<Vec<u8>>,
) -> Option<String> {
    let registry = crate::filesystem::default_registry();
    let driver = registry.driver(kind)?;
    let mut reader = ScanPartitionReader {
        disk,
        start_lba,
        sector_count,
        read_disk,
    };
    driver.read_metadata(&mut reader).ok()?.volume_label
}

pub struct Row {
    pub disk: u32,
    pub size: u64,
    pub vid: String,
    pub pid: String,
    pub proto: String,
    /// Raw hardware serial for the current scan session only. Do not persist this field.
    pub serial: Option<String>,
    /// Best-effort hardware model from native inquiry; available even for Plain media.
    pub hardware_model: Option<String>,
    pub device_id: Option<String>,
    /// Read-only canonical snapshot pinned to the protocol image seen by this scan.
    pub identity_pin: Option<crate::media_identity::MediaIdentityPin>,
    pub onlyid: Option<String>,
    pub dept: Option<String>,
    pub user: Option<String>,
    pub label: Option<String>,
    pub force_change_password: Option<bool>,
    pub cancel_password_complexity_check: Option<bool>,
    pub max_share_password_errors: Option<u8>,
    pub max_encrypt_password_errors: Option<u8>,
    pub n_baks: usize,
    pub n_possible_baks: usize,
    pub denied: bool,
    pub probe_error: Option<String>,
    pub provision_kind: DiskProvisionKind,
    pub partitions: Option<Vec<EdpfPartition>>,
    pub partition_table: Option<crate::partition_table::PartitionTableSnapshot>,
    /// Validated LBA7-pointed legacy compatibility extent for the current scan.
    pub lce: Option<crate::backup_metadata::Lba7CompatibilityGeometry>,
    pub partition_table_error: Option<String>,
}

impl Row {
    /// Single observed geometry used by capacity planning and TUI layout.
    /// This is per-device, not a mutable process-global setting. Legacy
    /// synthetic views without an observation retain their 512B fixtures.
    pub fn layout_geometry(&self) -> Result<crate::domain::hardware::NativeReadGeometry, String> {
        let logical = match self.identity_pin.as_ref() {
            Some(pin) => pin
                .snapshot
                .hardware
                .logical_sector_size
                .ok_or("设备逻辑扇区大小尚未确认")?,
            None => SECTOR as u32,
        };
        let native = crate::domain::hardware::ObservedDeviceGeometry {
            capacity_bytes: self.size,
            logical_sector_bytes: Some(logical),
            physical_sector_bytes: None,
        }
        .native_read_geometry()
        .map_err(str::to_string)?;
        if let Some(expected) = self
            .identity_pin
            .as_ref()
            .and_then(|pin| pin.snapshot.hardware.total_sectors)
        {
            if expected != native.native_sector_count {
                return Err("身份快照中的原生扇区数量与介质容量不一致".into());
            }
        }
        Ok(native)
    }

    /// UI-only prefill from the scan cache. The physical preparation path reads
    /// and validates the source metadata again before allowing PreserveExact.
    pub fn confirmed_provision_kind(&self) -> Option<DiskProvisionKind> {
        self.identity_pin
            .as_ref()
            .and_then(|pin| pin.snapshot.protocol.provision_kind)
    }

    pub fn existing_profile_for_prefill(&self) -> Option<ExistingProvisionProfile> {
        let mode = self.confirmed_provision_kind()?.official_mode()?;
        let parts = self.partitions.as_ref()?;
        if parts.len() != mode.partition_types().len() {
            return None;
        }
        let geometry = self.layout_geometry().ok()?;
        let sector_bytes = u64::from(geometry.logical_sector_bytes);
        let mut partitions = Vec::with_capacity(parts.len());
        for (index, part) in parts.iter().enumerate() {
            let partition_type = crate::protocol::edpf::EdpPartitionType::from_raw(part.ptype)?;
            if partition_type != mode.partition_types()[index]
                || part.size_bytes == 0
                || !part.size_bytes.is_multiple_of(sector_bytes)
            {
                return None;
            }
            let semantics =
                crate::provision::official_partition_semantics(mode, index, partition_type).ok()?;
            partitions.push(ExistingPartition {
                role: semantics.role,
                partition_type,
                start_lba: part.start_lba,
                sector_count: part.size_bytes / sector_bytes,
                physically_encrypted: semantics.physically_encrypted(),
                filesystem: None,
            });
        }
        Some(ExistingProvisionProfile {
            source_mode: mode,
            partitions,
        })
    }

    pub fn canonical_layout(&self) -> Result<crate::disk_layout::DiskLayoutModel, String> {
        use crate::disk_layout::{DiskLayoutModel, DiskLayoutSegment, DiskRegionKind};

        let geometry = self.layout_geometry()?;
        let total_sectors = geometry.native_sector_count;
        match self.confirmed_provision_kind() {
            Some(DiskProvisionKind::Plain) => {
                let table = self.partition_table.as_ref().ok_or_else(|| {
                    self.partition_table_error
                        .clone()
                        .unwrap_or_else(|| "普通盘分区表尚未完整读取".into())
                })?;
                DiskLayoutModel::canonical_plain(total_sectors, table)?
                    .with_logical_sector_bytes(geometry.logical_sector_bytes)
            }
            Some(_) => {
                let profile = self
                    .existing_profile_for_prefill()
                    .ok_or_else(|| "EDP 分区几何尚未完整确认".to_string())?;
                let lce = self
                    .lce
                    .as_ref()
                    .ok_or_else(|| "LBA7 LCE 几何尚未确认".to_string())?;
                let partitions = profile
                    .partitions
                    .into_iter()
                    .map(|partition| DiskLayoutSegment {
                        label: partition.role.label().into(),
                        start_lba: partition.start_lba,
                        sector_count: partition.sector_count,
                        kind: DiskRegionKind::from_partition_role(partition.role),
                    })
                    .collect();
                DiskLayoutModel::canonical_edp_with_sector_bytes(
                    total_sectors,
                    partitions,
                    lce.start_lba,
                    lce.sector_count,
                    geometry.logical_sector_bytes,
                )
            }
            None => Err("介质类型尚未确认，无法建立可靠容量布局".into()),
        }
    }
}

fn hardware_model(runner: &dyn CmdRunner, disk: u32) -> Option<String> {
    let inquiry = runner.hardware_probe(disk)?.inquiry?;
    let vendor = inquiry.vendor.trim();
    let product = inquiry.product.trim();
    let model = match (vendor.is_empty(), product.is_empty()) {
        (false, false)
            if product
                .to_ascii_lowercase()
                .starts_with(&vendor.to_ascii_lowercase()) =>
        {
            product.to_string()
        }
        (false, false) => format!("{vendor} {product}"),
        (false, true) => vendor.to_string(),
        (true, false) => product.to_string(),
        (true, true) => return None,
    };
    Some(model)
}

/// 外接盘一览数据: 编号/容量/接口; USB 盘再尽力识别 cems 身份、
/// EDPF 分区与备份份数。权限不足和读取异常分开记录。
pub fn scan_disks_with_catalog(
    runner: &dyn CmdRunner,
    read_disk: &dyn Fn(u32, u32) -> io::Result<Vec<u8>>,
    load_catalog: impl FnOnce() -> Result<
        Vec<crate::infrastructure::backup_store::catalog::BackupEntry>,
        String,
    >,
) -> Vec<Row> {
    let mut rows = Vec::new();
    let catalog = std::cell::OnceCell::new();
    let mut load_catalog = Some(load_catalog);
    for d in system::list_external_disks(runner) {
        let mut row = Row {
            disk: d.n,
            size: d.size,
            vid: d.vid.clone(),
            pid: d.pid.clone(),
            proto: d.proto.clone(),
            serial: runner.hardware_serial(d.n),
            hardware_model: hardware_model(runner, d.n),
            device_id: None,
            identity_pin: None,
            onlyid: None,
            dept: None,
            user: None,
            label: None,
            force_change_password: None,
            cancel_password_complexity_check: None,
            max_share_password_errors: None,
            max_encrypt_password_errors: None,
            n_baks: 0,
            n_possible_baks: 0,
            denied: false,
            probe_error: None,
            provision_kind: DiskProvisionKind::Plain,
            partitions: None,
            partition_table: None,
            lce: None,
            partition_table_error: None,
        };
        if d.proto == "USB" {
            let probe = (|| -> io::Result<()> {
                let sector_cache = RefCell::new(BTreeMap::<u32, Vec<u8>>::new());
                let read_exact = |lba: u32| -> io::Result<Vec<u8>> {
                    if let Some(data) = sector_cache.borrow().get(&lba).cloned() {
                        return Ok(data);
                    }
                    let data = read_disk(d.n, lba)?;
                    if data.len() != SECTOR {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            format!(
                                "disk{} LBA{} 读取 {}B，预期 {}B",
                                d.n,
                                lba,
                                data.len(),
                                SECTOR
                            ),
                        ));
                    }
                    sector_cache.borrow_mut().insert(lba, data.clone());
                    Ok(data)
                };
                let lba7 = read_exact(7)?;
                let id = identify(runner, d.n, &lba7);
                row.device_id = id.device_id.clone();
                let lba4 = read_exact(4)?;
                row.onlyid =
                    crate::infrastructure::backup_store::catalog::lba4_label_id_from(&lba4);
                if let Some(did) = &id.device_id {
                    let meta = SemanticContext {
                        device_id: Some(did.clone()),
                        vid: Some(d.vid.clone()),
                        pid: Some(d.pid.clone()),
                        size_bytes: Some(d.size),
                        onlyid: row.onlyid.clone(),
                    };
                    if let Ok(lba8) = read_exact(8) {
                        if let Some(ownership) = metainfo::ownership_from_lba8(&lba8, &meta) {
                            row.dept = ownership.dept;
                            row.user = ownership.user;
                            row.label = ownership.label;
                        }
                    }
                    let lba12 = read_exact(12)?;
                    let native_bytes = system::device_geometry(runner, d.n)
                        .and_then(|geometry| geometry.native_read_geometry().ok())
                        .map_or(SECTOR as u32, |geometry| geometry.logical_sector_bytes);
                    if let Some(kind) = DiskProvisionKind::from_sectors_with_logical_size(
                        &lba7,
                        &lba12,
                        did,
                        native_bytes,
                    ) {
                        row.provision_kind = kind;
                        row.partitions = parse_lba12(&lba12, did);
                        let lba6 = read_exact(6)?;
                        row.label = metainfo::safe6_label_from_lba6(&lba6).or(row.label.take());
                        if let Some(policy) =
                            crate::provision::pass_info_policy_from_sectors(&lba7, &lba12, did)
                        {
                            row.force_change_password = Some(policy.force_change_password);
                            row.cancel_password_complexity_check =
                                Some(policy.cancel_password_complexity_check);
                            row.max_share_password_errors = Some(policy.max_share_password_errors);
                            row.max_encrypt_password_errors =
                                Some(policy.max_encrypt_password_errors);
                        }
                    }
                }
                let mut protocol_image = Vec::with_capacity(crate::common::METADATA_IMAGE_LEN);
                for lba in 0..crate::common::METADATA_SECTOR_COUNT as u32 {
                    protocol_image.extend_from_slice(&read_exact(lba)?);
                }
                let mut identity =
                    crate::media_identity_observer::media_identity_from_protocol_image(
                        runner,
                        d.n,
                        &protocol_image,
                    )
                    .map_err(|error| io::Error::other(error.msg))?;
                let total_sectors = system::device_geometry(runner, d.n)
                    .and_then(|geometry| geometry.native_read_geometry().ok())
                    .map_or(d.size / SECTOR as u64, |geometry| {
                        geometry.native_sector_count
                    });
                identity = crate::media_identity_observer::apply_runtime_plain_override(
                    identity,
                    &protocol_image,
                    total_sectors,
                    |lba| {
                        let lba = u32::try_from(lba)
                            .map_err(|_| format!("LBA{lba} 超出当前扫描器 u32 范围"))?;
                        read_exact(lba).map_err(|error| error.to_string())
                    },
                );
                if identity.protocol.provision_kind == Some(DiskProvisionKind::Plain) {
                    row.device_id = None;
                    row.onlyid = None;
                    row.dept = None;
                    row.user = None;
                    row.label = None;
                    row.force_change_password = None;
                    row.cancel_password_complexity_check = None;
                    row.max_share_password_errors = None;
                    row.max_encrypt_password_errors = None;
                    row.provision_kind = DiskProvisionKind::Plain;
                    row.partitions = None;
                    row.lce = None;
                } else if let Some(kind) = identity.protocol.provision_kind {
                    row.provision_kind = kind;
                }
                if identity.protocol.provision_kind != Some(DiskProvisionKind::Plain) {
                    if let Some(device_id) = identity.protocol.device_id.as_deref() {
                        // The identity snapshot is pinned only after the scan's
                        // backup matching phase; use the SAME observed source
                        // geometry here rather than a premature Row fallback.
                        let observed = crate::domain::hardware::ObservedDeviceGeometry {
                            capacity_bytes: row.size,
                            logical_sector_bytes: identity.hardware.logical_sector_size,
                            physical_sector_bytes: None,
                        };
                        row.lce = observed.native_read_geometry().ok().and_then(|geometry| {
                            crate::domain::geometry::parse_lba7_compatibility_geometry_with_sector_bytes(
                                &protocol_image,
                                device_id,
                                geometry.native_sector_count,
                                geometry.logical_sector_bytes,
                            )
                            .ok()
                        });
                    }
                }

                if identity.protocol.provision_kind == Some(DiskProvisionKind::Plain) {
                    match crate::partition_table::read_partition_table(total_sectors, |lba| {
                        let lba = u32::try_from(lba)
                            .map_err(|_| format!("LBA{lba} 超出当前扫描器 u32 范围"))?;
                        read_exact(lba).map_err(|error| error.to_string())
                    }) {
                        Ok(mut table) => {
                            for partition in &mut table.partitions {
                                let Ok(start) = u32::try_from(partition.start_lba) else {
                                    continue;
                                };
                                let Ok(boot) = read_exact(start) else {
                                    continue;
                                };
                                let detected = crate::filesystem::detect_boot_sector_with_geometry(
                                    partition.start_lba,
                                    partition.sector_count,
                                    &boot,
                                )
                                .ok()
                                .flatten();
                                partition.filesystem =
                                    detected.map(|filesystem| filesystem.label().to_string());
                                partition.volume_label = detected.and_then(|filesystem| {
                                    scan_partition_volume_label(
                                        d.n,
                                        partition.start_lba,
                                        partition.sector_count,
                                        filesystem,
                                        read_disk,
                                    )
                                });
                            }
                            row.partition_table = Some(table);
                        }
                        Err(error) => row.partition_table_error = Some(error),
                    }
                }

                let entries = catalog
                    .get_or_init(|| load_catalog.take().expect("catalog loader called once")());
                let entries = entries
                    .as_ref()
                    .map_err(|error| io::Error::other(error.clone()))?;
                let matches = crate::infrastructure::backup_store::create::match_backup_entries(
                    entries, &identity,
                );
                row.identity_pin = Some(crate::media_identity::MediaIdentityPin::new(
                    identity,
                    &protocol_image,
                ));
                row.n_baks = matches.confirmed.len();
                row.n_possible_baks = matches.possible.len();
                Ok(())
            })();
            if let Err(e) = probe {
                if e.kind() == io::ErrorKind::PermissionDenied {
                    row.denied = true;
                } else {
                    row.probe_error = Some(e.to_string());
                }
            }
        } else if d.proto == "Disk Image"
            && crate::platform::include_virtual()
            && crate::platform::confirmed_virtual_disk_image(runner, d.n)
        {
            // Same native read path and source classifier used by the actual
            // CLI/TUI provisioning application. No 512B legacy reader here.
            let result = (|| -> io::Result<()> {
                use crate::diskio::NativeBlockDevice;
                let geometry = system::device_geometry(runner, d.n)
                    .ok_or_else(|| io::Error::other("磁盘镜像原生几何无法确认"))?
                    .native_read_geometry()
                    .map_err(io::Error::other)?;
                let probe =
                    system::native_provision_probe(runner, d.n).map_err(io::Error::other)?;
                let did = crate::provision::TargetIdentity::from_probe(
                    &probe,
                    geometry.native_sector_count,
                )
                .map_err(io::Error::other)?
                .device_id()
                .to_string();
                let mut dev = crate::diskio::NativeRawBlockDevice::open_readonly(
                    &crate::platform::raw_disk_path(d.n),
                    geometry,
                )?;
                let mut prefix = Vec::with_capacity(13);
                for lba in 0..13 {
                    prefix.push(dev.read_block_fresh(lba)?);
                }
                let mode = crate::provision::classify_native_source(
                    &prefix,
                    geometry.logical_sector_bytes,
                    geometry.native_sector_count,
                    &did,
                )
                .map_err(io::Error::other)?;
                let projection = prefix
                    .iter()
                    .flat_map(|block| block[..SECTOR].iter().copied())
                    .collect::<Vec<_>>();
                row.provision_kind = mode;
                row.vid = probe
                    .vid
                    .map(|v| format!("{v:04x}"))
                    .unwrap_or("xxxx".into());
                row.pid = probe
                    .pid
                    .map(|v| format!("{v:04x}"))
                    .unwrap_or("xxxx".into());
                let mut snapshot = crate::media_identity::MediaIdentitySnapshot::default();
                snapshot.hardware.logical_sector_size = Some(geometry.logical_sector_bytes);
                snapshot.hardware.total_sectors = Some(geometry.native_sector_count);
                snapshot.hardware.vid = probe.vid;
                snapshot.hardware.pid = probe.pid;
                snapshot.protocol.provision_kind = Some(mode);
                if mode != DiskProvisionKind::Plain {
                    snapshot.protocol.device_id = Some(did.clone());
                    row.device_id = Some(did.clone());
                    row.onlyid = crate::infrastructure::backup_store::catalog::lba4_label_id_from(
                        &prefix[4][..SECTOR],
                    );
                    snapshot.protocol.onlyid = row.onlyid.clone();
                    row.partitions = parse_lba12(&prefix[12][..SECTOR], &did);
                    row.lce = crate::domain::geometry::parse_lba7_compatibility_geometry_with_sector_bytes(
                        &projection, &did, geometry.native_sector_count,
                        geometry.logical_sector_bytes,
                    ).ok();
                }
                row.identity_pin = Some(crate::media_identity::MediaIdentityPin::new(
                    snapshot,
                    &projection,
                ));
                Ok(())
            })();
            if let Err(e) = result {
                if e.kind() == io::ErrorKind::PermissionDenied {
                    row.denied = true;
                } else {
                    row.probe_error = Some(e.to_string());
                }
            }
        }
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn refresh_loads_one_catalog_for_multiple_disks_and_none_if_denied() {
        use crate::platform::test_support::MultiUsbRunner;
        let mut mbr = vec![0; SECTOR];
        mbr[510..].copy_from_slice(&[0x55, 0xaa]);
        mbr[0x1be + 4] = 0x07;
        mbr[0x1be + 8..0x1be + 12].copy_from_slice(&2048u32.to_le_bytes());
        mbr[0x1be + 12..0x1be + 16].copy_from_slice(&30720u32.to_le_bytes());
        let reads = |_: u32, lba: u32| {
            Ok(if lba == 0 {
                mbr.clone()
            } else {
                vec![0; SECTOR]
            })
        };
        let loads = std::cell::Cell::new(0);
        let rows = scan_disks_with_catalog(&MultiUsbRunner, &reads, || {
            loads.set(loads.get() + 1);
            Ok(Vec::new())
        });
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter().all(|row| row.probe_error.is_none()),
            "{:?}",
            rows.iter().map(|row| &row.probe_error).collect::<Vec<_>>()
        );
        assert_eq!(loads.get(), 1);
        let denied =
            |_: u32, _: u32| Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        let rows = scan_disks_with_catalog(&MultiUsbRunner, &denied, || {
            panic!("denied disks must not scan backups")
        });
        assert!(rows.iter().all(|row| row.denied));
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "macOS-only P0 device discovery fixture; run with --ignored"]
    fn mock_device_scan_benchmark() {
        use crate::platform::test_support::MultiUsbRunner;
        use std::time::Instant;
        let mut mbr = vec![0_u8; SECTOR];
        mbr[510..].copy_from_slice(&[0x55, 0xaa]);
        mbr[0x1be + 4] = 0x07;
        mbr[0x1be + 8..0x1be + 12].copy_from_slice(&2048_u32.to_le_bytes());
        mbr[0x1be + 12..0x1be + 16].copy_from_slice(&30720_u32.to_le_bytes());
        let reads = |_: u32, lba: u32| {
            Ok(if lba == 0 {
                mbr.clone()
            } else {
                vec![0_u8; SECTOR]
            })
        };
        let loops = 300;
        let started = Instant::now();
        for _ in 0..loops {
            let rows = scan_disks_with_catalog(&MultiUsbRunner, &reads, || Ok(Vec::new()));
            assert_eq!(rows.len(), 2);
        }
        println!(
            "mock_device_scan_disks=2 loops={loops} avg_ms={:.4}",
            started.elapsed().as_secs_f64() * 1000.0 / f64::from(loops)
        );
    }

    #[test]
    fn scan_partition_volume_label_reads_real_fat16_metadata() {
        let start_lba = 63u64;
        let sector_count = 20_417u64;
        let image = crate::filesystem::build_empty_filesystem(
            crate::filesystem::FilesystemKind::Fat16,
            start_lba,
            sector_count,
            0x1234_5678,
            Some("MYBOOT"),
        )
        .unwrap();
        let read_disk = |disk: u32, lba: u32| -> io::Result<Vec<u8>> {
            assert_eq!(disk, 6);
            let relative = u64::from(lba)
                .checked_sub(start_lba)
                .ok_or_else(|| io::Error::other("read before partition"))?;
            image
                .sector_or_zero(relative)
                .map(|sector| sector.to_vec())
                .ok_or_else(|| io::Error::other("read beyond partition"))
        };

        assert_eq!(
            scan_partition_volume_label(
                6,
                start_lba,
                sector_count,
                crate::filesystem::FilesystemKind::Fat16,
                &read_disk,
            ),
            Some("MYBOOT".into())
        );
    }
}
