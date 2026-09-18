//! 极简 XML plist 解析(替代 Python plistlib; 零依赖)。
//!
//! 只覆盖 diskutil `-plist` 输出实际用到的构造:
//! `<plist>/<dict>/<key>/<array>/<string>/<integer>/<true|false/>`;
//! `<data>/<date>/<real>` 解析为 Other(调用方从不读取)。
//! dict 重复键取最后一个(plistlib 字典语义)。integer 用 i64(盘容达 2×10¹²)。

#[derive(Debug, Clone, PartialEq)]
pub enum Plist {
    Str(String),
    Int(i64),
    Bool(bool),
    Arr(Vec<Plist>),
    Dict(Vec<(String, Plist)>),
    Other,
}

impl Plist {
    /// dict 取键(plistlib 语义: 重复键后者覆盖前者)。
    pub fn get(&self, key: &str) -> Option<&Plist> {
        match self {
            Plist::Dict(kvs) => kvs.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Plist::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Plist::Int(i) => Some(*i),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Plist::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&[Plist]> {
        match self {
            Plist::Arr(v) => Some(v),
            _ => None,
        }
    }
}

pub fn parse(text: &str) -> Result<Plist, String> {
    let mut p = Px {
        s: text.as_bytes(),
        pos: 0,
    };
    p.skip_prolog();
    let v = p.parse_value()?;
    Ok(v)
}

struct Px<'a> {
    s: &'a [u8],
    pos: usize,
}

impl<'a> Px<'a> {
    fn starts_with(&self, pat: &[u8]) -> bool {
        self.s[self.pos..].starts_with(pat)
    }

    fn find_from(&self, pat: &[u8]) -> Option<usize> {
        self.s[self.pos..]
            .windows(pat.len())
            .position(|w| w == pat)
            .map(|i| i + self.pos)
    }

