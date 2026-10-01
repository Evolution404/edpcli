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
            decode_ranges: Vec::new(),
            method: format!("RAW（短读：{}B，应为 {}B）", raw.len(), SECTOR),
            fields: vec![],
            notes: vec!["扇区长度异常，停止结构化解析。".into()],
            parse_state: InspectParseState::Invalid,
            diagnostics: vec![InspectDiagnostic::new(
                InspectDiagnosticCode::ShortSector,
                format!("扇区长度 {}B，预期 {SECTOR}B", raw.len()),
            )],
        };
    }
    let raw_sector = raw_sector(raw).expect("length checked");
    let mut fields = Vec::new();
    let mut notes = Vec::new();
    let mut diagnostics = Vec::new();
    let mut decoded = raw.to_vec();
    let mut decode_ranges = Vec::new();
    macro_rules! buffers {
        () => {
            LbaRenderBuffers {
                fields: &mut fields,
                notes: &mut notes,
                decoded: &mut decoded,
                decode_ranges: &mut decode_ranges,
                diagnostics: &mut diagnostics,
            }
        };
    }

    let method = match lba {
        0..=4 => super::lba_early::render_lba0_4(lba, raw_sector, protocol_image, buffers!()),
        5..=8 => super::lba_middle::render_lba5_8(lba, raw_sector, meta, buffers!()),
        9..=12 => super::lba_late::render_lba9_12(
            lba,
            raw_sector,
            super::lba_late::LbaLateContext {
                meta,
                protocol_image,
            },
            buffers!(),
        ),
        _ => {
            diagnostics.push(InspectDiagnostic::new(
                InspectDiagnosticCode::UnsupportedSector,
                "无已知结构解析器",
            ));
            "RAW（无已知结构解析器）".into()
        }
    };

    let parse_state = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.parse_state() != InspectParseState::MissingContext)
        .or_else(|| diagnostics.first())
        .map_or(InspectParseState::Parsed, |diagnostic| {
            diagnostic.code.parse_state()
        });

    SectorView {
        lba,
        raw: raw.to_vec(),
        decoded,
        decode_ranges,
        method,
        fields,
        notes,
        parse_state,
        diagnostics,
    }
}
