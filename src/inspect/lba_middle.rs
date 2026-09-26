use super::*;

pub(super) fn render_lba5_8(
    lba: u32,
    raw_sector: &[u8; SECTOR],
    meta: &InspectMeta,
    fields: &mut Vec<SectorField>,
    notes: &mut Vec<String>,
    decoded: &mut Vec<u8>,
) -> String {
    match lba {
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
                *decoded = view.decoded().to_vec();
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
                let calculated_checksum = lba6_checksum(&raw_sector[..0x1fc]);
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
                        *decoded = view.stored_plain().to_vec();
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
                        *decoded = view.mixed_plain().to_vec();
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
        _ => "RAW（无已知结构解析器）".into(),
    }
}
