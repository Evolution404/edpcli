#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lba8IdentityField {
    Glab,
    Indus,
    Orgcd,
    Org,
    Unit,
    Alarm,
    Autonum,
    Rmark,
    Vol0,
    Vol1,
    Vol2,
    Volc0,
    Volc1,
    Volc2,
}

impl Lba8IdentityField {
    pub(crate) const ALL: [Self; 14] = [
        Self::Glab,
        Self::Indus,
        Self::Orgcd,
        Self::Org,
        Self::Unit,
        Self::Alarm,
        Self::Autonum,
        Self::Rmark,
        Self::Vol0,
        Self::Vol1,
        Self::Vol2,
        Self::Volc0,
        Self::Volc1,
        Self::Volc2,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Glab => "GLab",
            Self::Indus => "Indus",
            Self::Orgcd => "Orgcd",
            Self::Org => "Org",
            Self::Unit => "Unit",
            Self::Alarm => "Alarm",
            Self::Autonum => "Autonum",
            Self::Rmark => "Rmark",
            Self::Vol0 => "VOL0",
            Self::Vol1 => "VOL1",
            Self::Vol2 => "VOL2",
            Self::Volc0 => "VOLC0",
            Self::Volc1 => "VOLC1",
            Self::Volc2 => "VOLC2",
        }
    }

    pub(crate) fn value(self, identity: &crate::provision::Lba8Identity) -> &str {
        match self {
            Self::Glab => &identity.glab,
            Self::Indus => &identity.indus,
            Self::Orgcd => &identity.orgcd,
            Self::Org => &identity.org,
            Self::Unit => &identity.unit,
            Self::Alarm => &identity.alarm,
            Self::Autonum => &identity.autonum,
            Self::Rmark => &identity.rmark,
            Self::Vol0 => &identity.vol0,
            Self::Vol1 => &identity.vol1,
            Self::Vol2 => &identity.vol2,
            Self::Volc0 => &identity.volc0,
            Self::Volc1 => &identity.volc1,
            Self::Volc2 => &identity.volc2,
        }
    }

    pub(crate) fn value_mut(self, identity: &mut crate::provision::Lba8Identity) -> &mut String {
        match self {
            Self::Glab => &mut identity.glab,
            Self::Indus => &mut identity.indus,
            Self::Orgcd => &mut identity.orgcd,
            Self::Org => &mut identity.org,
            Self::Unit => &mut identity.unit,
            Self::Alarm => &mut identity.alarm,
            Self::Autonum => &mut identity.autonum,
            Self::Rmark => &mut identity.rmark,
            Self::Vol0 => &mut identity.vol0,
            Self::Vol1 => &mut identity.vol1,
            Self::Vol2 => &mut identity.vol2,
            Self::Volc0 => &mut identity.volc0,
            Self::Volc1 => &mut identity.volc1,
            Self::Volc2 => &mut identity.volc2,
        }
    }
}
