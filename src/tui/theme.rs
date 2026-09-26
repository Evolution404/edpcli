//! Centralized TUI palette, terminal capability detection, and semantic styles.

use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};

use super::disk_layout::DiskRegionKind;
use super::state::{ProvisionBarKind, ProvisionKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    TrueColorDark,
    Ansi256Dark,
    Ansi16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub background: Color,
    pub surface: Color,
    pub surface_active: Color,
    pub selection: Color,
    pub border: Color,
    pub border_focus: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub violet: Color,
    pub partition_plain: Color,
    pub partition_boot: Color,
    pub partition_share: Color,
    pub partition_encrypt: Color,
    pub partition_compatibility: Color,
    pub partition_free: Color,
    pub animation_dim: Color,
    pub animation_accent: Color,
    pub animation_core: Color,
    pub animation_guard: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    mode: ThemeMode,
    palette: Palette,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationTone {
    Dim,
    Accent,
    Core,
    Guard,
}

impl Theme {
    pub const fn truecolor_dark() -> Self {
        Self {
            mode: ThemeMode::TrueColorDark,
            palette: Palette {
                background: Color::Rgb(0x11, 0x16, 0x1C),
                surface: Color::Rgb(0x17, 0x1D, 0x24),
                surface_active: Color::Rgb(0x1D, 0x25, 0x30),
                selection: Color::Rgb(0x26, 0x34, 0x42),
                border: Color::Rgb(0x30, 0x39, 0x45),
                border_focus: Color::Rgb(0x58, 0x75, 0x8D),
                text_primary: Color::Rgb(0xD7, 0xDC, 0xE2),
                text_secondary: Color::Rgb(0x9B, 0xA7, 0xB3),
                text_muted: Color::Rgb(0x68, 0x74, 0x81),
                accent: Color::Rgb(0x78, 0xA9, 0xC1),
                accent_soft: Color::Rgb(0x52, 0x75, 0x8A),
                success: Color::Rgb(0x7F, 0xA6, 0x8A),
                warning: Color::Rgb(0xB4, 0x9A, 0x68),
                danger: Color::Rgb(0xB7, 0x7C, 0x7C),
                violet: Color::Rgb(0x96, 0x87, 0xA8),
                partition_plain: Color::Rgb(0x76, 0x93, 0xAE),
                partition_boot: Color::Rgb(0x6E, 0x9C, 0xA5),
                partition_share: Color::Rgb(0x78, 0x97, 0x82),
                partition_encrypt: Color::Rgb(0x8F, 0x81, 0x9E),
                partition_compatibility: Color::Rgb(0xA0, 0x8D, 0x68),
                partition_free: Color::Rgb(0x46, 0x51, 0x5C),
                animation_dim: Color::Rgb(0x46, 0x51, 0x5C),
                animation_accent: Color::Rgb(0x6F, 0x91, 0xA5),
                animation_core: Color::Rgb(0x8C, 0xB1, 0xC3),
                animation_guard: Color::Rgb(0xB7, 0x7C, 0x7C),
            },
        }
    }

    pub const fn ansi256_dark() -> Self {
        Self {
            mode: ThemeMode::Ansi256Dark,
            palette: Palette {
                background: Color::Indexed(234),
                surface: Color::Indexed(235),
                surface_active: Color::Indexed(236),
                selection: Color::Indexed(238),
                border: Color::Indexed(239),
                border_focus: Color::Indexed(67),
                text_primary: Color::Indexed(253),
                text_secondary: Color::Indexed(145),
                text_muted: Color::Indexed(244),
                accent: Color::Indexed(109),
                accent_soft: Color::Indexed(66),
                success: Color::Indexed(108),
                warning: Color::Indexed(137),
                danger: Color::Indexed(131),
                violet: Color::Indexed(103),
                partition_plain: Color::Indexed(103),
                partition_boot: Color::Indexed(73),
                partition_share: Color::Indexed(108),
                partition_encrypt: Color::Indexed(103),
                partition_compatibility: Color::Indexed(137),
                partition_free: Color::Indexed(239),
                animation_dim: Color::Indexed(239),
                animation_accent: Color::Indexed(67),
                animation_core: Color::Indexed(109),
                animation_guard: Color::Indexed(131),
            },
        }
    }

    pub const fn ansi16() -> Self {
        Self {
            mode: ThemeMode::Ansi16,
            palette: Palette {
                background: Color::Black,
                surface: Color::Black,
                surface_active: Color::DarkGray,
                selection: Color::DarkGray,
                border: Color::DarkGray,
                border_focus: Color::Blue,
                text_primary: Color::Gray,
                text_secondary: Color::Gray,
                text_muted: Color::DarkGray,
                accent: Color::Cyan,
                accent_soft: Color::Blue,
                success: Color::Green,
                warning: Color::Yellow,
                danger: Color::Red,
                violet: Color::Magenta,
                partition_plain: Color::Blue,
                partition_boot: Color::Cyan,
                partition_share: Color::Green,
                partition_encrypt: Color::Magenta,
                partition_compatibility: Color::Yellow,
                partition_free: Color::DarkGray,
                animation_dim: Color::DarkGray,
                animation_accent: Color::Blue,
                animation_core: Color::Cyan,
                animation_guard: Color::Red,
            },
        }
    }

    pub fn from_capabilities(
        override_value: Option<&str>,
        colorterm: Option<&str>,
        term: Option<&str>,
    ) -> Self {
        match override_value
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("dark") => return Self::truecolor_dark(),
            Some("ansi256") => return Self::ansi256_dark(),
            Some("ansi16") => return Self::ansi16(),
            _ => {}
        }

        let colorterm = colorterm.unwrap_or_default().to_ascii_lowercase();
        let term = term.unwrap_or_default().to_ascii_lowercase();
        if matches!(colorterm.as_str(), "truecolor" | "24bit") || term.contains("direct") {
            Self::truecolor_dark()
        } else if term.contains("256color") {
            Self::ansi256_dark()
        } else {
            Self::ansi16()
        }
    }

    pub fn detect() -> Self {
        Self::from_capabilities(
            std::env::var("EDPCLI_TUI_THEME").ok().as_deref(),
            std::env::var("COLORTERM").ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
        )
    }

    pub const fn mode(self) -> ThemeMode {
        self.mode
    }

    pub const fn palette(self) -> Palette {
        self.palette
    }

    pub fn background(self) -> Style {
        Style::default()
            .fg(self.palette.text_primary)
            .bg(self.palette.background)
    }

    pub fn surface(self) -> Style {
        Style::default()
            .fg(self.palette.text_primary)
            .bg(self.palette.surface)
    }

    pub fn text(self) -> Style {
        Style::default().fg(self.palette.text_primary)
    }

    pub fn secondary_text(self) -> Style {
        Style::default().fg(self.palette.text_secondary)
    }

    pub fn muted(self) -> Style {
        Style::default().fg(self.palette.text_muted)
    }

    pub fn accent(self) -> Style {
        Style::default()
            .fg(self.palette.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn secondary_accent(self) -> Style {
        Style::default().fg(self.palette.violet)
    }

    pub fn selection(self) -> Style {
        Style::default()
            .fg(self.palette.text_primary)
            .bg(self.palette.selection)
    }

    pub fn selection_marker(self) -> Style {
        Style::default()
            .fg(self.palette.accent)
            .bg(self.palette.selection)
    }

    pub fn panel(self) -> Style {
        Style::default().fg(self.palette.border)
    }

    pub fn focused_panel(self) -> Style {
        Style::default().fg(self.palette.border_focus)
    }

    pub fn input(self) -> Style {
        Style::default()
            .fg(self.palette.text_primary)
            .bg(self.palette.surface)
    }

    pub fn input_focused(self) -> Style {
        Style::default()
            .fg(self.palette.text_primary)
            .bg(self.palette.surface_active)
    }

    pub fn success(self) -> Style {
        Style::default().fg(self.palette.success)
    }

    pub fn warning(self) -> Style {
        Style::default().fg(self.palette.warning)
    }

    pub fn danger(self) -> Style {
        Style::default()
            .fg(self.palette.danger)
            .add_modifier(Modifier::BOLD)
    }

    pub fn tab(self) -> Style {
        self.muted()
    }

    pub fn active_tab(self) -> Style {
        Style::default()
            .fg(self.palette.accent)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    }

    pub fn partition(self, kind: ProvisionBarKind) -> Style {
        let color = match kind {
            ProvisionBarKind::Free | ProvisionBarKind::Unknown => self.palette.partition_free,
            ProvisionBarKind::Plain => self.palette.partition_plain,
            ProvisionBarKind::Boot => self.palette.partition_boot,
            ProvisionBarKind::Share => self.palette.partition_share,
            ProvisionBarKind::Encrypt => self.palette.partition_encrypt,
            ProvisionBarKind::Compatibility => self.palette.partition_compatibility,
        };
        Style::default().fg(color)
    }

    pub fn disk_region(self, kind: DiskRegionKind) -> Style {
        let color = match kind {
            DiskRegionKind::Protocol => self.palette.accent,
            DiskRegionKind::Reserved => self.palette.partition_compatibility,
            DiskRegionKind::Unknown => self.palette.text_muted,
            DiskRegionKind::Free => self.palette.partition_free,
            DiskRegionKind::Plain => self.palette.partition_plain,
            DiskRegionKind::Boot => self.palette.partition_boot,
            DiskRegionKind::Share | DiskRegionKind::Combined => self.palette.partition_share,
            DiskRegionKind::Encrypt => self.palette.partition_encrypt,
            DiskRegionKind::Compatibility => self.palette.partition_compatibility,
            DiskRegionKind::Lce => self.palette.violet,
            DiskRegionKind::Tail => self.palette.accent_soft,
        };
        Style::default().fg(color)
    }

    pub fn provision_kind(self, kind: ProvisionKind) -> Style {
        let color = match kind {
            ProvisionKind::Mode0 => self.palette.accent,
            ProvisionKind::Mode1 => self.palette.violet,
            ProvisionKind::Mode2 => self.palette.warning,
            ProvisionKind::Mode3 => self.palette.success,
            ProvisionKind::Plain => self.palette.partition_plain,
        };
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }

    pub fn animation(self, tone: AnimationTone) -> Style {
        let color = match tone {
            AnimationTone::Dim => self.palette.animation_dim,
            AnimationTone::Accent => self.palette.animation_accent,
            AnimationTone::Core => self.palette.animation_core,
            AnimationTone::Guard => self.palette.animation_guard,
        };
        Style::default().fg(color)
    }
}

static THEME: OnceLock<Theme> = OnceLock::new();

pub fn current() -> &'static Theme {
    THEME.get_or_init(Theme::detect)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truecolor_palette_matches_chapter_ten_contract() {
        let palette = Theme::truecolor_dark().palette();
        assert_eq!(palette.background, Color::Rgb(0x11, 0x16, 0x1C));
        assert_eq!(palette.surface, Color::Rgb(0x17, 0x1D, 0x24));
        assert_eq!(palette.surface_active, Color::Rgb(0x1D, 0x25, 0x30));
        assert_eq!(palette.selection, Color::Rgb(0x26, 0x34, 0x42));
        assert_eq!(palette.border, Color::Rgb(0x30, 0x39, 0x45));
        assert_eq!(palette.border_focus, Color::Rgb(0x58, 0x75, 0x8D));
        assert_eq!(palette.text_primary, Color::Rgb(0xD7, 0xDC, 0xE2));
        assert_eq!(palette.text_secondary, Color::Rgb(0x9B, 0xA7, 0xB3));
        assert_eq!(palette.text_muted, Color::Rgb(0x68, 0x74, 0x81));
        assert_eq!(palette.accent, Color::Rgb(0x78, 0xA9, 0xC1));
        assert_eq!(palette.accent_soft, Color::Rgb(0x52, 0x75, 0x8A));
        assert_eq!(palette.success, Color::Rgb(0x7F, 0xA6, 0x8A));
        assert_eq!(palette.warning, Color::Rgb(0xB4, 0x9A, 0x68));
        assert_eq!(palette.danger, Color::Rgb(0xB7, 0x7C, 0x7C));
        assert_eq!(palette.violet, Color::Rgb(0x96, 0x87, 0xA8));
        assert_eq!(palette.partition_plain, Color::Rgb(0x76, 0x93, 0xAE));
        assert_eq!(palette.partition_boot, Color::Rgb(0x6E, 0x9C, 0xA5));
        assert_eq!(palette.partition_share, Color::Rgb(0x78, 0x97, 0x82));
        assert_eq!(palette.partition_encrypt, Color::Rgb(0x8F, 0x81, 0x9E));
        assert_eq!(
            palette.partition_compatibility,
            Color::Rgb(0xA0, 0x8D, 0x68)
        );
        assert_eq!(palette.partition_free, Color::Rgb(0x46, 0x51, 0x5C));
        assert_eq!(palette.animation_dim, Color::Rgb(0x46, 0x51, 0x5C));
        assert_eq!(palette.animation_accent, Color::Rgb(0x6F, 0x91, 0xA5));
        assert_eq!(palette.animation_core, Color::Rgb(0x8C, 0xB1, 0xC3));
        assert_eq!(palette.animation_guard, Color::Rgb(0xB7, 0x7C, 0x7C));
    }

    #[test]
    fn capability_detection_has_truecolor_ansi256_and_ansi16_paths() {
        assert_eq!(
            Theme::from_capabilities(None, Some("truecolor"), Some("xterm-256color")).mode(),
            ThemeMode::TrueColorDark
        );
        assert_eq!(
            Theme::from_capabilities(None, None, Some("xterm-256color")).mode(),
            ThemeMode::Ansi256Dark
        );
        assert_eq!(
            Theme::from_capabilities(None, None, Some("vt100")).mode(),
            ThemeMode::Ansi16
        );
    }

    #[test]
    fn explicit_theme_override_wins_over_terminal_detection() {
        assert_eq!(
            Theme::from_capabilities(Some("ansi16"), Some("truecolor"), Some("xterm-256color"))
                .mode(),
            ThemeMode::Ansi16
        );
        assert_eq!(
            Theme::from_capabilities(Some("ansi256"), Some("truecolor"), None).mode(),
            ThemeMode::Ansi256Dark
        );
        assert_eq!(
            Theme::from_capabilities(Some("dark"), None, Some("vt100")).mode(),
            ThemeMode::TrueColorDark
        );
    }

    #[test]
    fn selection_and_focus_use_distinct_low_contrast_semantics() {
        let theme = Theme::truecolor_dark();
        assert_eq!(theme.selection().fg, Some(theme.palette.text_primary));
        assert_eq!(theme.selection().bg, Some(theme.palette.selection));
        assert_ne!(theme.selection().bg, Some(theme.palette.accent));
        assert_ne!(theme.panel().fg, theme.focused_panel().fg);
        assert_ne!(theme.input().bg, theme.input_focused().bg);
    }
}
