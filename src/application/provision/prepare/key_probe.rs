//! Read-only key-domain probing and source-password verification.

use super::*;

/// The 4Kn source path is a completely READ-ONLY evidence capture. It uses
/// full native LBA0..12 blocks, a 512B-per-LBA fixed wire projection, and
/// native-LBA geometry. It does not call the legacy 512B SectorDev reader or
/// open any write handle.
fn native_registration_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
) -> EdpCliResult<(
    crate::provision::DiskProvisionKind,
    Option<ParsedExistingProvision>,
)> {
    let source = crate::application::evidence::EvidenceSource::open_disk(runner, disk)
        .map_err(|error| err(EXIT_TARGET, format!("错误: 4Kn来源只读取证失败: {error}")))?;
    let native = source
        .native_protocol_image()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 来源不具备完整原生协议只读镜像"))?;
    if native.logical_sector_bytes() != 4096 {
        return Err(err(EXIT_TARGET, "错误: 来源逻辑扇区并非已认证4096B"));
    }
    let identity = source.identity();
    let kind = identity
        .provision_kind
        .ok_or_else(|| err(EXIT_TARGET, "错误: 4Kn来源盘型尚未确认，不能自动验证密码"))?;
    if kind == crate::provision::DiskProvisionKind::Plain {
        return Ok((kind, None));
    }
    let did = identity
        .device_id
        .as_deref()
        .filter(|did| {
            did.starts_with("disk&ven_")
                && did.len() <= 128
                && did
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'&' | b'.' | b'-'))
        })
        .ok_or_else(|| {
            err(
                EXIT_TARGET,
                "错误: 4Kn来源device_id不完整或不可信，不能验证密码",
            )
        })?;
    let parsed =
        crate::provision::parse_existing_provision_native(native, did, source.total_sectors())
            .map_err(|message| {
                err(
                    EXIT_TARGET,
                    format!("错误: 4Kn来源EDPF协议几何或密钥结构不可信: {message}"),
                )
            })?
            .ok_or_else(|| err(EXIT_TARGET, "错误: 4Kn来源没有完整且可验证的EDPF密码域"))?;
    if kind.official_mode() != Some(parsed.profile.source_mode) {
        return Err(err(EXIT_TARGET, "错误: 4Kn来源盘型与完整EDPF模式不一致"));
    }
    Ok((kind, Some(parsed)))
}

/// The native U391 source uses EncryptMode=3 (AES-128-ECB), unlike the
/// first-party 512B SM4 writer. The existing EDPF key-record verifier already
/// covers all documented wrap modes and validates both password and FileKey
/// CRC; native READ ONLY probing must honor the source record's actual mode.
fn verified_native_source_password(
    record: crate::provision::ExistingPartitionRecord,
    password: &[u8],
) -> Result<SourcePasswordKnowledge, String> {
    if record.lba12.need_encrypt == 0
        || FileKeyWrapMode::from_raw(record.lba12.encrypt_mode).is_none()
    {
        return Err("来源密码域使用未认证的 FileKey 封装类型".into());
    }
    record
        .verified_file_key(Some(password))
        .map_err(|error| error.to_string())?;
    Ok(if password == DEFAULT_KEY_DOMAIN_PASSWORD {
        SourcePasswordKnowledge::DefaultVerified
    } else {
        SourcePasswordKnowledge::UserVerified
    })
}

fn native_key_probe_on_disk(runner: &dyn CmdRunner, disk: u32) -> EdpCliResult<ProvisionKeyProbe> {
    let (source_kind, parsed) = native_registration_on_disk(runner, disk)?;
    let classify = |domain: KeyDomainRole| {
        let record = parsed
            .as_ref()
            .and_then(|source| source.record_for_domain(domain));
        let Some(record) = record else {
            return (None, false);
        };
        // Source AES-128-ECB can be *password-verified* but the first-party
        // target write/sector-crypto contract remains SM4-only. Keep the
        // opaque-compatibility warning for AES; never imply write support.
        let supported = record.lba12.need_encrypt != 0
            && FileKeyWrapMode::from_raw(record.lba12.encrypt_mode) == Some(FileKeyWrapMode::Sm4);
        let knowledge = verified_native_source_password(*record, DEFAULT_KEY_DOMAIN_PASSWORD)
            .unwrap_or(SourcePasswordKnowledge::Unknown);
        (Some(knowledge), !supported)
    };
    let (share, share_opaque_profile) = classify(KeyDomainRole::Share);
    let (encrypt, encrypt_opaque_profile) = classify(KeyDomainRole::Encrypt);
    Ok(ProvisionKeyProbe {
        source_kind,
        share,
        share_opaque_profile,
        encrypt,
        encrypt_opaque_profile,
    })
}

fn native_source_password_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    domain: KeyDomainRole,
    password: &[u8],
) -> EdpCliResult<SourcePasswordKnowledge> {
    let (_, parsed) = native_registration_on_disk(runner, disk)?;
    let source =
        parsed.ok_or_else(|| err(EXIT_TARGET, "错误: 4Kn来源未注册EDP密码域，拒绝密码验证"))?;
    let record = source
        .record_for_domain(domain)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 4Kn来源模式不包含指定密码域"))?;
    verified_native_source_password(*record, password).map_err(|message| {
        err(
            EXIT_TARGET,
            format!("错误: 4Kn来源原密码/FileKey校验失败: {message}"),
        )
    })
}

