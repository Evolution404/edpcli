use super::*;

pub(super) fn render_lba0_4(
    lba: u32,
    raw_sector: &[u8; SECTOR],
    protocol_image: Option<&[u8]>,
    fields: &mut Vec<SectorField>,
    notes: &mut Vec<String>,
    decoded: &mut Vec<u8>,
    diagnostics: &mut Vec<InspectDiagnostic>,
) -> String {
    match lba {
        0 => match lba0::parse_lba0(raw_sector) {
            Ok(view) => {
                fields.push(profile_field(
                    0x000,
                    0x190,
                    "MBR 引导代码 profile",
                    view.bootstrap,
                ));
                fields.push(profile_field(
                    0x1a0,
                    0x1a4,
                    "SectorSize overlay",
                    view.sector_size,
                ));
                fields.push(field(
                    0x190,
                    0x1a0,
                    "兼容 backing",
                    hex_bytes(view.compat_190_19f.bytes()),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x1a4,
                    0x1b5,
                    "兼容 backing",
                    hex_bytes(view.compat_1a4_1b4.bytes()),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x1b5,
                    0x1b8,
                    "旧版消息指针",
                    hex_bytes(&view.message_pointers),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x1b8,
                    0x1bc,
                    "MBR 磁盘签名",
                    format!("0x{:08X}", view.disk_signature),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x1bc,
                    0x1be,
                    "MBR 保留字",
                    hex_bytes(view.mbr_reserved.bytes()),
                    FieldStyle::Flag,
                ));
                parse_mbr(raw_sector, fields, notes);
                notes.push("字段语义来自 protocol::lba0::parse_lba0；bootstrap、SectorSize overlay 与 MBR 字段按独立 profile 解释。".into());
                "canonical protocol::lba0".into()
            }
            Err(error) => {
                diagnostics.push(InspectDiagnostic::new(
                    InspectDiagnosticCode::CanonicalParserRejected,
                    format!("LBA0 canonical parser 拒绝: {error}"),
                ));
                notes.push(format!("canonical LBA0 parser 拒绝该扇区: {error}"));
                "RAW（LBA0 canonical parser 未通过）".into()
            }
        },
        1 => match lba1::parse_lba1(raw_sector) {
            Ok(view) => {
                match view.header {
                    lba1::GptHeaderState::Absent => {
                        fields.push(field(0, SECTOR, "GPT profile", "absent", FieldStyle::Flag));
                    }
                    lba1::GptHeaderState::Enabled(header) => {
                        fields.push(field(
                            0x00,
                            0x08,
                            "GPT signature",
                            "EFI PART",
                            FieldStyle::Magic,
                        ));
                        fields.push(field(
                            0x08,
                            0x0c,
                            "GPT revision",
                            format!("0x{:08X}", header.revision),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x0c,
                            0x10,
                            "GPT header size",
                            header.header_size.to_string(),
                            FieldStyle::Size,
                        ));
                        fields.push(field(
                            0x10,
                            0x14,
                            "GPT header CRC32",
                            format!("0x{:08X}", header.header_crc32),
                            FieldStyle::Checksum,
                        ));
                        fields.push(field(
                            0x18,
                            0x20,
                            "当前 LBA",
                            header.current_lba.to_string(),
                            FieldStyle::Address,
                        ));
                        fields.push(field(
                            0x20,
                            0x28,
                            "备份 GPT LBA",
                            header.backup_lba.to_string(),
                            FieldStyle::Address,
                        ));
                        fields.push(field(
                            0x28,
                            0x30,
                            "首个可用 LBA",
                            header.first_usable_lba.to_string(),
                            FieldStyle::Address,
                        ));
                        fields.push(field(
                            0x30,
                            0x38,
                            "末个可用 LBA",
                            header.last_usable_lba.to_string(),
                            FieldStyle::Address,
                        ));
                        fields.push(field(
                            0x38,
                            0x48,
                            "磁盘 GUID",
                            hex_bytes(&header.disk_guid),
                            FieldStyle::Identity,
                        ));
                        fields.push(field(
                            0x48,
                            0x50,
                            "分区表首 LBA",
                            header.partition_entries_lba.to_string(),
                            FieldStyle::Address,
                        ));
                        fields.push(field(
                            0x50,
                            0x54,
                            "GPT entry 数量",
                            header.entry_count.to_string(),
                            FieldStyle::Size,
                        ));
                        fields.push(field(
                            0x54,
                            0x58,
                            "GPT entry 大小",
                            header.entry_size.to_string(),
                            FieldStyle::Size,
                        ));
                        fields.push(field(
                            0x58,
                            0x5c,
                            "分区数组 CRC32",
                            format!("0x{:08X}", header.partition_array_crc32),
                            FieldStyle::Checksum,
                        ));
                    }
                }
                notes.push("字段语义与 CRC/几何校验来自 protocol::lba1::parse_lba1。".into());
                "canonical protocol::lba1".into()
            }
            Err(error) => {
                diagnostics.push(InspectDiagnostic::new(
                    InspectDiagnosticCode::CanonicalParserRejected,
                    format!("LBA1 canonical parser 拒绝: {error}"),
                ));
                notes.push(format!("canonical LBA1 parser 拒绝该扇区: {error}"));
                "RAW（LBA1 canonical parser 未通过）".into()
            }
        },
        2 => {
            let (profile, profile_source) = if let Some(raw1) = protocol_sector(protocol_image, 1) {
                match lba1::parse_lba1(raw1) {
                    Ok(view) => (
                        match view.header {
                            lba1::GptHeaderState::Absent => {
                                crate::protocol::profile::GptLayout::Absent
                            }
                            lba1::GptHeaderState::Enabled(_) => {
                                crate::protocol::profile::GptLayout::Enabled
                            }
                        },
                        "LBA1 canonical profile",
                    ),
                    Err(error) => {
                        diagnostics.push(InspectDiagnostic::new(
                            InspectDiagnosticCode::MissingProtocolContext,
                            format!("LBA1 上下文无法确定 GPT profile: {error}"),
                        ));
                        notes.push(format!(
                            "LBA1 canonical parser 无法确定 GPT profile: {error}"
                        ));
                        (crate::protocol::profile::GptLayout::Unknown, "LBA1 invalid")
                    }
                }
            } else {
                diagnostics.push(InspectDiagnostic::new(
                    InspectDiagnosticCode::MissingProtocolContext,
                    "LBA2 缺 LBA1 上下文，使用单扇区兼容 profile",
                ));
                notes.push(
                    "未提供完整 LBA0–12 上下文；LBA2 单扇区 API 仅以全零/非零选择兼容 profile，CLI/TUI 会以 LBA1 为权威。"
                        .into(),
                );
                (
                    if raw_sector.iter().all(|byte| *byte == 0) {
                        crate::protocol::profile::GptLayout::Absent
                    } else {
                        crate::protocol::profile::GptLayout::Enabled
                    },
                    "single-sector fallback",
                )
            };
            match lba2::parse_lba2(raw_sector, profile) {
                Ok(view) => {
                    fields.push(profile_field(
                        0,
                        SECTOR,
                        "GPT partition profile",
                        view.profile,
                    ));
                    if let Some(entries) = view.entries {
                        for (index, entry) in entries.iter().enumerate() {
                            let base = index * 0x80;
                            match entry {
                                lba2::GptEntry::Unused { residual } => fields.push(grouped_field(
                                    base,
                                    base + 0x80,
                                    format!("GPT Entry[{index}]"),
                                    "未使用 entry/backing",
                                    hex_bytes(residual.bytes()),
                                    FieldStyle::Flag,
                                )),
                                lba2::GptEntry::Used(partition) => {
                                    let group = format!("GPT Entry[{index}]");
                                    fields.push(grouped_field(
                                        base,
                                        base + 0x10,
                                        group.clone(),
                                        "Type GUID",
                                        hex_bytes(&partition.type_guid),
                                        FieldStyle::Identity,
                                    ));
                                    fields.push(grouped_field(
                                        base + 0x10,
                                        base + 0x20,
                                        group.clone(),
                                        "Unique GUID",
                                        hex_bytes(&partition.unique_guid),
                                        FieldStyle::Identity,
                                    ));
                                    fields.push(grouped_field(
                                        base + 0x20,
                                        base + 0x28,
                                        group.clone(),
                                        "首 LBA",
                                        partition.first_lba.to_string(),
                                        FieldStyle::Address,
                                    ));
                                    fields.push(grouped_field(
                                        base + 0x28,
                                        base + 0x30,
                                        group.clone(),
                                        "末 LBA",
                                        partition.last_lba.to_string(),
                                        FieldStyle::Address,
                                    ));
                                    fields.push(grouped_field(
                                        base + 0x30,
                                        base + 0x38,
                                        group.clone(),
                                        "属性",
                                        format!("0x{:016X}", partition.attributes),
                                        FieldStyle::Flag,
                                    ));
                                    let name = String::from_utf16_lossy(&partition.name_utf16)
                                        .trim_end_matches('\0')
                                        .to_string();
                                    fields.push(grouped_field(
                                        base + 0x38,
                                        base + 0x80,
                                        group,
                                        "名称",
                                        if name.is_empty() {
                                            "<空>".into()
                                        } else {
                                            name
                                        },
                                        FieldStyle::Text,
                                    ));
                                }
                            }
                        }
                    }
                    notes.push(format!(
                        "字段语义来自 protocol::lba2::parse_lba2；profile 来源={profile_source}。"
                    ));
                    "canonical protocol::lba2".into()
                }
                Err(error) => {
                    diagnostics.push(InspectDiagnostic::new(
                        InspectDiagnosticCode::CanonicalParserRejected,
                        format!("LBA2 canonical parser 拒绝: {error}"),
                    ));
                    notes.push(format!("canonical LBA2 parser 拒绝该扇区: {error}"));
                    "RAW（LBA2 canonical parser 未通过）".into()
                }
            }
        }
        3 => {
            let view = lba3::parse_lba3(raw_sector);
            fields.push(profile_field(
                0,
                SECTOR,
                "制造商 metadata profile",
                view.profile,
            ));
            fields.push(field(
                0,
                SECTOR,
                "制造商私有 opaque payload",
                format!(
                    "SHA-256={}",
                    crate::sha256::sha256_hex(view.payload.bytes())
                ),
                FieldStyle::Identity,
            ));
            notes.push("LBA3 在 EDP 协议边界只做 profile 识别与逐字节 preserve；不虚构 Phison 私有子字段。".into());
            "canonical protocol::lba3".into()
        }
        4 => match lba4::parse_lba4(raw_sector, lba4::Lba4Context::default()) {
            Ok(view) => {
                *decoded = view.reader.bytes().to_vec();
                fields.push(field(
                    0x000,
                    0x018,
                    "labelOnlyId",
                    format!("{} (0x{:08X})", view.onlyid_text, view.onlyid),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x018,
                    0x01c,
                    "OnlyIdXor8",
                    format!("0x{:08X}", view.node.onlyid_guard),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x01c,
                    0x020,
                    "OnllyID2Nd",
                    format!("0x{:08X}", view.node.second_key),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x020,
                    0x034,
                    "HSerialCRC[5]",
                    view.node
                        .hserial
                        .iter()
                        .map(|v| format!("{v:08X}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x034,
                    0x035,
                    "SingleUsbFlg",
                    view.node.single_usb.to_string(),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x035,
                    0x039,
                    "MyHardinfo",
                    format!("0x{:08X}", view.node.host_hardinfo),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x039,
                    0x03d,
                    "NewLabFlag",
                    hex_bytes(&view.node.new_lab_flag),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x03d,
                    0x041,
                    "Version",
                    format!("0x{:08X}", view.node.version),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x041,
                    0x045,
                    "sector tuple",
                    hex_bytes(&view.node.sector_tuple),
                    FieldStyle::Flag,
                ));
                let reader_flags = view.reader_flags();
                let wire_flags = view.wire_flags();
                fields.push(field(0x045, 0x046, "bDataToServer", format!("reader=0x{:02X}; wire=0x{:02X}; producer=需按 writer 判定 (profile unknown)", reader_flags.data_to_server, wire_flags.data_to_server), FieldStyle::Flag));
                fields.push(field(0x046, 0x047, "bConnetServer", format!("reader=0x{:02X}; wire=0x{:02X}; producer=需按 writer 判定 (profile unknown)", reader_flags.connect_server, wire_flags.connect_server), FieldStyle::Flag));
                fields.push(field(
                    0x047,
                    0x1fc,
                    "representation backing",
                    if view.backing_is_raw_zero() {
                        "raw-zero profile"
                    } else {
                        "rolling representation carrier"
                    },
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x1fc,
                    0x200,
                    "trailing LLGB",
                    hex_bytes(&view.trailing_magic),
                    FieldStyle::Magic,
                ));
                notes.push("字段结构来自 protocol::lba4::parse_lba4。Inspect 不凭身份形态猜 writer：encoding/second-key/HSerial/host-hardinfo profile 保持 Unknown，因此 producer flag 不伪判。".into());
                format!(
                    "canonical protocol::lba4 labelOnlyId={}（writer provenance 未强猜）",
                    view.onlyid_text
                )
            }
            Err(error) => {
                diagnostics.push(InspectDiagnostic::new(
                    InspectDiagnosticCode::CanonicalParserRejected,
                    format!("LBA4 canonical parser 拒绝: {error}"),
                ));
                notes.push(format!("canonical LBA4 parser 拒绝该扇区: {error}"));
                "RAW（LBA4 canonical parser 未通过）".into()
            }
        },
        _ => "RAW（无已知结构解析器）".into(),
    }
}
