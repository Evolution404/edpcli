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

    pub const fn backup_device_sidebar_width(self) -> Option<u16> {
        match self {
            Self::Compact => None,
            Self::Standard => Some(30),
            Self::Wide => Some(36),
            Self::UltraWide => Some(40),
        }
    }

    pub fn backup_device_tree_inner_width(self, terminal_width: u16) -> u16 {
        self.backup_device_sidebar_width()
            .unwrap_or(terminal_width)
            .saturating_sub(2)
            .max(1)
    }
}
