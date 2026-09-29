use std::fs;
use std::path::{Path, PathBuf};

use edpcli::application::identity::IdentityMatchLevel;
use edpcli::provision::DiskProvisionKind;
use edpcli::tui::disk_layout::DiskRegionKind;
use edpcli::tui::{
    render,
    state::AppState,
    theme::{Theme, ThemeMode},
};
use ratatui::{
    backend::TestBackend,
    layout::Constraint,
    style::{Color, Style},
    widgets::Row,
    Terminal,
};

fn rust_files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in
        fs::read_dir(root).unwrap_or_else(|error| panic!("read {}: {error}", root.display()))
    {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn chapter_ten_truecolor_palette_is_exact() {
    let palette = Theme::truecolor_dark().palette();
    assert_eq!(palette.background, Color::Rgb(0x11, 0x16, 0x1C));
    assert_eq!(palette.surface, Color::Rgb(0x17, 0x1D, 0x24));
    assert_eq!(palette.surface_focus, Color::Rgb(0x10, 0x23, 0x30));
    assert_eq!(palette.surface_active, Color::Rgb(0x1D, 0x25, 0x30));
    assert_eq!(palette.selection, Color::Rgb(0x26, 0x34, 0x42));
    assert_eq!(palette.border, Color::Rgb(0x30, 0x39, 0x45));
    assert_eq!(palette.border_focus, Color::Rgb(0x58, 0x75, 0x8D));
    assert_eq!(palette.text_primary, Color::Rgb(0xC4, 0xCB, 0xD2));
    assert_eq!(palette.text_body, Color::Rgb(0xA7, 0xB1, 0xBA));
    assert_eq!(palette.text_secondary, Color::Rgb(0x89, 0x96, 0xA2));
    assert_eq!(palette.text_muted, Color::Rgb(0x66, 0x73, 0x7E));
    assert_eq!(palette.table_text, Color::Rgb(0xA7, 0xB1, 0xBA));
    assert_eq!(palette.table_text_muted, Color::Rgb(0x7C, 0x89, 0x94));
    assert_eq!(palette.table_text_active, Color::Rgb(0xC0, 0xC8, 0xCF));
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
fn disk_region_truecolor_fills_are_high_contrast_and_semantically_distinct() {
    let theme = Theme::truecolor_dark();
    let cases = [
        (
            DiskRegionKind::Protocol,
            Color::Rgb(0x2C, 0x62, 0x70),
            Color::Rgb(0x39, 0x7A, 0x89),
            Color::Rgb(0xB0, 0xD3, 0xDA),
        ),
        (
            DiskRegionKind::Free,
            Color::Rgb(0x50, 0x58, 0x61),
            Color::Rgb(0x68, 0x72, 0x7C),
            Color::Rgb(0xCB, 0xD2, 0xD8),
        ),
        (
            DiskRegionKind::Unknown,
            Color::Rgb(0x55, 0x4C, 0x49),
            Color::Rgb(0x6B, 0x5D, 0x58),
            Color::Rgb(0xD2, 0xC5, 0xC0),
        ),
        (
            DiskRegionKind::Plain,
            Color::Rgb(0x34, 0x5B, 0x72),
            Color::Rgb(0x44, 0x73, 0x8C),
            Color::Rgb(0xB7, 0xD2, 0xE0),
        ),
        (
            DiskRegionKind::Boot,
            Color::Rgb(0x35, 0x6B, 0x66),
            Color::Rgb(0x43, 0x83, 0x7D),
            Color::Rgb(0xB7, 0xD8, 0xD3),
        ),
        (
            DiskRegionKind::Share,
            Color::Rgb(0x3A, 0x76, 0x58),
            Color::Rgb(0x49, 0x8D, 0x6B),
            Color::Rgb(0xBE, 0xDB, 0xC6),
        ),
        (
            DiskRegionKind::Encrypt,
            Color::Rgb(0x67, 0x50, 0x78),
            Color::Rgb(0x7D, 0x63, 0x91),
            Color::Rgb(0xD1, 0xC2, 0xDB),
        ),
        (
            DiskRegionKind::Compatibility,
            Color::Rgb(0x5B, 0x50, 0x3B),
            Color::Rgb(0x74, 0x66, 0x4A),
            Color::Rgb(0xD8, 0xC9, 0xA6),
        ),
        (
            DiskRegionKind::Lce,
            Color::Rgb(0x35, 0x5D, 0x68),
            Color::Rgb(0x46, 0x74, 0x80),
            Color::Rgb(0xB9, 0xD0, 0xD6),
        ),
        (
            DiskRegionKind::BackupMirror,
            Color::Rgb(0x5D, 0x50, 0x3A),
            Color::Rgb(0x76, 0x66, 0x4A),
            Color::Rgb(0xD7, 0xC9, 0xA8),
        ),
        (
            DiskRegionKind::Tail,
            Color::Rgb(0x38, 0x54, 0x6D),
            Color::Rgb(0x49, 0x6C, 0x88),
            Color::Rgb(0xBC, 0xD0, 0xDF),
        ),
    ];

    for (kind, normal_bg, active_bg, active_fg) in cases {
        let normal = theme.disk_region_fill(kind, false);
        let active = theme.disk_region_fill(kind, true);
        assert_eq!(normal.bg, Some(normal_bg), "normal fill for {kind:?}");
        assert_eq!(active.bg, Some(active_bg), "active fill for {kind:?}");
        assert_eq!(active.fg, Some(active_fg), "active text for {kind:?}");
        assert_ne!(
            normal_bg,
            theme.palette().background,
            "{kind:?} must stand out from the page"
        );
        assert_ne!(
            normal_bg, active_bg,
            "{kind:?} active fill must visibly change"
        );
        let tree_active = theme.disk_region_tree(kind, true);
        assert_eq!(tree_active.fg, Some(active_fg));
        assert_eq!(
            tree_active.bg, None,
            "tree selection must not paint a background for {kind:?}"
        );
        assert!(tree_active
            .add_modifier
            .contains(ratatui::style::Modifier::BOLD));
    }
}

#[test]
fn disk_region_content_text_is_light_and_high_contrast_on_semantic_fills() {
    let theme = Theme::truecolor_dark();
    let share = theme.disk_region_content_text(DiskRegionKind::Share, false);
    let share_active = theme.disk_region_content_text(DiskRegionKind::Share, true);
    let encrypt = theme.disk_region_content_text(DiskRegionKind::Encrypt, false);
    let free = theme.disk_region_content_text(DiskRegionKind::Free, false);

    assert_eq!(share.fg, Some(Color::Rgb(0xE3, 0xEE, 0xE6)));
    assert_eq!(share_active.fg, Some(Color::Rgb(0xF1, 0xF8, 0xF3)));
    assert_eq!(encrypt.fg, Some(Color::Rgb(0xEC, 0xE5, 0xF0)));
    assert_eq!(free.fg, Some(Color::Rgb(0xE1, 0xE5, 0xE9)));
    assert_eq!(share.bg, None);
    assert!(share_active
        .add_modifier
        .contains(ratatui::style::Modifier::BOLD));
}

#[test]
fn generic_secondary_accent_is_steel_blue_and_encrypt_keeps_purple_semantics() {
    let theme = Theme::truecolor_dark();
    assert_eq!(
        theme.secondary_accent().fg,
        Some(Color::Rgb(0x7F, 0xA3, 0xB8))
    );
    assert_eq!(
        theme.disk_region(DiskRegionKind::Encrypt).fg,
        Some(theme.palette().partition_encrypt)
    );
    assert_ne!(
        theme.secondary_accent().fg,
        theme.disk_region(DiskRegionKind::Encrypt).fg
    );
}

#[test]
fn theme_detection_keeps_all_three_color_depths() {
    assert_eq!(
        Theme::from_capabilities(None, Some("24bit"), Some("xterm-256color")).mode(),
        ThemeMode::TrueColorDark
    );
    assert_eq!(
        Theme::from_capabilities(None, None, Some("screen-256color")).mode(),
        ThemeMode::Ansi256Dark
    );
    assert_eq!(
        Theme::from_capabilities(None, None, Some("vt100")).mode(),
        ThemeMode::Ansi16
    );
}

#[test]
fn tui_workspace_renderers_do_not_define_private_colors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui");
    let theme = root.join("theme.rs");
    let mut files = Vec::new();
    rust_files(&root, &mut files);

    for path in files {
        if path == theme {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        for forbidden in ["Color::", ".fg(", ".bg(", "Modifier::REVERSED"] {
            assert!(
                !source.contains(forbidden),
                "{} bypasses centralized TUI Theme with {forbidden}",
                path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
                    .unwrap_or(&path)
                    .display()
            );
        }
    }
}

#[test]
fn selection_overlay_never_overwrites_semantic_foreground() {
    let style = Theme::truecolor_dark().selection();
    assert_eq!(style.fg, None);
    assert_eq!(style.bg, Some(Color::Rgb(0x26, 0x34, 0x42)));
    assert_ne!(style.bg, Some(Color::Cyan));
}

#[test]
fn neutral_table_columns_brighten_without_becoming_accent_blue() {
    let theme = Theme::truecolor_dark();
    let palette = theme.palette();

    let ordinary = theme.table_cell(Style::default(), false, true);
    assert_eq!(ordinary.fg, Some(palette.table_text));
    assert_ne!(ordinary.fg, Some(palette.text_primary));

    let ordinary_active = theme.table_cell(Style::default(), true, true);
    assert_eq!(ordinary_active.fg, Some(palette.table_text_active));
    assert_ne!(ordinary_active.fg, Some(palette.accent));

    let muted = theme.table_text_muted();
    let muted_active = theme.table_cell(muted, true, true);
    assert_eq!(muted_active.fg, Some(palette.table_text));
    assert_ne!(muted_active.fg, Some(palette.accent));

    let semantic = theme.provision_kind(DiskProvisionKind::Mode2);
    let semantic_active = theme.table_cell(semantic, true, true);
    assert_ne!(semantic_active.fg, semantic.fg);
    assert_ne!(semantic_active.fg, Some(palette.accent));
}

#[test]
fn shared_surfaces_default_to_soft_body_text_not_heading_white() {
    let theme = Theme::truecolor_dark();
    let palette = theme.palette();

    assert_eq!(theme.background().fg, Some(palette.text_body));
    assert_eq!(theme.canvas().fg, Some(palette.text_body));
    assert_eq!(theme.raised_surface().fg, Some(palette.text_body));
    assert_eq!(theme.surface().fg, Some(palette.text_body));
    assert_eq!(theme.pane_surface(false).fg, Some(palette.text_body));
    assert_eq!(theme.pane_surface(true).fg, Some(palette.text_body));
    assert_eq!(theme.input().fg, Some(palette.text_body));
    assert_eq!(theme.text().fg, Some(palette.text_body));
    assert_eq!(theme.heading_text().fg, Some(palette.text_primary));
    assert_ne!(palette.text_body, palette.text_primary);
    assert_ne!(palette.text_body, palette.accent);
}

#[test]
fn restore_match_levels_escalate_from_green_to_red() {
    let theme = Theme::truecolor_dark();

    assert_eq!(
        theme.identity_match_level(IdentityMatchLevel::Strong).fg,
        theme.success().fg
    );
    assert_eq!(
        theme.identity_match_level(IdentityMatchLevel::Medium).fg,
        theme.secondary_accent().fg
    );
    assert_eq!(
        theme.identity_match_level(IdentityMatchLevel::Weak).fg,
        theme.warning().fg
    );
    assert_eq!(
        theme.identity_match_level(IdentityMatchLevel::Conflict).fg,
        theme.danger().fg
    );
    assert_eq!(
        theme.identity_match_level(IdentityMatchLevel::Unknown).fg,
        theme.muted().fg
    );
}

#[test]
fn workspace_overview_uses_shared_inactive_pane_border() {
    use edpcli::tui::ui::{workspace_overview, OverviewMetric, OverviewSearch};

    let theme = edpcli::tui::theme::current();
    let mut terminal = Terminal::new(TestBackend::new(100, 5)).unwrap();
    terminal
        .draw(|frame| {
            workspace_overview(
                frame,
                frame.area(),
                "设备概览",
                &[OverviewMetric::new("总计", 1, theme.table_text())],
                &OverviewSearch {
                    text: String::new(),
                    active: false,
                    filtered: false,
                },
            );
        })
        .unwrap();
    assert_eq!(
        terminal.backend().buffer()[(0, 1)].style().fg,
        Some(theme.palette().border_subtle),
        "overview must use the same subtle border as any inactive pane"
    );
}

#[test]
fn provision_kind_palette_has_one_base_and_active_style_per_mode() {
    let theme = Theme::truecolor_dark();
    let mut base_colors = Vec::new();
    for kind in [
        DiskProvisionKind::Plain,
        DiskProvisionKind::Mode0,
        DiskProvisionKind::Mode1,
        DiskProvisionKind::Mode2,
        DiskProvisionKind::Mode3,
    ] {
        let base = theme.provision_kind(kind);
        let active = theme.provision_kind_active(kind);
        assert_eq!(base.fg, Some(theme.provision_kind_color(kind)));
        assert_ne!(
            active.fg, base.fg,
            "{kind:?} must visibly brighten when active"
        );
        assert_ne!(
            active.fg,
            Some(theme.palette().text_primary),
            "{kind:?} active state must not turn white"
        );
        assert!(active.add_modifier.contains(ratatui::style::Modifier::BOLD));
        base_colors.push(base.fg);
    }
    base_colors.sort_by_key(|color| format!("{color:?}"));
    base_colors.dedup();
    assert_eq!(
        base_colors.len(),
        5,
        "all five disk kinds need distinct semantic colors"
    );
}

#[test]
fn pane_focus_has_color_shape_title_and_surface_signals() {
    let theme = Theme::truecolor_dark();
    assert_ne!(theme.pane_border(false).fg, theme.pane_border(true).fg);
    assert_ne!(theme.pane_border_type(false), theme.pane_border_type(true));
    assert_eq!(theme.pane_title_prefix(false), "");
    assert_eq!(theme.pane_title_prefix(true), "▌ ");
    assert_ne!(theme.pane_surface(false).bg, theme.pane_surface(true).bg);
    assert_eq!(
        theme.pane_surface(true).bg,
        Some(theme.palette().surface_focus)
    );
    assert_ne!(
        theme.pane_surface(true).bg,
        Some(theme.palette().surface_active)
    );
    assert_ne!(theme.pane_surface(true).bg, theme.selection().bg);
}

#[test]
fn fallback_card_focus_never_collides_with_selection_surface() {
    let ansi256 = Theme::ansi256_dark();
    assert_ne!(
        ansi256.pane_surface(false).bg,
        ansi256.pane_surface(true).bg
    );
    assert_ne!(ansi256.pane_surface(true).bg, ansi256.selection().bg);

    let ansi16 = Theme::ansi16();
    assert_eq!(ansi16.pane_surface(false).bg, ansi16.pane_surface(true).bg);
    assert_ne!(ansi16.pane_surface(true).bg, ansi16.selection().bg);
}

#[test]
fn shared_data_table_applies_the_same_focused_pane_surface_as_cards() {
    fn rendered_body_background(focused: bool) -> Color {
        let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        terminal
            .draw(|frame| {
                let table = edpcli::tui::ui::data_table(
                    "测试表格",
                    Row::new(["标题"]),
                    [Row::new(["值"])],
                    [Constraint::Min(1)],
                    focused,
                );
                frame.render_widget(table, frame.area());
            })
            .unwrap();
        terminal.backend().buffer()[(20, 6)]
            .style()
            .bg
            .expect("table pane surface background")
    }

    let palette = edpcli::tui::theme::current().palette();
    assert_eq!(rendered_body_background(false), palette.surface_raised);
    assert_eq!(rendered_body_background(true), palette.surface_focus);
    assert_ne!(palette.surface_focus, palette.selection);
}

#[test]
fn root_canvas_uses_theme_background_at_supported_sizes() {
    let state = AppState::new();
    let background = edpcli::tui::theme::current().palette().background;
    for (width, height) in [(40, 10), (60, 18), (80, 24), (120, 36)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal.draw(|frame| render::draw(frame, &state)).unwrap();
        assert_eq!(
            terminal.backend().buffer()[(0, 0)].style().bg,
            Some(background),
            "root background at {width}x{height}"
        );
    }
}
