use super::*;

pub(super) fn run_advanced_source<R: SectorReader + ?Sized>(
    source: String,
    meta: InspectMeta,
    context: crate::inspect_target::InspectDiskContext,
    request: &AdvancedInspectRequest,
    reader: &mut R,
) -> Result<AdvancedInspectWorkspace, InspectError> {
    let lbas = if request.lbas.is_empty() {
        (0..METADATA_SECTOR_COUNT as u64).collect::<Vec<_>>()
    } else {
        request.lbas.clone()
    };
    if lbas.len() > MAX_ADVANCED_INSPECT_SECTORS {
        return Err(InspectError::invalid(format!(
            "单次 Inspect 最多读取 {MAX_ADVANCED_INSPECT_SECTORS} 个扇区"
        )));
    }

    let mut bundle = request
        .export_dir
        .as_deref()
        .map(super::export::ExportBundle::new)
        .transpose()?;
    let export_dir = bundle.as_ref().map(|b| b.dir.clone());
    let mut items = Vec::with_capacity(lbas.len());
    for lba in lbas {
        context
            .validate_lba(lba)
            .map_err(InspectError::out_of_range)?;
        let raw = reader
            .read_sector(lba)
            .map_err(|error| InspectError::io(format!("读取 LBA{lba} 失败: {error}")))?;
        if raw.len() != SECTOR {
            return Err(InspectError::io(format!(
                "LBA{lba} 返回 {}B，预期 {SECTOR}B",
                raw.len()
            )));
        }

        let mut partition_boot = None;
        let mut partition_boot_issue = None;
        if request.mode != AdvancedInspectMode::Raw {
            if let Some(partition_start) = context.partition_start_for_lba(lba) {
                if partition_start == lba {
                    partition_boot = Some(raw.clone());
                } else {
                    match reader.read_sector(partition_start) {
                        Ok(boot) if boot.len() == SECTOR => partition_boot = Some(boot),
                        Ok(boot) => {
                            partition_boot_issue = Some(format!(
                                "分区起始 LBA{} 只读取到 {}B",
                                partition_start,
                                boot.len()
                            ));
                        }
                        Err(error) => {
                            partition_boot_issue =
                                Some(format!("无法读取分区起始 LBA{}: {error}", partition_start));
                        }
                    }
                }
            }
        }

        let regions = context
            .regions(lba)
            .into_iter()
            .map(|region| region.label())
            .collect::<Vec<_>>();
        let raw_sha256 = crate::sha256::sha256_hex(&raw);
        let raw_nonzero = raw.iter().filter(|&&byte| byte != 0).count();
        let protocol_view = if context.has_edp_protocol()
            && lba <= u64::from(crate::common::METADATA_LAST_LBA)
        {
            let lba32 = u32::try_from(lba)
                .map_err(|_| InspectError::out_of_range(format!("LBA{lba} 超出协议解析器范围")))?;
            Some(inspect::analyze_sector_with_context(
                lba32,
                &raw,
                &meta,
                Some(&context.protocol_image),
            ))
        } else if context.is_plain() && lba == 0 {
            Some(inspect::analyze_mbr_sector(&raw))
        } else {
            None
        };

        let fields = match protocol_view.as_ref() {
            Some(view) => materialize_protocol_fields(lba, &raw, &view.decoded, &view.fields)?,
            None => Vec::new(),
        };
        let mut item = AdvancedInspectItem {
            lba,
            regions,
            raw: raw.clone(),
            raw_sha256,
            raw_nonzero,
            decoded: None,
            decode_ranges: protocol_view
                .as_ref()
                .map(|view| view.decode_ranges.clone())
                .unwrap_or_default(),
            decoded_sha256: None,
            method: None,
            decode_error: None,
            parse_state: protocol_view
                .as_ref()
                .map_or(InspectParseState::Unsupported, |view| view.parse_state),
            diagnostics: protocol_view
                .as_ref()
                .map(|view| view.diagnostics.clone())
                .unwrap_or_default(),
            fields,
            notes: protocol_view
                .as_ref()
                .map(|view| view.notes.clone())
                .unwrap_or_default(),
            meta_text: None,
        };

        match request.mode {
            AdvancedInspectMode::Raw => {
                if let Some(dir) = &export_dir {
                    export_advanced_bytes(dir, lba, "raw", &raw)?;
                }
            }
            AdvancedInspectMode::Decode => {
                let decoded = if let Some(view) = protocol_view {
                    if view.parse_state == InspectParseState::Parsed {
                        Ok((view.decoded, view.method, view.decode_ranges))
                    } else {
                        Err(InspectError::decode(
                            view.diagnostics
                                .first()
                                .map(|diagnostic| diagnostic.message.clone())
                                .unwrap_or_else(|| "canonical protocol decoder unavailable".into()),
                        ))
                    }
                } else {
                    decode_sector(&context, &meta, lba, &raw, partition_boot.as_deref())
                };
                match decoded {
                    Ok((decoded, method, decode_ranges)) => {
                        item.decoded_sha256 = Some(crate::sha256::sha256_hex(&decoded));
                        item.method = Some(method);
                        item.decode_ranges = decode_ranges;
                        if let Some(dir) = &export_dir {
                            export_advanced_bytes(dir, lba, "decoded", &decoded)?;
                        }
                        item.decoded = Some(decoded);
                    }
                    Err(error) if request.fail_soft_decode => {
                        item.decode_error = Some(error.to_string());
                        item.parse_state = InspectParseState::Invalid;
                        item.diagnostics.push(InspectDiagnostic::new(
                            InspectDiagnosticCode::CanonicalParserRejected,
                            error.to_string(),
                        ));
                        item.method = Some("raw-only".into());
                    }
                    Err(error) => return Err(error),
                }
            }
            AdvancedInspectMode::Meta => {
                let text = sector_meta_text(
                    &context,
                    &meta,
                    lba,
                    &raw,
                    partition_boot.as_deref(),
                    partition_boot_issue.as_deref(),
                )?;
                if let Some(dir) = &export_dir {
                    export_advanced_meta(dir, lba, &text)?;
                }
                item.meta_text = Some(text);
            }
        }
        items.push(item);
    }

    let (disk_layout, disk_layout_issue) =
        match super::super::disk_layout::DiskLayoutModel::canonical_inspect_context(&context) {
            Ok(layout) => (Some(layout), None),
            Err(error) => (None, Some(error)),
        };
    let topology = super::super::inspect_tree::build_inspect_topology(&context);
    if let Some(bundle) = &mut bundle {
        bundle.finish()?;
    }
    Ok(AdvancedInspectWorkspace {
        source,
        meta,
        mode: request.mode,
        items,
        export_dir,
        topology,
        disk_layout,
        disk_layout_issue,
        backup_manifest: None,
    })
}