    fn skip_ws(&mut self) {
        while self.pos < self.s.len() && self.s[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    /// 跳过 `<?…?>` / `<!--…-->` / `<!DOCTYPE …>`(diskutil 输出头部两样)。
    fn skip_prolog(&mut self) {
        loop {
            self.skip_ws();
            if self.starts_with(b"<?") {
                self.pos = self.find_from(b"?>").unwrap_or(self.s.len()) + 2;
            } else if self.starts_with(b"<!--") {
                self.pos = self.find_from(b"-->").unwrap_or(self.s.len()) + 3;
            } else if self.starts_with(b"<!") {
                self.pos = self.find_from(b">").unwrap_or(self.s.len()) + 1;
            } else {
                break;
            }
        }
    }

    /// 读一个 `<name …>` 或 `<name…/>` 开标签; 返回 (名字, 是否自闭合)。
    fn read_open_tag(&mut self) -> Result<(String, bool), String> {
        self.skip_ws();
        if self.pos >= self.s.len() || self.s[self.pos] != b'<' {
            return Err(format!("plist: 位置 {} 期望 '<'", self.pos));
        }
        self.pos += 1;
        let name_start = self.pos;
        while self.pos < self.s.len()
            && !self.s[self.pos].is_ascii_whitespace()
            && self.s[self.pos] != b'>'
            && self.s[self.pos] != b'/'
        {
            self.pos += 1;
        }
        let name = String::from_utf8_lossy(&self.s[name_start..self.pos]).into_owned();
        let mut self_closing = false;
        while self.pos < self.s.len() {
            match self.s[self.pos] {
                b'/' => {
                    self_closing = true;
                    self.pos += 1;
                }
                b'>' => {
                    self.pos += 1;
                    break;
                }
                _ => self.pos += 1, // 属性等, 跳过
            }
        }
        if name.is_empty() {
            return Err("plist: 空标签名".into());
        }
        Ok((name, self_closing))
    }

    /// 读到 `</name>` 为止的文本(不含标签), 并消费闭合标签。
    fn read_text_until_close(&mut self, name: &str) -> Result<Vec<u8>, String> {
        let close = format!("</{}>", name);
        let end = self
            .find_from(close.as_bytes())
            .ok_or_else(|| format!("plist: 缺少 {}", close))?;
        let text = self.s[self.pos..end].to_vec();
        self.pos = end + close.len();
        Ok(text)
    }

    /// 跳过(不解析)一个元素的完整内容到其闭合标签。用于 data/date/real。
    fn skip_element(&mut self, name: &str, self_closing: bool) -> Plist {
        if !self_closing {
            let _ = self.read_text_until_close(name);
        }
        Plist::Other
    }

    fn parse_value(&mut self) -> Result<Plist, String> {
        self.skip_prolog();
        let (name, self_closing) = self.read_open_tag()?;
        match name.as_str() {
            "plist" => {
                // 外壳: 解析其内唯一值
                let v = self.parse_value()?;
                let _ = self.read_text_until_close("plist"); // 允许尾随空白
                Ok(v)
            }
            "dict" => {
                if self_closing {
                    return Ok(Plist::Dict(vec![]));
                }
                let mut kvs = Vec::new();
                loop {
                    self.skip_ws();
                    if self.starts_with(b"</dict>") {
                        self.pos += 7;
                        return Ok(Plist::Dict(kvs));
                    }
                    if self.pos >= self.s.len() {
                        return Err("plist: dict 未闭合".into());
                    }
                    let (t, sc) = self.read_open_tag()?;
                    if t != "key" || sc {
                        return Err(format!("plist: dict 内期望 <key>, 遇 <{}>", t));
                    }
                    let key = decode_entities(&self.read_text_until_close("key")?);
                    let v = self.parse_value()?;
                    kvs.push((key, v));
                }
            }
            "array" => {
                if self_closing {
                    return Ok(Plist::Arr(vec![]));
                }
                let mut items = Vec::new();
                loop {
                    self.skip_ws();
                    if self.starts_with(b"</array>") {
                        self.pos += 8;
                        return Ok(Plist::Arr(items));
                    }
                    if self.pos >= self.s.len() {
                        return Err("plist: array 未闭合".into());
                    }
                    items.push(self.parse_value()?);
                }
            }
            "string" => {
                if self_closing {
                    return Ok(Plist::Str(String::new()));
                }
                Ok(Plist::Str(decode_entities(
                    &self.read_text_until_close("string")?,
                )))
            }
            "integer" => {
                if self_closing {
                    return Err("plist: 空 <integer/>".into());
                }
                let text = String::from_utf8_lossy(&self.read_text_until_close("integer")?)
                    .trim()
                    .to_string();
                text.parse::<i64>()
                    .map(Plist::Int)
                    .map_err(|e| format!("plist: integer {:?} 解析失败: {}", text, e))
            }
            "true" => {
                if !self_closing {
                    let _ = self.read_text_until_close("true");
                }
                Ok(Plist::Bool(true))
            }
            "false" => {
                if !self_closing {
                    let _ = self.read_text_until_close("false");
                }
                Ok(Plist::Bool(false))
            }
            "data" | "date" | "real" => Ok(self.skip_element(&name, self_closing)),
            other => Err(format!("plist: 不支持的标签 <{}>", other)),
        }
    }
}

/// XML 实体解码: 5 个预定义 + 十进制/十六进制数字引用。
fn decode_entities(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    if !s.contains('&') {
        return s.into_owned();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(end) = s[i..].find(';') {
                let ent = &s[i + 1..i + end];
                let repl = match ent {
                    "amp" => Some('&'.to_string()),
                    "lt" => Some('<'.to_string()),
                    "gt" => Some('>'.to_string()),
                    "quot" => Some('"'.to_string()),
                    "apos" => Some('\''.to_string()),
                    _ => {
                        if let Some(hex) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X"))
                        {
                            u32::from_str_radix(hex, 16)
                                .ok()
                                .and_then(char::from_u32)
                                .map(|c| c.to_string())
                        } else if let Some(dec) = ent.strip_prefix('#') {
                            dec.parse::<u32>()
                                .ok()
                                .and_then(char::from_u32)
                                .map(|c| c.to_string())
                        } else {
                            None
                        }
                    }
                };
                if let Some(r) = repl {
                    out.push_str(&r);
                    i += end + 1;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_LIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>AllDisks</key>
	<array>
		<string>disk0</string>
		<string>disk4</string>
		<string>disk4s1</string>
	</array>
	<key>AllDisksAndPartitions</key>
	<array>
		<dict>
			<key>DeviceIdentifier</key>
			<string>disk0</string>
			<key>Partitions</key>
			<array/>
		</dict>
	</array>
	<key>VolumeNames</key>
	<dict>
		<key>disk0</key>
		<string>Macintosh HD</string>
	</dict>
	<key>WholeDisks</key>
	<array>
		<string>disk0</string>
		<string>disk4</string>
	</array>
</dict>
</plist>
"#;

    #[test]
    fn parses_real_diskutil_list_shape() {
        let v = parse(REAL_LIST).unwrap();
        let disks: Vec<&str> = v
            .get("AllDisks")
            .and_then(|a| a.as_arr())
            .unwrap()
            .iter()
            .filter_map(|d| d.as_str())
            .collect();
        assert_eq!(disks, vec!["disk0", "disk4", "disk4s1"]);
        // 嵌套 dict/array(AllDisksAndPartitions)不消费也不报错
        assert!(v.get("Nope").is_none());
    }

    #[test]
    fn parses_disk_info_shape() {
        let xml = r#"<plist version="1.0"><dict>
	<key>DiskSize</key><integer>62914560000</integer>
	<key>TotalSize</key><integer>62914560000</integer>
	<key>Size</key><integer>62914560000</integer>
	<key>WholeDisk</key><true/>
	<key>Internal</key><false/>
	<key>VirtualOrPhysical</key><string>Physical</string>
	<key>BusProtocol</key><string>USB</string>
	<key>DeviceName</key><string>Netac &amp; Co</string>
</dict></plist>"#;
        let v = parse(xml).unwrap();
        assert_eq!(
            v.get("DiskSize").and_then(|x| x.as_int()),
            Some(62_914_560_000)
        );
        assert_eq!(v.get("WholeDisk").and_then(|x| x.as_bool()), Some(true));
        assert_eq!(v.get("Internal").and_then(|x| x.as_bool()), Some(false));
        assert_eq!(v.get("BusProtocol").and_then(|x| x.as_str()), Some("USB"));
        assert_eq!(
            v.get("DeviceName").and_then(|x| x.as_str()),
            Some("Netac & Co")
        );
    }

    #[test]
    fn error_plist_degrades_via_missing_keys() {
        // diskutil 出错时输出 <dict><key>Error</key><true/>… — 解析成功, 键缺失走降级
        let xml = r#"<plist version="1.0"><dict><key>Error</key><true/><key>ErrorMessage</key><string>Could not find disk</string></dict></plist>"#;
        let v = parse(xml).unwrap();
        assert_eq!(v.get("Error").and_then(|x| x.as_bool()), Some(true));
        assert!(v.get("DiskSize").is_none());
    }

    #[test]
    fn duplicate_key_last_wins() {
        let xml = r#"<plist version="1.0"><dict><key>Size</key><integer>1</integer><key>Size</key><integer>2</integer></dict></plist>"#;
        let v = parse(xml).unwrap();
        assert_eq!(v.get("Size").and_then(|x| x.as_int()), Some(2));
    }

    #[test]
    fn self_closing_and_empty_containers() {
        let xml = r#"<plist version="1.0"><dict><key>A</key><array/><key>B</key><dict/><key>C</key><string/></dict></plist>"#;
        let v = parse(xml).unwrap();
        assert_eq!(v.get("A").unwrap(), &Plist::Arr(vec![]));
        assert_eq!(v.get("B").unwrap(), &Plist::Dict(vec![]));
        assert_eq!(v.get("C").unwrap(), &Plist::Str(String::new()));
    }

    #[test]
    fn data_date_real_become_other() {
        let xml = r#"<plist version="1.0"><dict><key>D</key><data>AAAA</data><key>T</key><date>2026-09-16T23:36:26Z</date></dict></plist>"#;
        let v = parse(xml).unwrap();
        assert_eq!(v.get("D"), Some(&Plist::Other));
        assert_eq!(v.get("T"), Some(&Plist::Other));
    }

    #[test]
    fn numeric_entities() {
        let xml = r#"<plist version="1.0"><string>a&#65;b&#x42;c</string></plist>"#;
        let v = parse(xml).unwrap();
        assert_eq!(v.as_str(), Some("aAbBc"));
    }
}