/// Choose by observed physical sector geometry rather than infer from EDP
/// protocol contents. Unknown geometry preserves the historical 512B reader
/// behavior; an observed unsupported non-512B size fails closed.
fn source_native_sector_bytes(runner: &dyn CmdRunner, disk: u32) -> EdpCliResult<Option<u32>> {
    let observed = crate::platform::system::device_geometry(runner, disk)
        .and_then(|geometry| geometry.logical_sector_bytes);
    validate_source_password_sector_bytes(observed)
}

fn validate_source_password_sector_bytes(observed: Option<u32>) -> EdpCliResult<Option<u32>> {
    if observed.is_some_and(|bytes| !matches!(bytes, 512 | 4096)) {
        return Err(err(
            EXIT_TARGET,
            "错误: 原密码自动验证仅认证512B与4096B逻辑扇区",
        ));
    }
    Ok(observed)
}

pub fn probe_provision_key_domains_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
) -> EdpCliResult<ProvisionKeyProbe> {
    if source_native_sector_bytes(runner, disk)? == Some(4096) {
        return native_key_probe_on_disk(runner, disk);
    }
    let target_session = TargetSession::<ReadOnly>::open_usb(runner, disk)?;
    let total_sectors = target_session
        .total_sectors()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘总扇区数"))?;
    let probe = target_session
        .hardware_probe()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘 USB/SCSI 硬件身份"))?;
    let target = TargetIdentity::from_probe(&probe, total_sectors)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 目标硬件身份不完整: {message}")))?;
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    let source_metadata = read_image(&mut dev)?;
    let source_identity =
        classify_live_source_identity(runner, disk, &source_metadata, total_sectors, &mut dev)?;
    let source_kind = source_identity
        .protocol
        .provision_kind
        .ok_or_else(|| err(EXIT_TARGET, "错误: 来源盘型未确认；拒绝继续探测密码域"))?;
    let source_device_id = source_protocol_device_id(&target, &source_identity);
    if source_kind == crate::provision::DiskProvisionKind::Plain {
        return Ok(ProvisionKeyProbe {
            source_kind,
            share: None,
            share_opaque_profile: false,
            encrypt: None,
            encrypt_opaque_profile: false,
        });
    }
    let image = ProvisionImage::from_bytes(source_metadata.clone())
        .map_err(|message| err(EXIT_TARGET, format!("错误: 来源元数据长度无效: {message}")))?;
    let parsed =
        parse_existing_provision(&image, &source_device_id, total_sectors).map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 来源盘注册结构无法可靠解析: {message}"),
            )
        })?;
    let domain_probe = |domain: KeyDomainRole| {
        parsed
            .as_ref()
            .and_then(|source| {
                source
                    .record_for_domain(domain)
                    .map(|record| (source, record))
            })
            .map(|(source, record)| {
                (
                    Some(source.source_password_knowledge(domain, None)),
                    record.lba12.need_encrypt != 0
                        && FileKeyWrapMode::from_raw(record.lba12.encrypt_mode)
                            == Some(FileKeyWrapMode::Sm4),
                )
            })
            .unwrap_or((None, false))
    };
    let (share, share_opaque_profile) = domain_probe(KeyDomainRole::Share);
    let (encrypt, encrypt_opaque_profile) = domain_probe(KeyDomainRole::Encrypt);
    Ok(ProvisionKeyProbe {
        source_kind,
        share,
        share_opaque_profile,
        encrypt,
        encrypt_opaque_profile,
    })
}

pub fn verify_provision_source_password_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
    domain: KeyDomainRole,
    password: &[u8],
) -> EdpCliResult<SourcePasswordKnowledge> {
    if password.is_empty() {
        return Err(err(EXIT_TARGET, "错误: 来源密码不能为空"));
    }
    if source_native_sector_bytes(runner, disk)? == Some(4096) {
        return native_source_password_on_disk(runner, disk, domain, password);
    }
    let target_session = TargetSession::<ReadOnly>::open_usb(runner, disk)?;
    let total_sectors = target_session
        .total_sectors()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘总扇区数"))?;
    let probe = target_session
        .hardware_probe()
        .ok_or_else(|| err(EXIT_TARGET, "错误: 无法取得目标盘 USB/SCSI 硬件身份"))?;
    let target = TargetIdentity::from_probe(&probe, total_sectors)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 目标硬件身份不完整: {message}")))?;
    let mut dev = open_readonly_usb_disk(runner, disk)?;
    let source_metadata = read_image(&mut dev)?;
    let source_identity =
        classify_live_source_identity(runner, disk, &source_metadata, total_sectors, &mut dev)?;
    if source_identity.protocol.provision_kind == Some(crate::provision::DiskProvisionKind::Plain) {
        return Err(err(
            EXIT_TARGET,
            "错误: 当前来源盘已确认是 Plain；不存在可验证的 EDP key domain",
        ));
    }
    let source_device_id = source_protocol_device_id(&target, &source_identity);
    let image = ProvisionImage::from_bytes(source_metadata)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 来源元数据长度无效: {message}")))?;
    let source = parse_existing_provision(&image, &source_device_id, total_sectors)
        .map_err(|message| {
            err(
                EXIT_TARGET,
                format!("错误: 来源盘注册结构无法解析: {message}"),
            )
        })?
        .ok_or_else(|| err(EXIT_TARGET, "错误: 当前来源盘没有可验证的 EDP key domain"))?;
    let record = source
        .record_for_domain(domain)
        .ok_or_else(|| err(EXIT_TARGET, "错误: 当前来源模式不包含该密码域"))?;
    record
        .verified_sm4_file_key(password)
        .map_err(|message| err(EXIT_TARGET, format!("错误: 来源密码验证失败: {message}")))?;
    Ok(if password == DEFAULT_KEY_DOMAIN_PASSWORD {
        SourcePasswordKnowledge::DefaultVerified
    } else {
        SourcePasswordKnowledge::UserVerified
    })
}

