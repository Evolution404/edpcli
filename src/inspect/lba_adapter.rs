use super::*;

pub fn analyze_sector(lba: u32, raw: &[u8], meta: &InspectMeta) -> SectorView {
    analyze_sector_with_context(lba, raw, meta, None)
}

pub fn analyze_sector_with_context(
    lba: u32,
    raw: &[u8],
    meta: &InspectMeta,
    protocol_image: Option<&[u8]>,
) -> SectorView {
    if raw.len() != SECTOR {
        return SectorView {
            lba,
            raw: raw.to_vec(),
            decoded: raw.to_vec(),
            method: format!("RAW（短读：{}B，应为 {}B）", raw.len(), SECTOR),
            fields: vec![],
            notes: vec!["扇区长度异常，停止结构化解析。".into()],
        };
    }
    let raw_sector = raw_sector(raw).expect("length checked");
    let mut fields = Vec::new();
    let mut notes = Vec::new();
    let mut decoded = raw.to_vec();

    let method = match lba {
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
                parse_mbr(raw_sector, &mut fields, &mut notes);
                notes.push("字段语义来自 protocol::lba0::parse_lba0；bootstrap、SectorSize overlay 与 MBR 字段按独立 profile 解释。".into());
                "canonical protocol::lba0".into()
            }
            Err(error) => {
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
                        notes.push(format!(
                            "LBA1 canonical parser 无法确定 GPT profile: {error}"
                        ));
                        (crate::protocol::profile::GptLayout::Unknown, "LBA1 invalid")
                    }
                }
            } else {
                notes.push(
                    "未提供完整 LBA0–12 上下文；LBA2 单扇区 API 仅以全零/非零选择兼容 profile，CLI/TUI 会以 LBA1 为权威。"
                        .into(),
                );
                (
                    if raw.iter().all(|byte| *byte == 0) {
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
                decoded = view.reader.bytes().to_vec();
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
                notes.push(format!("canonical LBA4 parser 拒绝该扇区: {error}"));
                "RAW（LBA4 canonical parser 未通过）".into()
            }
        },
        5 => {
            let view = lba5::parse_lba5(raw_sector);
            fields.push(field(
                0,
                SECTOR,
                "写保护探测 scratch 区",
                format!(
                    "内容本身不解析；opaque preserve；SHA-256={}",
                    crate::sha256::sha256_hex(view.payload.bytes())
                ),
                FieldStyle::Flag,
            ));
            notes.push("LBA5 内容不解析；EDP 只消费写回结果判断 ERROR_WRITE_PROTECT，扇区本身原样保留；当前样本可以全零，但全零不是协议要求。".into());
            "canonical protocol::lba5（写保护探测）".into()
        }
        6 => match lba6::parse_lba6(raw_sector) {
            Ok(view) => {
                decoded = view.decoded().to_vec();
                let dept = match &view.dept {
                    lba6::DeptInline::Short(slot) => format!("short: {}", slot_value(slot)),
                    lba6::DeptInline::Join59(bytes) => {
                        format!("join59 inline: {}", text_value(bytes))
                    }
                    lba6::DeptInline::Join60(bytes) => {
                        format!("join60 inline: {}", text_value(bytes))
                    }
                };
                fields.push(field(0x000, 0x040, "Dept inline", dept, FieldStyle::Text));
                fields.push(field(
                    0x040,
                    0x050,
                    "SAFE6 模板",
                    hex_bytes(&view.template_040_04f),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x050,
                    0x070,
                    "User",
                    slot_value(&view.user),
                    FieldStyle::Text,
                ));
                fields.push(field(
                    0x070,
                    0x080,
                    "Autonum",
                    slot_value(&view.autonum),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x080,
                    0x0c0,
                    "Office",
                    slot_value(&view.office),
                    FieldStyle::Text,
                ));
                fields.push(field(
                    0x0c0,
                    0x100,
                    "模板区",
                    hex_bytes(&view.template_0c0_0ff),
                    FieldStyle::Flag,
                ));
                let device_crc_value = match crc_key(meta) {
                    Some((expected, _)) if view.device_crc == expected => {
                        format!(
                            "0x{:08X} / device_id 计算 0x{expected:08X} ✓",
                            view.device_crc
                        )
                    }
                    Some((expected, _)) => {
                        format!(
                            "0x{:08X} / device_id 计算 0x{expected:08X} ✗",
                            view.device_crc
                        )
                    }
                    None => format!("0x{:08X}（缺少 device_id，未校验）", view.device_crc),
                };
                fields.push(field(
                    0x100,
                    0x104,
                    "device_id CRC32",
                    device_crc_value,
                    FieldStyle::Checksum,
                ));
                fields.push(field(
                    0x104,
                    0x108,
                    "CRC guard",
                    format!("0x{:08X}", view.device_crc_guard),
                    FieldStyle::Checksum,
                ));
                fields.push(field(
                    0x108,
                    0x188,
                    "兼容模板区",
                    hex_bytes(&view.template_108_187),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x188,
                    0x1c0,
                    "Label",
                    slot_value(&view.label),
                    FieldStyle::Text,
                ));
                fields.push(field(
                    0x1c0,
                    0x1d0,
                    "m_usbGSerial 槽",
                    slot_value(&view.gserial),
                    FieldStyle::Identity,
                ));
                fields.push(field(
                    0x1d0,
                    0x1e0,
                    "BeiZhu 槽",
                    slot_value(&view.beizhu),
                    FieldStyle::Text,
                ));
                match &view.mbr_underlay {
                    lba6::MbrUnderlay::ZeroBacking => fields.push(field(
                        0x1e0,
                        0x1ee,
                        "legacy MBR snapshot",
                        "zero-underlay",
                        FieldStyle::Flag,
                    )),
                    lba6::MbrUnderlay::LegacySnapshot(snapshot) => fields.push(field(
                        0x1e0,
                        0x1ee,
                        "legacy MBR snapshot",
                        format!(
                            "start_lba={} sectors={} prefix={}",
                            snapshot.start_lba,
                            snapshot.sector_count,
                            hex_bytes(&snapshot.prefix)
                        ),
                        FieldStyle::Address,
                    )),
                    lba6::MbrUnderlay::Unknown(bytes) => fields.push(field(
                        0x1e0,
                        0x1ee,
                        "legacy MBR snapshot",
                        format!("unknown opaque {}", hex_bytes(bytes.bytes())),
                        FieldStyle::Flag,
                    )),
                }
                fields.push(field(
                    0x1ee,
                    0x1f0,
                    "兼容 tail",
                    hex_bytes(&view.compatibility_tail),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x1f0,
                    0x1f4,
                    "m_encrypt",
                    format!(
                        "{} (0x{:08X})",
                        view.encrypt_generation_flag, view.encrypt_generation_flag
                    ),
                    FieldStyle::Flag,
                ));
                fields.push(field(
                    0x1f4,
                    0x1fc,
                    "zero tail",
                    hex_bytes(&view.zero_tail),
                    FieldStyle::Flag,
                ));
                let calculated_checksum = lba6_checksum(&raw[..0x1fc]);
                let checksum_value = if view.checksum == calculated_checksum {
                    format!(
                        "0x{:08X} / 计算 0x{:08X} ✓",
                        view.checksum, calculated_checksum
                    )
                } else if view.checksum == calculated_checksum.wrapping_mul(2) {
                    format!(
                        "0x{:08X} / 计算 0x{:08X} ×2 profile ✓",
                        view.checksum, calculated_checksum
                    )
                } else {
                    format!(
                        "0x{:08X} / 计算 0x{:08X} ✗",
                        view.checksum, calculated_checksum
                    )
                };
                fields.push(field(
                    0x1fc,
                    0x200,
                    "校验和",
                    checksum_value,
                    FieldStyle::Checksum,
                ));
                notes.push(format!(
                    "Dept profile={}；字段语义来自 protocol::lba6::parse_lba6。",
                    view.dept_profile().as_str()
                ));
                "canonical protocol::lba6 (SAFE6 rolling XOR)".into()
            }
            Err(error) => {
                notes.push(format!("canonical LBA6 parser 拒绝该扇区: {error}"));
                "RAW（LBA6 canonical parser 未通过）".into()
            }
        },
        7 => {
            if let Some((crc, _)) = crc_key(meta) {
                match infer_lba7(raw_sector, crc) {
                    Some((view, entry_profile, pass_profile)) => {
                        decoded = view.stored_plain().to_vec();
                        for (index, entry) in view.entries_0_1.iter().enumerate() {
                            fields.extend(edpf64_fields(index * 0x40, index, entry));
                        }
                        if let lba7::Entry2::Present(entry) = view.entry2 {
                            fields.extend(edpf64_fields(0x80, 2, &entry));
                        } else {
                            fields.push(field(
                                0x80,
                                0xc0,
                                "Entry[2]",
                                "absent/zero",
                                FieldStyle::Flag,
                            ));
                        }
                        fields.extend(pass_info_fields(0xc0, &view.pass_info, "PassInfo"));
                        fields.push(field(
                            0xce,
                            0x200,
                            "post-table zero",
                            "306B writer-zero",
                            FieldStyle::Flag,
                        ));
                        if let Some(mode) = view.official_partition_mode() {
                            notes.push(format!("官方模式={} ({})", mode as u8, mode.ui_name_zh()));
                        } else {
                            notes.push(
                                "分区类型序列不属于当前已证实的四个官方模式，保持未分类。".into(),
                            );
                        }
                        notes.push(format!(
                            "profile: entry_count={} passinfo={}",
                            entry_profile.as_str(),
                            pass_profile.as_str()
                        ));
                        "canonical protocol::lba7 (rolling XOR)".into()
                    }
                    None => {
                        notes.push(
                            "无法从已知 entry-count/pass-info profile 中唯一解析 LBA7；拒绝猜测。"
                                .into(),
                        );
                        "RAW（LBA7 profile 未唯一确定）".into()
                    }
                }
            } else {
                "RAW（缺 device_id，无法解 LBA7）".into()
            }
        }
        8 => {
            if let Some((crc, _)) = crc_key(meta) {
                match infer_lba8(raw_sector, crc, meta_onlyid(meta)) {
                    Some((view, usb_profiles, host_profiles)) => {
                        decoded = view.mixed_plain().to_vec();
                        fields.push(field(0x000, 0x004, "LLGB magic", "LLGB", FieldStyle::Magic));
                        fields.push(field(
                            0x004,
                            0x008,
                            "logical length",
                            view.logical_length.to_string(),
                            FieldStyle::Size,
                        ));
                        fields.push(field(
                            0x008,
                            0x00c,
                            "tool version",
                            hex_bytes(&view.tool_version),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x00c,
                            0x010,
                            "lab version",
                            format!("0x{:08X}", view.lab_version),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x010,
                            0x014,
                            "writeTime",
                            view.write_time.to_string(),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x014,
                            0x018,
                            "HDSerialInfo/MyHardinfo",
                            format!("0x{:08X}", view.host_hardinfo),
                            FieldStyle::Identity,
                        ));
                        fields.push(field(
                            0x018,
                            0x01e,
                            "MacInfo",
                            hex_bytes(&view.mac_info),
                            FieldStyle::Identity,
                        ));
                        let usb = match &view.usb_only_info {
                            lba8::UsbOnlyInfo::Current(bytes)
                            | lba8::UsbOnlyInfo::Transitional(bytes) => text_value(bytes),
                            lba8::UsbOnlyInfo::StrictLegacyAbsent => "<absent>".into(),
                        };
                        let usb_candidates = usb_profiles
                            .iter()
                            .map(|profile| profile.as_str())
                            .collect::<Vec<_>>()
                            .join(",");
                        let host_candidates = host_profiles
                            .iter()
                            .map(|profile| profile.as_str())
                            .collect::<Vec<_>>()
                            .join(",");
                        fields.push(field(
                            0x01e,
                            0x02e,
                            "UsbOnlyInfo",
                            format!("{usb}；profile 候选={usb_candidates}"),
                            FieldStyle::Identity,
                        ));
                        fields.push(field(
                            0x02e,
                            0x03e,
                            "UsbOnlyInfo suffix",
                            hex_bytes(&view.usb_only_suffix),
                            FieldStyle::Flag,
                        ));
                        fields.push(field(
                            0x03e,
                            0x040,
                            "ELABEL offset",
                            format!("0x{:04X}", view.elab_offset),
                            FieldStyle::Address,
                        ));
                        fields.push(field(
                            0x040,
                            0x080,
                            "reserved header",
                            hex_bytes(&view.reserved_header),
                            FieldStyle::Flag,
                        ));
                        fields.push(elabel_field(0x080, &view.elabel_body));
                        if !view.encrypted_backing.is_empty() {
                            let start = 0x080 + view.elabel_body.len() + 1;
                            fields.push(field(
                                start,
                                view.encrypted_len(),
                                "encrypted backing",
                                hex_bytes(&view.encrypted_backing),
                                FieldStyle::Flag,
                            ));
                        }
                        if !view.tail_backing.is_empty() {
                            fields.push(field(
                                view.encrypted_len(),
                                SECTOR,
                                "raw tail backing",
                                hex_bytes(&view.tail_backing),
                                FieldStyle::Flag,
                            ));
                        }
                        notes.push(format!(
                            "profile 候选: UsbOnlyInfo=[{usb_candidates}] host-hardinfo=[{host_candidates}]；encrypted_len={}B",
                            view.encrypted_len()
                        ));
                        format!("canonical protocol::lba8 A6B0 前 {}B", view.encrypted_len())
                    }
                    None => {
                        notes.push("LBA8 已解出候选但无法在已知 UsbOnlyInfo/host-hardinfo profile 中唯一归类；拒绝强猜。".into());
                        "RAW（LBA8 profile 未唯一确定）".into()
                    }
                }
            } else {
                "RAW（缺 device_id，无法解 LBA8）".into()
            }
        }
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
                        decoded = view.decoded().to_vec();
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
                let profile = if raw.iter().all(|byte| *byte == 0) {
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
            } else if raw.iter().all(|byte| *byte == 0) {
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
                        decoded = view.decoded().to_vec();
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
    };

    SectorView {
        lba,
        raw: raw.to_vec(),
        decoded,
        method,
        fields,
        notes,
    }
}
