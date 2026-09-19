//! Versioned canonical protocol profile.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisionProfile {
    id: &'static str,
    version: u32,
    glab: &'static str,
    autonum: &'static str,
}

impl ProvisionProfile {
    pub fn canonical_v1() -> Self {
        Self {
            id: "jiangsu-safe6-nopwd",
            version: 1,
            glab: "322CA28A-D7D1448B-DCE2CED9",
            autonum: "YD000001",
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
}
