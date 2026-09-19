//! Versioned canonical protocol profile.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisionProfile {
    id: &'static str,
    version: u32,
    glab: &'static str,
    autonum: &'static str,
    lba4_profile_word: [u8; 4],
    lba7_material: [u8; 16],
    lba12_material: [u8; 24],
    lba7_terminator: [u8; 8],
    lba12_terminator: [u8; 8],
}

impl ProvisionProfile {
    pub fn canonical_v1() -> Self {
        Self {
            id: "jiangsu-safe6-nopwd",
            version: 1,
            glab: "322CA28A-D7D1448B-DCE2CED9",
            autonum: "YD000001",
            lba4_profile_word: [0x78, 0xad, 0x17, 0xa0],
            lba7_material: [
                0x5d, 0x73, 0x29, 0x04, 0x97, 0xbc, 0x69, 0xf1, 0xec, 0x0f, 0x75, 0x79, 0xe4, 0xdb,
                0x45, 0xa9,
            ],
            lba12_material: [
                0x5d, 0x73, 0x29, 0x04, 0xcf, 0x18, 0x96, 0xfe, 0xee, 0xf4, 0x08, 0x82, 0x9e, 0xc2,
                0xd5, 0xf5, 0x40, 0x66, 0x0f, 0x21, 0x3e, 0x70, 0x95, 0x2e,
            ],
            lba7_terminator: [0xec, 0x00, 0x01, 0x77, 0x00, 0x01, 0x77, 0x00],
            lba12_terminator: [0x8e, 0x02, 0x01, 0x77, 0x00, 0x01, 0x77, 0x00],
        }
    }

    pub fn id(&self) -> &'static str {
        self.id
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn glab(&self) -> &'static str {
        self.glab
    }

    pub fn autonum(&self) -> &'static str {
        self.autonum
    }

    pub(crate) fn lba4_profile_word(&self) -> [u8; 4] {
        self.lba4_profile_word
    }

    pub(crate) fn lba7_material(&self) -> &[u8; 16] {
        &self.lba7_material
    }

    pub(crate) fn lba12_material(&self) -> &[u8; 24] {
        &self.lba12_material
    }

    pub(crate) fn lba7_terminator(&self) -> &[u8; 8] {
        &self.lba7_terminator
    }

    pub(crate) fn lba12_terminator(&self) -> &[u8; 8] {
        &self.lba12_terminator
    }

    pub(crate) fn safe6_template(&self) -> [u8; 512] {
        decode_hex_512(SAFE6_TEMPLATE_HEX)
    }
}

fn decode_hex_512(hex: &str) -> [u8; 512] {
    debug_assert_eq!(hex.len(), 1024);
    let mut out = [0u8; 512];
    for (index, slot) in out.iter_mut().enumerate() {
        let offset = index * 2;
        *slot = u8::from_str_radix(&hex[offset..offset + 2], 16).expect("profile hex");
    }
    out
}

// Decoded LBA6 from a verified real SAFE6 sample. All target/user identity fields are
// overwritten by generate.rs before encryption. Opaque template bytes are frozen here
// so provisioning never guesses or zero-fills unknown reserved bytes.
const SAFE6_TEMPLATE_HEX: &str = concat!(
    "bdadcbd5caa1b5e7c1a6d3d0cfdeb9abcbbe2fcca9d6ddb9a9b5e7b9abcbbe2f",
    "cae4b5e7d4cbbcecd6d0d0c40000000000000000000000000000000000000000",
    "f0ac3c0074fcbb0700b40ecd10ebf288cbced0f1c1d500fe4610807e040b740b",
    "807e040c7405a0b60775d28046020683594430303030303100007305a0b607eb",
    "00813efe7d55aa740b807e100074c8a0b707eba98bfc1e578bf5cbbf05008a56",
    "00b408cd1372238ac1243f988ade8afc43f7e38bd186d6b106d2ee42f7e23956",
    "0a77237205394608731cb80102bb007c8b4e028b5600cd1373514f744e32e48a",
    "5600cd13ebe48a560060bbaa55b441cd13723681fb55aa7530f6c101742b6160",
    "1988a7f132104fe376086a0068007c6a016a10b4428bf4cd136161730e4f740b",
    "32e48a5600cd13ebd661f9c3496e76616c696420706172746974696f6e207461",
    "626c65004572726f72206c6f6164696e67206f7065726174696e672073797374",
    "656d004d697373696e67206f7065726174696e672073797374656d0000000000",
    "0000000000000000bdadcbd5b5e7c1a621534146453600000000000000000000",
    "00020000000000000000000000000000000000002c4463462877090000000133",
    "323243413238410000e0f5010000000000000000000000000000000000000000",
    "0000000000000000000000000000000001000000000000000000000088bbfba8"
);