pub(super) fn run_evidence_source(
    mut evidence: EvidenceSource,
    request: &AdvancedInspectRequest,
) -> Result<AdvancedInspectWorkspace, InspectError> {
    let backup_manifest = evidence.backup_manifest().cloned().map(std::sync::Arc::new);
    let identity = evidence.identity().clone();
    let mut meta = InspectMeta {
        device_id: identity.device_id,
        vid: identity.vid,
        pid: identity.pid,
        size_bytes: identity.size_bytes,
        onlyid: identity.onlyid,
    };
    if let Some(device_id) = request.device_id_override.as_ref() {
        meta.device_id = Some(device_id.clone());
    }
    let provision_kind = identity.provision_kind;
    let (partition_table, partition_table_issue) = if provision_kind
        == Some(crate::provision::DiskProvisionKind::Plain)
    {
        match crate::application::partition_table::read_partition_table(
            evidence.total_sectors(),
            |lba| evidence.read_sector(lba).map_err(|error| error.to_string()),
        ) {
            Ok(mut table) => {
                for partition in &mut table.partitions {
                    if let Ok(boot) = evidence.read_sector(partition.start_lba) {
                        partition.filesystem =
                            crate::filesystem::detect_boot_sector(partition.sector_count, &boot)
                                .ok()
                                .flatten()
                                .map(|filesystem| filesystem.label().to_string());
                    }
                }
                (Some(table), None)
            }
            Err(error) => (None, Some(error)),
        }
    } else {
        (None, None)
    };
    let context = crate::inspect_target::InspectDiskContext::new_with_partition_table(
        evidence.protocol().to_vec(),
        meta.device_id.clone(),
        evidence.total_sectors(),
        provision_kind,
        partition_table,
        partition_table_issue,
    );
    let source = evidence.source_label().to_string();
    let mut workspace = run_advanced_source(source, meta, context, request, &mut evidence)?;
    workspace.backup_manifest = backup_manifest;
    Ok(workspace)
}