#[cfg(test)]
mod native_sector_probe_policy_tests {
    use super::*;

    #[test]
    fn native_wrap_mode_auto_probe_accepts_real_u391_aes3_and_rejects_wrong_password() {
        use crate::protocol::edpf::{EdpfEntry64, EdpfEntry96};
        use crate::provision::{wrap_file_key, ExistingPartitionRecord};
        for mode in [
            FileKeyWrapMode::A7f0,
            FileKeyWrapMode::Sm4,
            FileKeyWrapMode::Aes128Ecb, // Real U391 AES-128-ECB profile
        ] {
            let material = wrap_file_key(DEFAULT_KEY_DOMAIN_PASSWORD, [0x49; 16], mode);
            let record = ExistingPartitionRecord {
                lba7: EdpfEntry64 {
                    version: 0x0206,
                    partition_count: 2,
                    partition_type: 2,
                    need_disturb: 0,
                    need_encrypt: 1,
                    start_sector: 2560,
                    sector_size: 4096,
                    partition_size: 4096 * 1024,
                    user_key_crc: 0,
                    file_key_crc: 0,
                    encrypted_file_key: [0; 8],
                },
                lba12: EdpfEntry96 {
                    version: 0x0206,
                    partition_count: 2,
                    partition_type: 2,
                    need_disturb: 0,
                    need_encrypt: 1,
                    start_sector: 2560,
                    sector_size: 4096,
                    partition_size: 4096 * 1024,
                    user_key_crc: material.user_key_crc,
                    file_key_crc: material.file_key_crc,
                    encrypted_file_key: material.wrapped_file_key,
                    compatibility_key: [0; 16],
                    encrypt_mode: mode.raw(),
                    reserved: [0; 7],
                },
            };
            assert_eq!(
                verified_native_source_password(record, DEFAULT_KEY_DOMAIN_PASSWORD),
                Ok(SourcePasswordKnowledge::DefaultVerified),
            );
            assert!(verified_native_source_password(record, b"wrong").is_err());
            let user_wrapped = wrap_file_key(b"NamedUserPass", [0x49; 16], mode);
            let named = ExistingPartitionRecord {
                lba12: EdpfEntry96 {
                    user_key_crc: user_wrapped.user_key_crc,
                    file_key_crc: user_wrapped.file_key_crc,
                    encrypted_file_key: user_wrapped.wrapped_file_key,
                    ..record.lba12
                },
                ..record
            };
            assert_eq!(
                verified_native_source_password(named, b"NamedUserPass"),
                Ok(SourcePasswordKnowledge::UserVerified),
            );
            let damaged = ExistingPartitionRecord {
                lba12: EdpfEntry96 {
                    file_key_crc: record.lba12.file_key_crc ^ 1,
                    ..record.lba12
                },
                ..record
            };
            assert!(verified_native_source_password(damaged, DEFAULT_KEY_DOMAIN_PASSWORD).is_err());
            for malformed in [
                ExistingPartitionRecord {
                    lba12: EdpfEntry96 {
                        need_encrypt: 0,
                        ..record.lba12
                    },
                    ..record
                },
                ExistingPartitionRecord {
                    lba12: EdpfEntry96 {
                        encrypt_mode: 99,
                        ..record.lba12
                    },
                    ..record
                },
            ] {
                assert!(
                    verified_native_source_password(malformed, DEFAULT_KEY_DOMAIN_PASSWORD)
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn observed_sector_size_routes_4kn_only_and_fails_closed_for_other_sizes() {
        assert_eq!(
            validate_source_password_sector_bytes(Some(512)).unwrap(),
            Some(512)
        );
        assert_eq!(
            validate_source_password_sector_bytes(Some(4096)).unwrap(),
            Some(4096)
        );
        assert_eq!(validate_source_password_sector_bytes(None).unwrap(), None);
        for unsupported in [0, 256, 1024, 2048, 8192, 65536] {
            assert!(validate_source_password_sector_bytes(Some(unsupported)).is_err());
        }
    }
}
