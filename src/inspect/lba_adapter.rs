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
        0..=4 => super::lba_early::render_lba0_4(
            lba,
            raw_sector,
            protocol_image,
            &mut fields,
            &mut notes,
            &mut decoded,
        ),
        5..=8 => super::lba_middle::render_lba5_8(
            lba,
            raw_sector,
            meta,
            &mut fields,
            &mut notes,
            &mut decoded,
        ),
        9..=12 => super::lba_late::render_lba9_12(
            lba,
            raw_sector,
            meta,
            protocol_image,
            &mut fields,
            &mut notes,
            &mut decoded,
        ),
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
