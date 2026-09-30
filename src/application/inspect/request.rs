use super::*;

fn parse_u64_decimal(value: &str, label: &str) -> Result<u64, InspectError> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(InspectError::invalid(format!(
            "{label} 必须为非负十进制整数"
        )));
    }
    value
        .parse::<u64>()
        .map_err(|_| InspectError::invalid(format!("{label} 超出 u64 范围")))
}

pub fn parse_advanced_lbas(spec: &str, count: &str) -> Result<Vec<u64>, InspectError> {
    use std::collections::HashSet;

    let spec = spec.trim();
    let count = count.trim();
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    if spec.is_empty() {
        if !count.is_empty() {
            return Err(InspectError::invalid("填写 count 时必须先填写单个起始 LBA"));
        }
        return Ok((0..METADATA_SECTOR_COUNT as u64).collect());
    }

    for token in spec.split(',').map(str::trim) {
        if token.is_empty() {
            return Err(InspectError::invalid("LBA 列表包含空项"));
        }
        if let Some((start, end)) = token.split_once('-') {
            let start = parse_u64_decimal(start, "LBA 范围起点")?;
            let end = parse_u64_decimal(end, "LBA 范围终点")?;
            if start > end {
                return Err(InspectError::invalid(format!(
                    "LBA 范围起点大于终点: {token}"
                )));
            }
            let span = end
                .checked_sub(start)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| InspectError::invalid(format!("LBA 范围溢出: {token}")))?;
            if span > MAX_ADVANCED_INSPECT_SECTORS as u64 {
                return Err(InspectError::invalid(format!(
                    "单个 LBA 范围最多包含 {MAX_ADVANCED_INSPECT_SECTORS} 个扇区"
                )));
            }
            for lba in start..=end {
                if seen.insert(lba) {
                    if out.len() >= MAX_ADVANCED_INSPECT_SECTORS {
                        return Err(InspectError::invalid(format!(
                            "单次 Inspect 最多读取 {MAX_ADVANCED_INSPECT_SECTORS} 个扇区"
                        )));
                    }
                    out.push(lba);
                }
            }
        } else {
            let lba = parse_u64_decimal(token, "LBA")?;
            if seen.insert(lba) {
                if out.len() >= MAX_ADVANCED_INSPECT_SECTORS {
                    return Err(InspectError::invalid(format!(
                        "单次 Inspect 最多读取 {MAX_ADVANCED_INSPECT_SECTORS} 个扇区"
                    )));
                }
                out.push(lba);
            }
        }
    }

    if !count.is_empty() {
        if out.len() != 1 {
            return Err(InspectError::invalid("count 只能与单个起始 LBA 同时使用"));
        }
        let count = parse_u64_decimal(count, "count")?;
        if count == 0 || count > MAX_ADVANCED_INSPECT_SECTORS as u64 {
            return Err(InspectError::invalid(format!(
                "count 必须为 1..={MAX_ADVANCED_INSPECT_SECTORS}"
            )));
        }
        let start = out[0];
        out.clear();
        for offset in 0..count {
            out.push(
                start
                    .checked_add(offset)
                    .ok_or_else(|| InspectError::invalid("count 产生的 LBA 范围溢出"))?,
            );
        }
    }

    Ok(out)
}
