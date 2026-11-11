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
    e.type_text("split resize").keys("enter 1");
    assert!(e.app().panes.is_split());
}

#[test]
fn alt_w_shows_the_recent_files_after_the_delay() {
    let mut e = project(0);
    open_all(&mut e);
    e.keys("alt+w");
    e.snapshot("keys_switcher");
    e.keys("3");
    assert_eq!(e.app().doc().name(), "b.js");
    e.assert_screen_lacks("Switch to");
    e.keys("alt+w 3");
    assert_eq!(e.app().doc().name(), "c.js");
    assert_eq!(e.app().docs.len(), 1);
}

#[test]
fn the_switcher_waits_before_popping_up() {
    let mut e = project(60_000);
    e.keys("alt+w");
    e.assert_screen_lacks("Switch to");
    e.assert_screen_contains("SWITCH");
    e.keys("esc");
    e.assert_screen_contains("EDIT");
}

#[test]
fn alt_w_1_focuses_the_file_tree() {
    let mut e = project(0);
    e.keys("alt+w 1");
    assert_eq!(e.app().focus, yate::app::Focus::Tree);
    assert!(e.app().tree_visible);
    e.keys("alt+w 2");
    assert_eq!(e.app().focus, yate::app::Focus::Editor);
}

#[test]
fn repeated_alt_w_cycles_the_recent_files() {
    let mut e = project(0);
    open_all(&mut e);
    e.keys("alt+w alt+w");
    assert_eq!(e.app().doc().name(), "b.js");
    e.keys("alt+w");
    assert_eq!(e.app().doc().name(), "a.js");
    e.keys("alt+w");
    assert_eq!(e.app().doc().name(), "c.js");
    e.keys("alt+shift+w");
    assert_eq!(e.app().doc().name(), "a.js");
    e.keys("x");
    assert_eq!(e.editor_text(), "xlet a = 1;\n");
}

#[test]
fn the_keys_of_tabs_are_not_bound() {
    let mut e = project(0);
    open_all(&mut e);
    e.keys("ctrl+w ctrl+e alt+, alt+.");
    assert_eq!(e.app().doc().name(), "c.js");
    e.assert_screen_lacks("a.js");
}

#[test]
fn a_key_that_is_not_a_target_leaves_the_switcher_and_works() {
    let mut e = project(0);
    e.keys("alt+w ctrl+p");
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
        "alt+s split · alt+p commands · ctrl+p files · alt+w switch · ctrl+h keys",
    );
    e.keys("alt+e");
    e.assert_screen_contains(
        "g grep here · f files here · r rename · ctrl+n new · alt+w switch · ctrl+h keys",
    );
}

#[test]
fn legacy_terminals_send_ctrl_slash_as_ctrl_7() {
    let mut e = project(0);
    e.keys("ctrl+7");
    assert_eq!(e.editor_text(), "// let a = 1;\n");
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
    assert_eq!(e.editor_text(), "  let a = 1;\n");
    e.keys("alt+o");
    assert_eq!(e.editor_text(), "let a = 1;\n");
}

#[test]
fn ctrl_shift_keys_are_not_bound() {
    let mut e = project(0);
    e.keys("ctrl+shift+p ctrl+shift+e ctrl+shift+f ctrl+shift+g");
    e.assert_screen_contains("EDIT");
    assert!(!e.app().tree_visible);
}
