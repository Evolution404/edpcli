//! SAFE6 restore node. Writer provenance is explicit; identity shape never selects encoding.
use super::{layout, profile::*, types::*};
use crate::crypto::xor_rolling;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Lba4Context {
    pub encoding: Lba4Encoding,
    pub second_key_source: Lba4SecondKeySource,
    pub hserial_source: Lba4HserialSource,
    pub host_hardinfo_source: HostHardinfoSource,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServerFlags {
    pub data_to_server: u8,
    pub connect_server: u8,
}
#[derive(Clone, Debug)]
pub struct RestoreNode {
    pub onlyid_guard: u32,
    pub second_key: u32,
    pub hserial: [u32; 5],
    pub single_usb: u8,
    pub host_hardinfo: u32,
    pub new_lab_flag: [u8; 4],
    pub version: u32,
    pub sector_tuple: [u8; 4],
}
#[derive(Clone, Debug)]
pub struct RollingReaderView([u8; 512]);
impl RollingReaderView {
    pub fn bytes(&self) -> &[u8; 512] {
        &self.0
    }
}
#[derive(Clone, Debug)]
pub struct Lba4View {
    wire: WireSector,
    pub reader: RollingReaderView,
    pub onlyid: u32,
    pub onlyid_text: String,
    pub context: Lba4Context,
    pub node: RestoreNode,
    pub representation_backing: Backing<437>,
    pub trailing_magic: [u8; 4],
    raw_zero_backing: bool,
}
impl Lba4View {
    pub fn reconstruct(&self) -> [u8; 512] {
        self.wire.0
    }
    pub fn backing_is_raw_zero(&self) -> bool {
        self.raw_zero_backing
    }
    pub fn reader_flags(&self) -> ServerFlags {
        flags(&self.reader.0)
    }
    pub fn wire_flags(&self) -> ServerFlags {
        flags(&self.wire.0)
    }
    pub fn producer_flags(&self) -> Option<ServerFlags> {
        match self.context.encoding {
            Lba4Encoding::PostXor => Some(self.wire_flags()),
            Lba4Encoding::OrdinaryRolling => Some(self.reader_flags()),
            Lba4Encoding::Unknown => None,
        }
    }
    /// Re-encode the typed node and retained backing under the declared representation.
    /// Unknown writer provenance deliberately prevents producing wire bytes.
    pub fn encode(&self) -> Result<[u8; 512]> {
        let flags = self
            .producer_flags()
            .ok_or(ProtocolError::UnsupportedProfile {
                axis: "lba4_encoding",
            })?;
        let mut plain = self.reader.0;
        let mut put = |id: &str, data: &[u8]| {
            let f = layout::field(id, "post-xor").unwrap();
            plain[f.start..=f.end].copy_from_slice(data);
        };
        put("lba4.onlyid_xor8", &self.node.onlyid_guard.to_le_bytes());
        put("lba4.second_key", &self.node.second_key.to_le_bytes());
        let hserial: Vec<_> = self
            .node
            .hserial
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        put("lba4.hserial", &hserial);
        put("lba4.single_usb", &[self.node.single_usb]);
        put("lba4.my_hardinfo", &self.node.host_hardinfo.to_le_bytes());
        put("lba4.new_lab_flag", &self.node.new_lab_flag);
        put("lba4.version", &self.node.version.to_le_bytes());
        put("lba4.sector_tuple", &self.node.sector_tuple);
        put("lba4.data_to_server", &[flags.data_to_server]);
        put("lba4.connect_server", &[flags.connect_server]);
        put(
            "lba4.representation_backing",
            self.representation_backing.bytes(),
        );
        put("lba4.trailing_llgb", &self.trailing_magic);
        let mut out = plain;
        out[24..].copy_from_slice(&xor_rolling(
            &plain[24..],
            (self.onlyid & 0xffff) ^ (self.onlyid >> 16),
        ));
        if self.context.encoding == Lba4Encoding::PostXor {
            out[69] = flags.data_to_server;
            out[70] = flags.connect_server;
        }
        if self.raw_zero_backing {
            out[71..508].fill(0);
        }
        Ok(out)
    }
}
fn flags(b: &[u8; 512]) -> ServerFlags {
    ServerFlags {
        data_to_server: b[69],
        connect_server: b[70],
    }
}
pub fn parse_lba4(raw: &[u8; 512], context: Lba4Context) -> Result<Lba4View> {
    let bad = |field| ProtocolError::InvalidField { lba: 4, field };
    let header = layout::bytes(raw, "lba4.onlyid_header", "all");
    if &header[..3] != b"$$$" {
        return Err(bad("onlyid_header"));
    }
    let end = header[3..]
        .windows(3)
        .position(|b| b == b"$$$")
        .ok_or(bad("onlyid_header"))?
        + 3;
    let text = std::str::from_utf8(&header[3..end]).map_err(|_| bad("onlyid_header"))?;
    let onlyid = if text.starts_with('-') {
        text.parse::<i32>().map_err(|_| bad("onlyid_header"))? as u32
    } else {
        text.parse::<u32>().map_err(|_| bad("onlyid_header"))?
    };
    let mut reader = *raw;
    reader[24..].copy_from_slice(&xor_rolling(&raw[24..], (onlyid & 0xffff) ^ (onlyid >> 16)));
    let gap = layout::bytes(raw, "lba4.representation_backing", "post-xor");
    let raw_zero_backing = gap.iter().all(|b| *b == 0);
    if raw_zero_backing {
        reader[71..508].fill(0);
    }
    let f = |id| layout::bytes(&reader, id, "post-xor");
    let node = RestoreNode {
        onlyid_guard: u32le(f("lba4.onlyid_xor8"), 0),
        second_key: u32le(f("lba4.second_key"), 0),
        hserial: std::array::from_fn(|i| u32le(f("lba4.hserial"), 4 * i)),
        single_usb: f("lba4.single_usb")[0],
        host_hardinfo: u32le(f("lba4.my_hardinfo"), 0),
        new_lab_flag: f("lba4.new_lab_flag").try_into().unwrap(),
        version: u32le(f("lba4.version"), 0),
        sector_tuple: f("lba4.sector_tuple").try_into().unwrap(),
    };
    if node.onlyid_guard != onlyid ^ 0x8888_8888 {
        return Err(bad("onlyid_guard"));
    }
    // These are caller-supplied provenance claims, not inferred writer generations.
    if context.second_key_source == Lba4SecondKeySource::CurrentMainOnlyid
        && node.second_key != onlyid
    {
        return Err(bad("second_key_source"));
    }
    if context.hserial_source == Lba4HserialSource::CurrentZero && node.hserial != [0; 5] {
        return Err(bad("hserial_source"));
    }
    if context.host_hardinfo_source == HostHardinfoSource::CurrentZero && node.host_hardinfo != 0 {
        return Err(bad("host_hardinfo_source"));
    }
    Ok(Lba4View {
        wire: WireSector(*raw),
        onlyid,
        onlyid_text: text.into(),
        context,
        node,
        representation_backing: Backing(f("lba4.representation_backing").try_into().unwrap()),
        trailing_magic: f("lba4.trailing_llgb").try_into().unwrap(),
        raw_zero_backing,
        reader: RollingReaderView(reader),
    })
}
