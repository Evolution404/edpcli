use super::*;

pub fn analyze_mbr_sector(raw: &[u8]) -> SectorView {
    if raw.len() != SECTOR {
        return SectorView {
            lba: 0,
            raw: raw.to_vec(),
            decoded: raw.to_vec(),
            decode_ranges: Vec::new(),
            method: format!("RAW（短读：{}B，应为 {}B）", raw.len(), SECTOR),
            fields: vec![],
            notes: vec!["MBR 扇区长度异常，停止结构化解析。".into()],
            parse_state: InspectParseState::Invalid,
            diagnostics: vec![InspectDiagnostic::new(
                InspectDiagnosticCode::ShortSector,
                format!("MBR 扇区长度 {}B，预期 {SECTOR}B", raw.len()),
            )],
        };
    }

    let mut fields = Vec::new();
    let mut notes = Vec::new();
    parse_mbr(raw, &mut fields, &mut notes);
    let valid = raw[510..512] == [0x55, 0xaa];
    let diagnostics = if valid {
        Vec::new()
    } else {
        vec![InspectDiagnostic::new(
            InspectDiagnosticCode::CanonicalParserRejected,
            "MBR 缺少 55 AA 签名",
        )]
    };
    SectorView {
        lba: 0,
        raw: raw.to_vec(),
        decoded: raw.to_vec(),
        method: "标准 MBR 分区表（raw=decoded）".into(),
        decode_ranges: Vec::new(),
        fields,
        notes,
        parse_state: if valid {
            InspectParseState::Parsed
        } else {
            InspectParseState::Invalid
        },
        diagnostics,
    }
}
