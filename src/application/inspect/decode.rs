use super::*;

pub(super) fn materialize_protocol_fields(
    lba: u64,
    raw: &[u8],
    decoded: &[u8],
    fields: &[crate::inspect_adapter::SectorField],
) -> Result<Vec<InspectField>, InspectError> {
    materialize_protocol_fields_with_sector_bytes(lba, raw, decoded, fields, SECTOR as u32)
}

pub(super) fn materialize_protocol_fields_with_sector_bytes(
    lba: u64,
    raw: &[u8],
    decoded: &[u8],
    fields: &[crate::inspect_adapter::SectorField],
    native_sector_bytes: u32,
) -> Result<Vec<InspectField>, InspectError> {
    let base = lba
        .checked_mul(u64::from(native_sector_bytes))
        .ok_or_else(|| InspectError::decode(format!("LBA{lba} 字段绝对字节偏移溢出")))?;
    fields
        .iter()
        .enumerate()
        .map(|(ordinal, field)| {
            if field.end < field.start || field.end > raw.len() || field.end > decoded.len() {
                return Err(InspectError::decode(format!(
                    "LBA{lba} 字段 {} range +0x{:X}..+0x{:X} 越界",
                    field.label, field.start, field.end
                )));
            }
            let start = base.checked_add(field.start as u64).ok_or_else(|| {
                InspectError::decode(format!("LBA{lba} 字段 {} 绝对起点溢出", field.label))
            })?;
            let end_exclusive = base.checked_add(field.end as u64).ok_or_else(|| {
                InspectError::decode(format!("LBA{lba} 字段 {} 绝对终点溢出", field.label))
            })?;
            Ok(InspectField {
                key: InspectFieldKey::protocol(
                    lba,
                    field.start,
                    field.end,
                    fields[..ordinal]
                        .iter()
                        .filter(|previous| {
                            previous.start == field.start && previous.end == field.end
                        })
                        .count(),
                ),
                range: AbsoluteByteRange {
                    start,
                    end_exclusive,
                },
                field_type: field.style.into(),
                raw: raw[field.start..field.end].to_vec(),
                decoded: decoded[field.start..field.end].to_vec(),
                field_logical: field
                    .transform
                    .and_then(|transform| transform.apply(&decoded[field.start..field.end])),
                transform: field.transform,
                status: field.status,
                label: field.label.clone(),
                value: field.value.clone(),
                style: field.style,
                group: field.group.clone(),
                children: field.children.clone(),
            })
        })
        .collect()
}

pub fn decode_sector(
    context: &crate::inspect_target::InspectDiskContext,
    meta: &InspectMeta,
    lba: u64,
    raw: &[u8],
    partition_boot_raw: Option<&[u8]>,
) -> Result<(Vec<u8>, String, Vec<crate::inspect_adapter::DecodeRange>), InspectError> {
    let decoder = DECODER_REGISTRY
        .iter()
        .copied()
        .find(|decoder| decoder.matches(context, lba))
        .ok_or_else(|| {
            InspectError::decode(format!(
                "LBA{lba} 不属于已注册 decoder 区域；raw 可读，decode 拒绝猜测"
            ))
        })?;
    match decoder {
        InspectDecoderKind::Protocol => {
            let lba32 = u32::try_from(lba)
                .map_err(|_| InspectError::out_of_range(format!("LBA{lba} 超出协议解析器范围")))?;
            let view = inspect::analyze_sector_with_context(
                lba32,
                raw,
                meta,
                Some(&context.protocol_image),
            );
            if view.parse_state != InspectParseState::Parsed {
                let reason = view
                    .diagnostics
                    .first()
                    .map(|diagnostic| diagnostic.message.clone())
                    .unwrap_or_else(|| "canonical protocol decoder unavailable".into());
                return Err(InspectError::decode(reason));
            }
            Ok((view.decoded, view.method, view.decode_ranges))
        }
        InspectDecoderKind::Lce => {
            let (decoded, method) = context
                .decode_non_protocol_with_boot(lba, raw, partition_boot_raw)
                .map_err(InspectError::decode)?;
            Ok((
                decoded,
                method,
                vec![crate::inspect_adapter::DecodeRange::new(0, SECTOR)],
            ))
        }
        InspectDecoderKind::Partition => {
            let (decoded, method) = context
                .decode_non_protocol_with_boot(lba, raw, partition_boot_raw)
                .map_err(InspectError::decode)?;
            let transformed = if context.is_plain() {
                false
            } else if let Some(partition) = context.partition_for_lba(lba) {
                let boot = if lba == partition.start_sector {
                    raw
                } else {
                    partition_boot_raw.ok_or_else(|| {
                        InspectError::decode(format!(
                            "分区[{}] 缺少起始扇区证据，无法确认 Decode provenance",
                            partition.index
                        ))
                    })?
                };
                matches!(
                    context.partition_physical_state(partition, boot),
                    crate::inspect_target::PhysicalDataState::EncryptedMode2 { .. }
                )
            } else {
                false
            };
            let ranges = if transformed {
                vec![crate::inspect_adapter::DecodeRange::new(0, SECTOR)]
            } else {
                Vec::new()
            };
            Ok((decoded, method, ranges))
        }
    }
}

