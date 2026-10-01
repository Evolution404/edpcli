//! 外接盘发现与 cems 只读探测。
//!
//! 这里集中平台设备信息 + LBA4/7/12 的只读探测逻辑，顶层 CLI 只负责路由。

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use crate::common::SECTOR;
use crate::diskio::{self, find_backups};
use crate::identify::identify;
use crate::metainfo;
use crate::protocol::semantic::SemanticContext;
use crate::provision::{
    DiskProvisionKind, ExistingPartition, ExistingProvisionProfile, PartitionRole,
};
use crate::sectors::{parse_lba12, EdpfPartition};
use crate::sysinfo::{self, CmdRunner};

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
        let mut partitions = Vec::with_capacity(parts.len());
        for (index, part) in parts.iter().enumerate() {
            let partition_type = crate::protocol::edpf::EdpPartitionType::from_raw(part.ptype)?;
            if partition_type != mode.partition_types()[index]
                || part.size_bytes == 0
                || part.size_bytes % SECTOR as u64 != 0
            {
                return None;
            }
            let role = crate::provision::official_partition_role(mode, index, partition_type);
            partitions.push(ExistingPartition {
                role,
                partition_type,
                start_lba: part.start_lba,
                sector_count: part.size_bytes / SECTOR as u64,
                physically_encrypted: matches!(role, PartitionRole::Share | PartitionRole::Encrypt),
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

        let total_sectors = self.size / SECTOR as u64;
        match self.confirmed_provision_kind() {
            Some(DiskProvisionKind::Plain) => {
                let table = self.partition_table.as_ref().ok_or_else(|| {
                    self.partition_table_error
                        .clone()
                        .unwrap_or_else(|| "普通盘分区表尚未完整读取".into())
                })?;
                DiskLayoutModel::canonical_plain(total_sectors, table)
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
                DiskLayoutModel::canonical_edp(
                    total_sectors,
                    partitions,
                    lce.start_lba,
                    lce.sector_count,
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
pub fn scan_disks(
    runner: &dyn CmdRunner,
    backup_dir: &Path,
    read_disk: &dyn Fn(u32, u32) -> io::Result<Vec<u8>>,
) -> Vec<Row> {
    let mut rows = Vec::new();
    for d in sysinfo::list_external_disks(runner) {
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
                row.onlyid = diskio::lba4_label_id_from(&lba4);
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
                    if let Some(kind) = DiskProvisionKind::from_sectors(&lba7, &lba12, did) {
                        row.provision_kind = kind;
                        row.partitions = parse_lba12(&lba12, did);
                        let lba6 = read_exact(6)?;
                        row.label =
                            metainfo::safe6_label_from_lba6(&lba6, &meta).or(row.label.take());
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
                let total_sectors = d.size / SECTOR as u64;
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
                        row.lce = crate::backup_metadata::parse_lba7_compatibility_geometry(
                            &protocol_image,
                            device_id,
                            total_sectors,
                        )
                        .ok();
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
                                partition.filesystem =
                                    crate::filesystem::detect_boot_sector_with_geometry(
                                        partition.start_lba,
                                        partition.sector_count,
                                        &boot,
                                    )
                                    .ok()
                                    .flatten()
                                    .map(|filesystem| filesystem.label().to_string());
                            }
                            row.partition_table = Some(table);
                        }
                        Err(error) => row.partition_table_error = Some(error),
                    }
                }

                let matches = find_backups(backup_dir, &identity);
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
        }
        rows.push(row);
    }
    rows
}
