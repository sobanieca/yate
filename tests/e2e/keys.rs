use yate::testing::Harness;

fn project(delay_ms: u64) -> Harness {
    Harness::builder()
        .size(70, 12)
        .file("a.js", "let a = 1;\n")
        .file("b.js", "let b = 2;\n")
        .file("c.js", "let c = 3;\n")
        .home_file(
            "yate.json",
            &format!(r#"{{ "options": {{ "keyHintDelay": {delay_ms} }} }}"#),
        )
        .open("a.js")
        .start()
}

fn open_all(e: &mut Harness) {
    for name in ["b.js", "c.js"] {
        e.keys("ctrl+p").type_text(name).keys("enter");
    }
}

#[test]
fn ctrl_h_lists_the_key_bindings_and_runs_them() {
    let mut e = project(500);
    e.keys("ctrl+h");
    e.snapshot("keys_help");
    e.type_text("split editor").keys("enter");
    assert!(e.app().split.is_some());
}

#[test]
fn alt_tab_shows_numbered_targets_after_the_delay() {
    let mut e = project(0);
    open_all(&mut e);
    e.keys("alt+tab");
    e.snapshot("keys_switcher");
    e.keys("2");
    assert_eq!(e.app().doc().name(), "a.js");
    e.assert_screen_lacks("Switch to");
}

#[test]
fn the_switcher_waits_before_popping_up() {
    let mut e = project(60_000);
    e.keys("alt+tab");
    e.assert_screen_lacks("Switch to");
    e.assert_screen_contains("SWITCH");
    e.keys("esc");
    e.assert_screen_contains("EDIT");
}

#[test]
fn alt_tab_1_focuses_the_file_tree() {
    let mut e = project(0);
    e.keys("alt+tab 1");
    assert_eq!(e.app().focus, yate::app::Focus::Tree);
    assert!(e.app().tree_visible);
    e.keys("alt+w 2");
    assert_eq!(e.app().focus, yate::app::Focus::Editor);
}

#[test]
fn repeated_alt_tab_cycles_the_open_files() {
    let mut e = project(0);
    open_all(&mut e);
    e.keys("alt+tab 2");
    assert_eq!(e.app().doc().name(), "a.js");
    e.keys("alt+tab alt+tab");
    assert_eq!(e.app().doc().name(), "b.js");
    e.keys("alt+tab");
    assert_eq!(e.app().doc().name(), "c.js");
    e.keys("alt+shift+w");
    assert_eq!(e.app().doc().name(), "b.js");
    e.keys("x");
    assert_eq!(e.buffer_text(), "xlet b = 2;\n");
}

#[test]
fn a_key_that_is_not_a_target_leaves_the_switcher_and_works() {
    let mut e = project(0);
    e.keys("alt+tab ctrl+p");
    e.assert_screen_contains("Find file");
}

#[test]
fn the_status_line_hints_at_keys() {
    let mut e = Harness::builder()
        .size(130, 6)
        .file("a.js", "let a = 1;\n")
        .open("a.js")
        .start();
    e.assert_screen_contains(
        "ctrl+\\ split · alt+p commands · ctrl+p files · alt+w switch · ctrl+h keys",
    );
    e.keys("alt+e");
    e.assert_screen_contains(
        "g grep here · f files here · r rename · a new · alt+w switch · ctrl+h keys",
    );
}

#[test]
fn legacy_terminals_send_ctrl_backslash_as_ctrl_4() {
    let mut e = project(0);
    e.keys("ctrl+4");
    assert!(e.app().split.is_some());
}

#[test]
fn tmux_and_termux_friendly_keys() {
    let mut e = project(0);
    e.keys("alt+e");
    assert!(e.app().tree_visible);
    e.keys("alt+e");
    assert!(!e.app().tree_visible);
    e.keys("ctrl+b");
    assert!(!e.app().tree_visible);
    e.keys("alt+i");
    assert_eq!(e.buffer_text(), "  let a = 1;\n");
    e.keys("alt+o");
    assert_eq!(e.buffer_text(), "let a = 1;\n");
}
