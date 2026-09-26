use super::*;

pub(super) fn render_lba9_12(
    lba: u32,
    raw_sector: &[u8; SECTOR],
    meta: &InspectMeta,
    protocol_image: Option<&[u8]>,
    fields: &mut Vec<SectorField>,
    notes: &mut Vec<String>,
    decoded: &mut Vec<u8>,
) -> String {
    match lba {
        9 => {
            if let Some((crc, _)) = crc_key(meta) {
                let companion =
                    protocol_sector(protocol_image, 6).and_then(|raw6| lba6::parse_lba6(raw6).ok());
                let (dept_profile, long_user) = companion
                    .as_ref()
                    .map(|view| (view.dept_profile(), view.has_long_user()))
                    .unwrap_or((DeptLayout::Short, false));
                if companion.is_none() {
                    notes.push("未提供完整 LBA0–12 上下文；LBA9 使用 short/non-long-user 兼容解析，仅用于单扇区 API。CLI/TUI 会提供 LBA6 上下文。".into());
                }
                match lba9::parse_lba9(raw_sector, crc, dept_profile, long_user) {
                    Ok(view) => {
                        *decoded = view.decoded().to_vec();
                        fields.push(field(
                            0x080,
                            0x100,
                            "Dept continuation",
                            slot_value(&view.dept_continuation),
                            FieldStyle::Text,
                        ));
                        match &view.eetu {
                            lba9::EetuState::Absent => fields.push(field(
                                0x000,
                                0x080,
                                "EETU",
                                "absent-zero",
                                FieldStyle::Flag,
                            )),
                            lba9::EetuState::Present(eetu) => {
                                fields.push(field(
                                    0x000,
                                    0x004,
                                    "EETU magic",
                                    "EETU",
                                    FieldStyle::Magic,
                                ));
                                fields.push(field(
                                    0x004,
                                    0x00c,
                                    "EETU 开始时间 (ullBTime)",
                                    if eetu.begin_time == 0 {
                                        "0（不限制）".into()
                                    } else {
                                        eetu.begin_time.to_string()
                                    },
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x00c,
                                    0x014,
                                    "EETU 结束时间 (ullETime)",
                                    if eetu.end_time == 0 {
                                        "0（不限制）".into()
                                    } else {
                                        eetu.end_time.to_string()
                                    },
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x014,
                                    0x018,
                                    "EETU 使用次数 (useCount)",
                                    if eetu.use_count == u32::MAX {
                                        "无限（0xFFFFFFFF）".into()
                                    } else {
                                        eetu.use_count.to_string()
                                    },
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x018,
                                    0x07e,
                                    "EETU reverse backing",
                                    hex_bytes(eetu.reverse.bytes()),
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x07e,
                                    0x080,
                                    "EETU zero tail",
                                    hex_bytes(&eetu.zero_tail),
                                    FieldStyle::Flag,
                                ));
                            }
                            lba9::EetuState::Unknown(bytes) => fields.push(field(
                                0x000,
                                0x080,
                                "EETU",
                                format!("unknown backing {}", hex_bytes(bytes.bytes())),
                                FieldStyle::Flag,
                            )),
                        }
                        match &view.upper {
                            lba9::UpperPayload::Zero => fields.push(field(
                                0x100,
                                0x200,
                                "upper payload",
                                "zero",
                                FieldStyle::Flag,
                            )),
                            lba9::UpperPayload::Sapf(sapf) => {
                                fields.push(field(
                                    0x100,
                                    0x104,
                                    "SAPF magic",
                                    "SAPF",
                                    FieldStyle::Magic,
                                ));
                                fields.push(field(
                                    0x108,
                                    0x109,
                                    "partition type",
                                    format!("0x{:02X}", sapf.partition.partition_type),
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x10c,
                                    0x110,
                                    "起始 LBA",
                                    sapf.partition.start_lba.to_string(),
                                    FieldStyle::Address,
                                ));
                                fields.push(field(
                                    0x110,
                                    0x114,
                                    "SAPF 扇区数",
                                    sapf.partition.sector_count.to_string(),
                                    FieldStyle::Size,
                                ));
                                fields.push(field(
                                    0x114,
                                    0x120,
                                    "SAPF compatibility",
                                    hex_bytes(&sapf.compatibility),
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x120,
                                    0x200,
                                    "SAPF tail backing",
                                    hex_bytes(sapf.tail.bytes()),
                                    FieldStyle::Flag,
                                ));
                            }
                            lba9::UpperPayload::LongUser { continuation, tail } => {
                                fields.push(field(
                                    0x100,
                                    0x180,
                                    "User continuation",
                                    slot_value(continuation),
                                    FieldStyle::Text,
                                ));
                                fields.push(field(
                                    0x180,
                                    0x200,
                                    "User tail backing",
                                    hex_bytes(tail.bytes()),
                                    FieldStyle::Flag,
                                ));
                            }
                            lba9::UpperPayload::Eppe(eppe) => {
                                fields.push(field(
                                    0x180,
                                    0x184,
                                    "EPPE magic",
                                    "EPPE",
                                    FieldStyle::Magic,
                                ));
                                fields.push(field(
                                    0x184,
                                    0x188,
                                    "最小密码长度",
                                    eppe.min_length.to_string(),
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x100,
                                    0x180,
                                    "EPPE prefix backing",
                                    hex_bytes(eppe.prefix.bytes()),
                                    FieldStyle::Flag,
                                ));
                                fields.push(field(
                                    0x188,
                                    0x200,
                                    "EPPE zero tail",
                                    hex_bytes(&eppe.zero_tail),
                                    FieldStyle::Flag,
                                ));
                            }
                            lba9::UpperPayload::Unknown(bytes) => fields.push(field(
                                0x100,
                                0x200,
                                "upper payload",
                                format!("unknown backing {}", hex_bytes(bytes.bytes())),
                                FieldStyle::Flag,
                            )),
                        }
                        notes.push(format!("字段语义来自 protocol::lba9::parse_lba9；Dept profile={}；EETU 时间字段由运行时与 time(NULL) 比较，useCount=0xFFFFFFFF 表示无限；旧称 reverse[104] 已拆分为 reverse backing[102] + zero tail[2]，语义状态 COMPLETE。", dept_profile.as_str()));
                        "canonical protocol::lba9".into()
                    }
                    Err(error) => {
                        notes.push(format!("canonical LBA9 parser 拒绝该扇区: {error}"));
                        "RAW（LBA9 canonical parser 未通过）".into()
                    }
                }
            } else {
                "RAW（缺 device_id，无法解 LBA9）".into()
            }
        }
        10 => {
            if let Some((crc, _)) = crc_key(meta) {
                let profile = if raw_sector.iter().all(|byte| *byte == 0) {
                    Lba10Eesi::AbsentZero
                } else {
                    Lba10Eesi::EesiEnabled
                };
                match lba10::parse_lba10(raw_sector, crc, profile) {
                    Ok(lba10::Lba10View::Absent { .. }) => {
                        fields.push(field(
                            0,
                            0x80,
                            "EESI profile",
                            "absent-zero",
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x80,
                            0x200,
                            "preserve/ignore tail",
                            "当前全零；协议不赋予 payload 语义",
                            FieldStyle::Flag,
                        ));
                        "canonical protocol::lba10 absent-zero".into()
                    }
                    Ok(lba10::Lba10View::Enabled {
                        plain_prefix,
                        suspension_flag,
                        share_label,
                        encrypt_label,
                        extension,
                        tail,
                        ..
                    }) => {
                        decoded[..0x80].copy_from_slice(&plain_prefix);
                        fields.push(field(0x000, 0x004, "EESI magic", "EESI", FieldStyle::Magic));
                        fields.push(field(
                            0x004,
                            0x008,
                            "UsbSuspensionWnd flag",
                            format!("{} (0x{:08X})", suspension_flag, suspension_flag),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x008,
                            0x018,
                            "EESI 交换区卷标",
                            slot_value(&share_label),
                            FieldStyle::Text,
                        ));
                        fields.push(field(
                            0x018,
                            0x028,
                            "EESI 保密区卷标",
                            slot_value(&encrypt_label),
                            FieldStyle::Text,
                        ));
                        fields.push(field(
                            0x028,
                            0x080,
                            "caller extension",
                            hex_bytes(extension.bytes()),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x080,
                            0x200,
                            "preserve/ignore tail",
                            hex_bytes(tail.bytes()),
                            FieldStyle::Flag,
                        ));
                        notes.push("EESI +0x04 最新语义为 UsbSuspensionWnd flag；仅前 0x80B 属于 EESI，后 0x180B 为 preserve/ignore backing；交换区/保密区卷标分别进入 type2/type4 SetVolumeLabelA。".into());
                        "canonical protocol::lba10 EESI 前 0x80B".into()
                    }
                    Err(error) => {
                        notes.push(format!("canonical LBA10 parser 拒绝该扇区: {error}"));
                        "RAW（LBA10 canonical parser 未通过）".into()
                    }
                }
            } else if raw_sector.iter().all(|byte| *byte == 0) {
                fields.push(field(0, SECTOR, "LBA10", "absent-zero", FieldStyle::Flag));
                "canonical LBA10 absent-zero（无需 device_id）".into()
            } else {
                "RAW（缺 device_id，无法解 LBA10）".into()
            }
        }
        11 => match infer_lba11(raw_sector, meta) {
            Some((view, profiles)) => {
                decoded[..0x100].copy_from_slice(&view.reconstruct()[..0x100]);
                decoded[0x100..].copy_from_slice(view.decoded_pdkb());
                fields.push(field(0x000, 0x004, "DRKB magic", "DRKB", FieldStyle::Magic));
                fields.push(field(
                    0x004,
                    0x100,
                    "random252",
                    hex_bytes(&view.random252),
                    FieldStyle::Identity,
                ));
                fields.push(field(0x100, 0x104, "PDKB magic", "PDKB", FieldStyle::Magic));
                fields.push(field(
                    0x104,
                    0x200,
                    "PDKB device_id",
                    slot_value(&view.uid),
                    FieldStyle::Identity,
                ));
                notes.push(format!("capacity profile 候选={}；选中容量={}B。若 DiskSize 与 repair-CHS 数值相同，两者可同时通过但盘面语义等价。", profiles.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(","), view.capacity_bytes));
                "canonical protocol::lba11 (DRKB + PDKB)".into()
            }
            None => "RAW（缺 VID/PID/容量或 LBA11 canonical parser 未通过）".into(),
        },
        12 => {
            if let Some((crc, _)) = crc_key(meta) {
                match infer_lba12(raw_sector, crc) {
                    Some((view, modes)) => {
                        *decoded = view.decoded().to_vec();
                        for (index, entry) in view.entries.iter().enumerate() {
                            fields.extend(edpf96_fields(index * 0x60, index, entry));
                        }
                        fields.extend(pass_info_fields(0x120, &view.pass_info, "PassInfo"));
                        fields.push(field(
                            0x12e,
                            0x200,
                            "zero padding",
                            "210B writer-zero（位于整扇外层密文内）",
                            FieldStyle::Flag,
                        ));
                        notes.push(format!(
                            "wrapped-key mode 候选={}；外层始终是 device CRC 派生的整扇 A6B0。",
                            modes
                                .iter()
                                .map(|mode| mode.as_str())
                                .collect::<Vec<_>>()
                                .join(",")
                        ));
                        "canonical protocol::lba12 (A6B0 整扇 512B)".into()
                    }
                    None => {
                        notes.push("无法在 legacy-v0064/mode1/mode2/mode3 中解析 LBA12；拒绝猜测 wrapped-key profile。".into());
                        "RAW（LBA12 canonical parser 未通过）".into()
                    }
                }
            } else {
                "RAW（缺 device_id，无法解 LBA12）".into()
            }
        }
        _ => "RAW（无已知结构解析器）".into(),
    }
}
