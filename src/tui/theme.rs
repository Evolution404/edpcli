//! Centralized TUI palette, terminal capability detection, and semantic styles.

use std::sync::OnceLock;

use ratatui::{
    style::{Color, Modifier, Style},
    widgets::BorderType,
};

use crate::provision::DiskProvisionKind;

use super::disk_layout::DiskRegionKind;
use super::state::ProvisionBarKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    TrueColorDark,
    Ansi256Dark,
    Ansi16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub canvas: Color,
    pub background: Color,
    pub surface: Color,
    pub surface_raised: Color,
    pub surface_active: Color,
    pub selection: Color,
    pub border_subtle: Color,
    pub border_strong: Color,
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
                canvas: Color::Rgb(0x07, 0x11, 0x1A),
                background: Color::Rgb(0x11, 0x16, 0x1C),
                surface: Color::Rgb(0x17, 0x1D, 0x24),
                surface_raised: Color::Rgb(0x0E, 0x1C, 0x27),
                surface_active: Color::Rgb(0x1D, 0x25, 0x30),
                selection: Color::Rgb(0x26, 0x34, 0x42),
                border_subtle: Color::Rgb(0x29, 0x3A, 0x47),
                border_strong: Color::Rgb(0x4A, 0x6B, 0x7C),
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
                violet: Color::Rgb(0x7F, 0xA3, 0xB8),
                partition_plain: Color::Rgb(0x6F, 0xA8, 0xDC),
                partition_boot: Color::Rgb(0x63, 0xC7, 0xBE),
                partition_share: Color::Rgb(0x78, 0xBE, 0x7E),
                partition_encrypt: Color::Rgb(0xB1, 0x8A, 0xCB),
                partition_compatibility: Color::Rgb(0xD0, 0xAA, 0x62),
                partition_free: Color::Rgb(0x6C, 0x78, 0x84),
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
                canvas: Color::Indexed(233),
                background: Color::Indexed(234),
                surface: Color::Indexed(235),
                surface_raised: Color::Indexed(236),
                surface_active: Color::Indexed(237),
                selection: Color::Indexed(238),
                border_subtle: Color::Indexed(238),
                border_strong: Color::Indexed(67),
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
                violet: Color::Indexed(110),
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
                canvas: Color::Black,
                background: Color::Black,
                surface: Color::Black,
                surface_raised: Color::Black,
                surface_active: Color::DarkGray,
                selection: Color::DarkGray,
                border_subtle: Color::DarkGray,
                border_strong: Color::Blue,
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
                violet: Color::Cyan,
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

    pub fn canvas(self) -> Style {
        Style::default()
            .fg(self.palette.text_primary)
            .bg(self.palette.canvas)
    }

    pub fn raised_surface(self) -> Style {
        Style::default()
            .fg(self.palette.text_primary)
            .bg(self.palette.surface_raised)
    }

    pub fn modal_surface(self) -> Style {
        self.raised_surface()
    }

    pub fn modal_background(self) -> Style {
        Style::default().bg(self.palette.surface_raised)
    }

    pub fn modal_border(self) -> Style {
        self.strong_border()
    }

    pub fn modal_title(self) -> Style {
        self.accent()
    }

    pub fn subtle_border(self) -> Style {
        Style::default().fg(self.palette.border_subtle)
    }

    pub fn strong_border(self) -> Style {
        Style::default().fg(self.palette.border_strong)
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
        self.selection_overlay(true)
    }

    pub fn selection_overlay(self, focused: bool) -> Style {
        Style::default().bg(if focused {
            self.palette.selection
        } else {
            self.palette.surface_active
        })
    }

    pub fn selection_marker(self) -> Style {
        Style::default()
            .fg(self.palette.accent)
            .bg(self.palette.selection)
    }

    pub fn cursor(self) -> Style {
        self.selection_overlay(true).add_modifier(Modifier::BOLD)
    }

    pub fn panel(self) -> Style {
        Style::default().fg(self.palette.border)
    }

    pub fn focused_panel(self) -> Style {
        Style::default().fg(self.palette.border_focus)
    }

    pub fn pane_border(self, focused: bool) -> Style {
        if focused {
            self.focused_panel()
        } else {
            self.subtle_border()
        }
    }

    pub const fn pane_border_type(self, focused: bool) -> BorderType {
        if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        }
    }

    pub fn pane_title(self, focused: bool) -> Style {
        if focused {
            self.accent().add_modifier(Modifier::BOLD)
        } else {
            self.secondary_text()
        }
    }

    pub const fn pane_title_prefix(self, focused: bool) -> &'static str {
        if focused {
            "▌ "
        } else {
            ""
        }
    }

    pub fn card_surface(self, focused: bool) -> Style {
        let background = match (self.mode, focused) {
            (ThemeMode::Ansi16, _) | (_, false) => self.palette.surface_raised,
            (_, true) => self.palette.surface_active,
        };
        Style::default()
            .fg(self.palette.text_primary)
            .bg(background)
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

    pub fn table_header(self, active: bool, pane_focused: bool) -> Style {
        if active && pane_focused {
            self.accent()
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            self.secondary_text().add_modifier(Modifier::BOLD)
        }
    }

    pub fn active_semantic(self, base: Style) -> Style {
        let foreground = base
            .fg
            .map(|color| self.brighter_semantic_color(color))
            .unwrap_or(self.palette.accent);
        base.fg(foreground).add_modifier(Modifier::BOLD)
    }

    pub fn table_cell(self, base: Style, column_active: bool, pane_focused: bool) -> Style {
        if column_active && pane_focused {
            self.active_semantic(base)
        } else if column_active {
            base.add_modifier(Modifier::BOLD)
        } else {
            base
        }
    }

    pub fn apply_selection(self, base: Style, selected: bool, pane_focused: bool) -> Style {
        if selected {
            base.bg(if pane_focused {
                self.palette.selection
            } else {
                self.palette.surface_active
            })
        } else {
            base
        }
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

    fn disk_region_color(self, kind: DiskRegionKind) -> Color {
        match kind {
            DiskRegionKind::Protocol => self.palette.accent,
            DiskRegionKind::Metadata | DiskRegionKind::Reserved => {
                self.palette.partition_compatibility
            }
            DiskRegionKind::Unknown => self.palette.text_muted,
            DiskRegionKind::Free => self.palette.partition_free,
            DiskRegionKind::Plain => self.palette.partition_plain,
            DiskRegionKind::Boot => self.palette.partition_boot,
            DiskRegionKind::Share | DiskRegionKind::Combined => self.palette.partition_share,
            DiskRegionKind::Encrypt => self.palette.partition_encrypt,
            DiskRegionKind::Compatibility => self.palette.partition_compatibility,
            DiskRegionKind::Lce => self.palette.accent,
            DiskRegionKind::BackupMirror => self.palette.warning,
            DiskRegionKind::RestoreNode | DiskRegionKind::Tail => self.palette.accent_soft,
        }
    }

    pub fn disk_region(self, kind: DiskRegionKind) -> Style {
        Style::default().fg(self.disk_region_color(kind))
    }

    pub fn disk_region_tree(self, kind: DiskRegionKind, active: bool) -> Style {
        if active {
            self.disk_region_outline(kind, true)
        } else {
            self.disk_region(kind)
        }
    }

    fn disk_region_active_color(self, kind: DiskRegionKind) -> Color {
        match self.mode {
            ThemeMode::TrueColorDark => match kind {
                DiskRegionKind::Protocol => Color::Rgb(0xB0, 0xD3, 0xDA),
                DiskRegionKind::Metadata
                | DiskRegionKind::Reserved
                | DiskRegionKind::Compatibility => Color::Rgb(0xD8, 0xC9, 0xA6),
                DiskRegionKind::Unknown => Color::Rgb(0xD2, 0xC5, 0xC0),
                DiskRegionKind::Free => Color::Rgb(0xCB, 0xD2, 0xD8),
                DiskRegionKind::Plain => Color::Rgb(0xB7, 0xD2, 0xE0),
                DiskRegionKind::Boot => Color::Rgb(0xB7, 0xD8, 0xD3),
                DiskRegionKind::Share | DiskRegionKind::Combined => Color::Rgb(0xBE, 0xDB, 0xC6),
                DiskRegionKind::Encrypt => Color::Rgb(0xD1, 0xC2, 0xDB),
                DiskRegionKind::Lce => Color::Rgb(0xB9, 0xD0, 0xD6),
                DiskRegionKind::BackupMirror => Color::Rgb(0xD7, 0xC9, 0xA8),
                DiskRegionKind::RestoreNode | DiskRegionKind::Tail => Color::Rgb(0xBC, 0xD0, 0xDF),
            },
            ThemeMode::Ansi256Dark => match kind {
                DiskRegionKind::Protocol => Color::Indexed(117),
                DiskRegionKind::Metadata
                | DiskRegionKind::Reserved
                | DiskRegionKind::Compatibility => Color::Indexed(180),
                DiskRegionKind::Unknown | DiskRegionKind::Free => Color::Indexed(250),
                DiskRegionKind::Plain => Color::Indexed(111),
                DiskRegionKind::Boot => Color::Indexed(116),
                DiskRegionKind::Share | DiskRegionKind::Combined => Color::Indexed(151),
                DiskRegionKind::Encrypt => Color::Indexed(183),
                DiskRegionKind::Lce => Color::Indexed(153),
                DiskRegionKind::BackupMirror => Color::Indexed(180),
                DiskRegionKind::RestoreNode | DiskRegionKind::Tail => Color::Indexed(110),
            },
            ThemeMode::Ansi16 => match kind {
                DiskRegionKind::Protocol | DiskRegionKind::Boot => Color::Cyan,
                DiskRegionKind::Metadata
                | DiskRegionKind::Reserved
                | DiskRegionKind::Compatibility => Color::Yellow,
                DiskRegionKind::Unknown | DiskRegionKind::Free => Color::Gray,
                DiskRegionKind::Plain | DiskRegionKind::RestoreNode | DiskRegionKind::Tail => {
                    Color::Blue
                }
                DiskRegionKind::Share | DiskRegionKind::Combined => Color::Green,
                DiskRegionKind::Encrypt => Color::Magenta,
                DiskRegionKind::Lce => Color::Cyan,
                DiskRegionKind::BackupMirror => Color::Yellow,
            },
        }
    }

    pub fn disk_region_fill_color(self, kind: DiskRegionKind, active: bool) -> Color {
        match self.mode {
            ThemeMode::TrueColorDark => match (kind, active) {
                (DiskRegionKind::Protocol, false) => Color::Rgb(0x2C, 0x62, 0x70),
                (DiskRegionKind::Protocol, true) => Color::Rgb(0x39, 0x7A, 0x89),
                (
                    DiskRegionKind::Metadata
                    | DiskRegionKind::Reserved
                    | DiskRegionKind::Compatibility,
                    false,
                ) => Color::Rgb(0x5B, 0x50, 0x3B),
                (
                    DiskRegionKind::Metadata
                    | DiskRegionKind::Reserved
                    | DiskRegionKind::Compatibility,
                    true,
                ) => Color::Rgb(0x74, 0x66, 0x4A),
                (DiskRegionKind::Unknown, false) => Color::Rgb(0x55, 0x4C, 0x49),
                (DiskRegionKind::Unknown, true) => Color::Rgb(0x6B, 0x5D, 0x58),
                (DiskRegionKind::Free, false) => Color::Rgb(0x50, 0x58, 0x61),
                (DiskRegionKind::Free, true) => Color::Rgb(0x68, 0x72, 0x7C),
                (DiskRegionKind::Plain, false) => Color::Rgb(0x34, 0x5B, 0x72),
                (DiskRegionKind::Plain, true) => Color::Rgb(0x44, 0x73, 0x8C),
                (DiskRegionKind::Boot, false) => Color::Rgb(0x35, 0x6B, 0x66),
                (DiskRegionKind::Boot, true) => Color::Rgb(0x43, 0x83, 0x7D),
                (DiskRegionKind::Share | DiskRegionKind::Combined, false) => {
                    Color::Rgb(0x3A, 0x76, 0x58)
                }
                (DiskRegionKind::Share | DiskRegionKind::Combined, true) => {
                    Color::Rgb(0x49, 0x8D, 0x6B)
                }
                (DiskRegionKind::Encrypt, false) => Color::Rgb(0x67, 0x50, 0x78),
                (DiskRegionKind::Encrypt, true) => Color::Rgb(0x7D, 0x63, 0x91),
                (DiskRegionKind::Lce, false) => Color::Rgb(0x35, 0x5D, 0x68),
                (DiskRegionKind::Lce, true) => Color::Rgb(0x46, 0x74, 0x80),
                (DiskRegionKind::BackupMirror, false) => Color::Rgb(0x5D, 0x50, 0x3A),
                (DiskRegionKind::BackupMirror, true) => Color::Rgb(0x76, 0x66, 0x4A),
                (DiskRegionKind::RestoreNode | DiskRegionKind::Tail, false) => {
                    Color::Rgb(0x38, 0x54, 0x6D)
                }
                (DiskRegionKind::RestoreNode | DiskRegionKind::Tail, true) => {
                    Color::Rgb(0x49, 0x6C, 0x88)
                }
            },
            ThemeMode::Ansi256Dark => {
                let index = match (kind, active) {
                    (DiskRegionKind::Protocol, false) => 23,
                    (DiskRegionKind::Protocol, true) => 30,
                    (
                        DiskRegionKind::Metadata
                        | DiskRegionKind::Reserved
                        | DiskRegionKind::Compatibility,
                        false,
                    ) => 58,
                    (
                        DiskRegionKind::Metadata
                        | DiskRegionKind::Reserved
                        | DiskRegionKind::Compatibility,
                        true,
                    ) => 94,
                    (DiskRegionKind::Unknown, false) => 95,
                    (DiskRegionKind::Unknown, true) => 131,
                    (DiskRegionKind::Free, false) => 240,
                    (DiskRegionKind::Free, true) => 244,
                    (DiskRegionKind::Plain, false) => 17,
                    (DiskRegionKind::Plain, true) => 24,
                    (DiskRegionKind::Boot, false) => 22,
                    (DiskRegionKind::Boot, true) => 29,
                    (DiskRegionKind::Share | DiskRegionKind::Combined, false) => 22,
                    (DiskRegionKind::Share | DiskRegionKind::Combined, true) => 28,
                    (DiskRegionKind::Encrypt, false) => 53,
                    (DiskRegionKind::Encrypt, true) => 60,
                    (DiskRegionKind::Lce, false) => 23,
                    (DiskRegionKind::Lce, true) => 30,
                    (DiskRegionKind::BackupMirror, false) => 58,
                    (DiskRegionKind::BackupMirror, true) => 94,
                    (DiskRegionKind::RestoreNode | DiskRegionKind::Tail, false) => 17,
                    (DiskRegionKind::RestoreNode | DiskRegionKind::Tail, true) => 24,
                };
                Color::Indexed(index)
            }
            ThemeMode::Ansi16 => {
                if active {
                    Color::DarkGray
                } else {
                    Color::Black
                }
            }
        }
    }

    pub fn disk_region_fill(self, kind: DiskRegionKind, active: bool) -> Style {
        let background = self.disk_region_fill_color(kind, active);
        let foreground = if active {
            self.disk_region_active_color(kind)
        } else {
            self.disk_region_color(kind)
        };
        let style = Style::default().fg(foreground).bg(background);
        if active {
            style.add_modifier(Modifier::BOLD)
        } else {
            style
        }
    }

    pub fn disk_region_content_text(self, kind: DiskRegionKind, active: bool) -> Style {
        let foreground = match self.mode {
            ThemeMode::TrueColorDark => match (kind, active) {
                (DiskRegionKind::Protocol, false) => Color::Rgb(0xDD, 0xEC, 0xEF),
                (DiskRegionKind::Protocol, true) => Color::Rgb(0xED, 0xF7, 0xF9),
                (
                    DiskRegionKind::Metadata
                    | DiskRegionKind::Reserved
                    | DiskRegionKind::Compatibility,
                    false,
                ) => Color::Rgb(0xE9, 0xE4, 0xD8),
                (
                    DiskRegionKind::Metadata
                    | DiskRegionKind::Reserved
                    | DiskRegionKind::Compatibility,
                    true,
                ) => Color::Rgb(0xF4, 0xEE, 0xDF),
                (DiskRegionKind::Unknown, false) => Color::Rgb(0xE6, 0xE1, 0xDE),
                (DiskRegionKind::Unknown, true) => Color::Rgb(0xF1, 0xEC, 0xE9),
                (DiskRegionKind::Free, false) => Color::Rgb(0xE1, 0xE5, 0xE9),
                (DiskRegionKind::Free, true) => Color::Rgb(0xF1, 0xF4, 0xF6),
                (DiskRegionKind::Plain, false) => Color::Rgb(0xE0, 0xEA, 0xF0),
                (DiskRegionKind::Plain, true) => Color::Rgb(0xEF, 0xF6, 0xFA),
                (DiskRegionKind::Boot, false) => Color::Rgb(0xE1, 0xEF, 0xEC),
                (DiskRegionKind::Boot, true) => Color::Rgb(0xF0, 0xF8, 0xF6),
                (DiskRegionKind::Share | DiskRegionKind::Combined, false) => {
                    Color::Rgb(0xE3, 0xEE, 0xE6)
                }
                (DiskRegionKind::Share | DiskRegionKind::Combined, true) => {
                    Color::Rgb(0xF1, 0xF8, 0xF3)
                }
                (DiskRegionKind::Encrypt, false) => Color::Rgb(0xEC, 0xE5, 0xF0),
                (DiskRegionKind::Encrypt, true) => Color::Rgb(0xF6, 0xF1, 0xF8),
                (DiskRegionKind::Lce, false) => Color::Rgb(0xE0, 0xEA, 0xED),
                (DiskRegionKind::Lce, true) => Color::Rgb(0xEF, 0xF5, 0xF7),
                (DiskRegionKind::BackupMirror, false) => Color::Rgb(0xEB, 0xE5, 0xD8),
                (DiskRegionKind::BackupMirror, true) => Color::Rgb(0xF5, 0xF0, 0xE4),
                (DiskRegionKind::RestoreNode | DiskRegionKind::Tail, false) => {
                    Color::Rgb(0xE1, 0xEA, 0xF1)
                }
                (DiskRegionKind::RestoreNode | DiskRegionKind::Tail, true) => {
                    Color::Rgb(0xF0, 0xF5, 0xF9)
                }
            },
            ThemeMode::Ansi256Dark => {
                if active {
                    Color::Indexed(231)
                } else {
                    Color::Indexed(255)
                }
            }
            ThemeMode::Ansi16 => Color::White,
        };
        let style = Style::default().fg(foreground);
        if active {
            style.add_modifier(Modifier::BOLD)
        } else {
            style
        }
    }

    pub fn disk_region_outline(self, kind: DiskRegionKind, active: bool) -> Style {
        let foreground = if active {
            self.disk_region_active_color(kind)
        } else {
            self.disk_region_color(kind)
        };
        let style = Style::default().fg(foreground);
        if active {
            style.add_modifier(Modifier::BOLD)
        } else {
            style
        }
    }

    pub fn disk_region_half_block(self, kind: DiskRegionKind, active: bool) -> Style {
        Style::default().fg(self.disk_region_fill_color(kind, active))
    }

    pub fn provision_kind_color(self, kind: DiskProvisionKind) -> Color {
        match kind {
            DiskProvisionKind::Mode0 => self.palette.accent,
            DiskProvisionKind::Mode1 => self.palette.violet,
            DiskProvisionKind::Mode2 => self.palette.warning,
            DiskProvisionKind::Mode3 => self.palette.success,
            DiskProvisionKind::Plain => self.palette.partition_plain,
        }
    }

    fn brighter_semantic_color(self, color: Color) -> Color {
        match (self.mode, color) {
            (ThemeMode::TrueColorDark, Color::Rgb(r, g, b)) => {
                let lift =
                    |value: u8| value.saturating_add(((u16::from(255 - value) * 18) / 100) as u8);
                Color::Rgb(lift(r), lift(g), lift(b))
            }
            _ => color,
        }
    }

    pub fn provision_kind(self, kind: DiskProvisionKind) -> Style {
        Style::default().fg(self.provision_kind_color(kind))
    }

    pub fn provision_kind_emphasis(self, kind: DiskProvisionKind) -> Style {
        self.provision_kind(kind).add_modifier(Modifier::BOLD)
    }

    pub fn provision_kind_active(self, kind: DiskProvisionKind) -> Style {
        Style::default()
            .fg(self.brighter_semantic_color(self.provision_kind_color(kind)))
            .add_modifier(Modifier::BOLD)
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
        assert_eq!(palette.violet, Color::Rgb(0x7F, 0xA3, 0xB8));
        assert_eq!(palette.partition_plain, Color::Rgb(0x6F, 0xA8, 0xDC));
        assert_eq!(palette.partition_boot, Color::Rgb(0x63, 0xC7, 0xBE));
        assert_eq!(palette.partition_share, Color::Rgb(0x78, 0xBE, 0x7E));
        assert_eq!(palette.partition_encrypt, Color::Rgb(0xB1, 0x8A, 0xCB));
        assert_eq!(
            palette.partition_compatibility,
            Color::Rgb(0xD0, 0xAA, 0x62)
        );
        assert_eq!(palette.partition_free, Color::Rgb(0x6C, 0x78, 0x84));
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
    fn selection_and_focus_preserve_semantics_and_add_structural_focus() {
        let theme = Theme::truecolor_dark();
        assert_eq!(theme.selection().fg, None);
        assert_eq!(theme.selection().bg, Some(theme.palette.selection));
        assert_ne!(theme.selection().bg, Some(theme.palette.accent));
        assert_ne!(theme.pane_border(false).fg, theme.pane_border(true).fg);
        assert_ne!(theme.pane_border_type(false), theme.pane_border_type(true));
        assert_eq!(theme.pane_title_prefix(false), "");
        assert_eq!(theme.pane_title_prefix(true), "▌ ");
        assert_ne!(theme.card_surface(false).bg, theme.card_surface(true).bg);
        assert_ne!(theme.input().bg, theme.input_focused().bg);
    }

    #[test]
    fn provision_kind_active_keeps_each_semantic_hue_instead_of_turning_white() {
        let theme = Theme::truecolor_dark();
        for kind in [
            DiskProvisionKind::Plain,
            DiskProvisionKind::Mode0,
            DiskProvisionKind::Mode1,
            DiskProvisionKind::Mode2,
            DiskProvisionKind::Mode3,
        ] {
            let normal = theme.provision_kind(kind);
            let active = theme.provision_kind_active(kind);
            assert_eq!(normal.fg, Some(theme.provision_kind_color(kind)));
            assert_ne!(active.fg, normal.fg, "{kind:?} active color must brighten");
            assert_ne!(
                active.fg,
                Some(theme.palette.text_primary),
                "{kind:?} active color must keep its semantic hue"
            );
            assert!(active.add_modifier.contains(Modifier::BOLD));
        }
    }

    #[test]
    fn disk_regions_keep_their_own_hue_when_active() {
        let theme = Theme::truecolor_dark();
        let boot = theme.disk_region_fill(DiskRegionKind::Boot, false);
        let boot_active = theme.disk_region_fill(DiskRegionKind::Boot, true);
        let share = theme.disk_region_fill(DiskRegionKind::Share, false);
        let share_active = theme.disk_region_fill(DiskRegionKind::Share, true);
        let encrypt = theme.disk_region_fill(DiskRegionKind::Encrypt, false);
        let encrypt_active = theme.disk_region_fill(DiskRegionKind::Encrypt, true);

        assert_ne!(boot.bg, boot_active.bg);
        assert_ne!(share.bg, share_active.bg);
        assert_ne!(encrypt.bg, encrypt_active.bg);
        assert_ne!(boot.bg, share.bg);
        assert_ne!(share.bg, encrypt.bg);
        assert_ne!(boot_active.bg, share_active.bg);
        assert_ne!(share_active.bg, encrypt_active.bg);

        for kind in [
            DiskRegionKind::Boot,
            DiskRegionKind::Share,
            DiskRegionKind::Encrypt,
        ] {
            let normal = theme.disk_region_fill(kind, false);
            let normal_outline = theme.disk_region_outline(kind, false);
            let active = theme.disk_region_fill(kind, true);
            let outline = theme.disk_region_outline(kind, true);
            assert_eq!(normal_outline.bg, None);
            assert_eq!(normal_outline.fg, normal.fg);
            assert_eq!(outline.bg, None);
            assert_eq!(outline.fg, active.fg);
            assert_ne!(outline.fg, Some(theme.palette.accent));
        }
    }
}
