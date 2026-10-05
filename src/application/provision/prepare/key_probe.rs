//! Read-only key-domain probing and source-password verification.

use super::*;

pub fn probe_provision_key_domains_on_disk(
    runner: &dyn CmdRunner,
    disk: u32,
) -> EdpCliResult<ProvisionKeyProbe> {
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
