use edpcli::ui::sanitize_terminal_text;

#[test]
fn terminal_text_sanitizer_neutralizes_control_sequences_without_losing_cjk() {
    let input = "江苏\u{1b}[2J\r\n输电\t中心\u{7}";
    let safe = sanitize_terminal_text(input);
    assert_eq!(safe, "江苏␛[2J␍␊输电⇥中心␇");
    assert!(!safe.contains('\u{1b}'));
    assert!(!safe.contains('\n'));
    assert!(!safe.contains('\r'));
    assert!(!safe.contains('\t'));
}

#[test]
fn tui_render_routes_untrusted_device_metadata_through_terminal_sanitizer() {
    let source = include_str!("../src/tui/render.rs");
    assert!(
        source.contains("sanitize_terminal_text"),
        "TUI must sanitize disk/backup/inspect text before ratatui renders it"
    );
}

#[test]
fn tui_background_workers_never_write_directly_to_terminal() {
    let source = include_str!("../src/tui/task.rs");
    for forbidden in ["println!", "eprintln!", "print!", "eprint!"] {
        assert!(
            !source.contains(forbidden),
            "TUI worker must report through TaskHub, found direct terminal macro {forbidden}"
        );
    }
}