pub fn sector_meta_text(
    context: &crate::inspect_target::InspectDiskContext,
    meta: &InspectMeta,
    lba: u64,
    raw: &[u8],
    partition_boot_raw: Option<&[u8]>,
    partition_boot_issue: Option<&str>,
) -> Result<String, InspectError> {
    use crate::inspect_target::PhysicalDataState;

    let offset = lba
        .checked_mul(SECTOR as u64)
        .ok_or_else(|| InspectError::out_of_range("LBA 字节偏移溢出"))?;
    let mut out = format!(
        "LBA: {lba}\n物理字节偏移: {offset} (0x{offset:X})\nRAW SHA-256: {}\nRAW 非零字节: {}/512\n",
        crate::sha256::sha256_hex(raw),
        raw.iter().filter(|&&byte| byte != 0).count()
    );
    let regions = context.regions(lba);
    out.push_str("区域:\n");
    for region in &regions {
        out.push_str(&format!("  - {}\n", region.label()));
    }
    if let Some(primary) = regions.first() {
        out.push_str(&format!("主区域: {}\n", primary.label()));
    }
    if regions.len() > 1 {
        out.push_str(&format!(
            "重叠区域: {}\n",
            regions[1..]
                .iter()
                .map(crate::inspect_target::SectorRegion::label)
                .collect::<Vec<_>>()
                .join("；")
        ));
    } else {
        out.push_str("重叠区域: 无\n");
    }

    if context.has_edp_protocol() && lba <= u64::from(crate::common::METADATA_LAST_LBA) {
        let lba32 = u32::try_from(lba)
            .map_err(|_| InspectError::out_of_range(format!("LBA{lba} 超出协议解析器范围")))?;
        let view =
            inspect::analyze_sector_with_context(lba32, raw, meta, Some(&context.protocol_image));
        out.push_str(&format!(
            "协议解码: {}
",
            view.method
        ));
        out.push_str(&super::super::inspect_text::render_fields_plain(&view));
        for note in &view.notes {
            out.push_str(&format!(
                "  └─ {note}
"
            ));
        }
    } else if context.is_plain() && lba == 0 {
        let view = inspect::analyze_mbr_sector(raw);
        out.push_str(&format!(
            "分区表解码: {}
",
            view.method
        ));
        out.push_str(&super::super::inspect_text::render_fields_plain(&view));
    }

    if let Some(partition) = context.plain_partition_for_lba(lba) {
        out.push_str(&format!(
            "普通分区: P{} {} relative_lba={} start={} sectors={}
",
            partition.index,
            partition.type_label(),
            lba - partition.start_lba,
            partition.start_lba,
            partition.sector_count,
        ));
        let filesystem = partition_boot_raw
            .and_then(|boot| {
                crate::filesystem::detect_boot_sector(partition.sector_count, boot)
                    .ok()
                    .flatten()
            })
            .map(|filesystem| filesystem.label().to_string())
            .or_else(|| partition.filesystem.clone());
        out.push_str(&format!(
            "文件系统识别: {}
",
            filesystem.unwrap_or_else(|| "未确认".into())
        ));
        out.push_str(
            "加密配置: 不适用（普通盘）
",
        );
    } else if let Some(partition) = context.partition_for_lba(lba) {
        out.push_str(&format!(
            "分区: index={} type={} relative_lba={} start={} sectors={}\n",
            partition.index,
            partition.partition_type,
            lba - partition.start_sector,
            partition.start_sector,
            partition.sector_count,
        ));
        out.push_str(&format!(
            "协议密钥字段: NeedEncrypt={} EncryptMode={}\n",
            partition.need_encrypt, partition.encrypt_mode
        ));
        match context.partition_semantics(partition) {
            Ok(semantics) => out.push_str(&format!(
                "物理加密语义: mode{} slot{} {} · {}\n",
                semantics.mode as u8,
                semantics.index,
                semantics.role.label(),
                semantics.physical_encryption.label(),
            )),
            Err(error) => out.push_str(&format!("物理加密语义: 未确认（{error}）\n")),
        }
        out.push_str(&format!(
            "MBR 直接暴露: {}\n",
            context.partition_mbr_exposure(partition)
        ));
        out.push_str(&format!(
            "密钥: FileKeyCRC=0x{:08X} 状态={}\n",
            partition.file_key_crc,
            context.partition_file_key_crc_status(partition)
        ));
        let state = match partition_boot_raw {
            Some(boot) => context.partition_physical_state(partition, boot),
            None => PhysicalDataState::Unknown {
                reason: partition_boot_issue
                    .unwrap_or("缺少分区起始扇区证据")
                    .to_string(),
            },
        };
        out.push_str(&format!("物理数据状态: {}\n", state.label()));
        out.push_str(&format!("decode 策略: {}\n", state.decode_strategy()));
        out.push_str(&format!(
            "文件系统识别: {}\n",
            state
                .filesystem()
                .map(|filesystem| filesystem.label())
                .unwrap_or("未确认")
        ));
    }

    if let Some(lce) = &context.lce {
        if lba >= lce.start_lba && lba < lce.start_lba + lce.sector_count {
            out.push_str(&format!(
                "LCE: start={} sectors={} pointers={:?} mode={:?} chs_crosscheck={:?}\n",
                lce.start_lba,
                lce.sector_count,
                lce.lba7_pointer_entries
                    .iter()
                    .map(|pointer| (pointer.entry_index, pointer.partition_type))
                    .collect::<Vec<_>>(),
                lce.official_partition_mode,
                lce.chs_expected_start_lba
            ));
            out.push_str("LCE 解码: EDPSECDISK200709/A6B0，zero8，64 位物理字节偏移 tweak\n");
        }
    }
    for issue in &context.context_issues {
        out.push_str(&format!("上下文提示: {issue}\n"));
    }
    Ok(out)
}
