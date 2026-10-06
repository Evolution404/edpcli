#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionDisposition {
    PreserveOpaque,
    PreserveVerified,
    RewrapVerified,
    Rebuild,
    Drop,
}

impl RegionDisposition {
    pub const fn preserves_extent(self) -> bool {
        matches!(
            self,
            Self::PreserveOpaque | Self::PreserveVerified | Self::RewrapVerified
        )
    }
}
