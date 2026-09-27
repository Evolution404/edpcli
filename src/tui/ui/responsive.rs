//! The only TUI width breakpoints. Workspace layouts branch on this class.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ViewportClass {
    Compact,
    Standard,
    Wide,
    UltraWide,
}

impl ViewportClass {
    pub const fn for_width(width: u16) -> Self {
        match width {
            0..80 => Self::Compact,
            80..120 => Self::Standard,
            120..160 => Self::Wide,
            _ => Self::UltraWide,
        }
    }

    pub const fn has_secondary(self) -> bool {
        !matches!(self, Self::Compact)
    }

    pub const fn has_diagnostics(self) -> bool {
        matches!(self, Self::UltraWide)
    }
}
