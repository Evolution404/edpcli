use std::fs;
use std::path::{Path, PathBuf};

use edpcli::tui::{
    render,
    state::AppState,
    theme::{Theme, ThemeMode},
};
use ratatui::{backend::TestBackend, style::Color, Terminal};

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
        assert!(
            !source.contains("Color::"),
            "{} bypasses centralized TUI Theme with a direct Color::*",
            path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
                .unwrap_or(&path)
                .display()
        );
    }
}

#[test]
fn selection_is_not_the_old_black_on_cyan_highlight() {
    let style = Theme::truecolor_dark().selection();
    assert_eq!(style.fg, Some(Color::Rgb(0xD7, 0xDC, 0xE2)));
    assert_eq!(style.bg, Some(Color::Rgb(0x26, 0x34, 0x42)));
    assert_ne!(style.fg, Some(Color::Black));
    assert_ne!(style.bg, Some(Color::Cyan));
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
