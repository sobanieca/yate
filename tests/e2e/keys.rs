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
fn ctrl_h_lists_the_commands_with_their_keys_and_runs_them() {
    let mut e = project(500);
    e.keys("ctrl+h");
    e.snapshot("keys_help");
    e.type_text("split side").keys("enter");
    assert!(e.app().panes.is_split());
}

#[test]
fn a_menu_waits_before_popping_up_and_takes_keys_at_once() {
    let mut e = project(60_000);
    e.keys("ctrl+t");
    e.assert_screen_lacks("Reveal current file");
    e.assert_screen_contains("MENU");
    e.assert_screen_contains("File tree: t r f l");
    e.keys("t");
    assert!(e.app().tree_visible);
    e.assert_screen_contains("TREE");
}

#[test]
fn ctrl_t_groups_the_file_tree_commands() {
    let mut e = project(0);
    e.keys("ctrl+t");
    e.snapshot("keys_tree_menu");
    e.keys("ctrl+t");
    assert!(e.app().tree_visible);
    assert_eq!(e.app().focus, yate::app::Focus::Tree);
    e.keys("ctrl+t ctrl+t");
    assert!(!e.app().tree_visible);
    e.keys("ctrl+t f");
    assert_eq!(e.app().focus, yate::app::Focus::Tree);
    e.keys("esc ctrl+t l");
    assert!(e.app().tree.locked);
}

#[test]
fn a_menu_key_bound_to_another_key_still_runs_its_command_twice() {
    let mut e = Harness::builder()
        .file("a.js", "let a = 1;\n")
        .home_file(
            "yate.json",
            r#"{ "keys": { "global": { "ctrl+e": "tree.menu", "ctrl+t": "" } } }"#,
        )
        .open("a.js")
        .start();
    e.keys("ctrl+t ctrl+t");
    assert!(!e.app().tree_visible);
    e.keys("ctrl+e ctrl+e");
    assert!(e.app().tree_visible);
}

#[test]
fn a_click_chooses_from_a_menu() {
    let mut e = project(0);
    e.keys("ctrl+t");
    let row = e
        .screen()
        .lines()
        .iter()
        .position(|line| line.contains("Toggle"));
    e.click(30, row.unwrap() as u16);
    assert!(e.app().tree_visible);
}

#[test]
fn ctrl_o_shows_the_tabs_after_the_delay() {
    let mut e = project(0);
    open_all(&mut e);
    e.keys("ctrl+o");
    e.snapshot("keys_switcher");
    e.keys("3");
    assert_eq!(e.app().doc().name(), "b.js");
    e.assert_screen_lacks("Switch to");
    e.keys("ctrl+o 4");
    assert_eq!(e.app().doc().name(), "c.js");
    assert_eq!(e.app().docs.len(), 1);
}

#[test]
fn the_switcher_waits_before_popping_up() {
    let mut e = project(60_000);
    e.keys("ctrl+o");
    e.assert_screen_lacks("Switch to");
    e.assert_screen_contains("SWITCH");
    e.keys("esc");
    e.assert_screen_contains("EDIT");
}

#[test]
fn ctrl_o_1_focuses_the_file_tree() {
    let mut e = project(0);
    e.keys("ctrl+o 1");
    assert_eq!(e.app().focus, yate::app::Focus::Tree);
    assert!(e.app().tree_visible);
    e.keys("ctrl+o 2");
    assert_eq!(e.app().focus, yate::app::Focus::Editor);
}

#[test]
fn repeated_ctrl_o_walks_through_the_tabs() {
    let mut e = project(0);
    open_all(&mut e);
    e.keys("ctrl+o ctrl+o");
    assert_eq!(e.app().doc().name(), "a.js");
    e.keys("ctrl+o");
    assert_eq!(e.app().doc().name(), "b.js");
    e.keys("ctrl+o");
    assert_eq!(e.app().doc().name(), "c.js");
    e.command("switch.previous");
    assert_eq!(e.app().doc().name(), "b.js");
    e.keys("x");
    assert_eq!(e.editor_text(), "xlet b = 2;\n");
}

#[test]
fn alt_and_ctrl_tab_keys_switch_no_tab() {
    let mut e = project(0);
    open_all(&mut e);
    e.keys("ctrl+e alt+, alt+. ctrl+tab");
    assert_eq!(e.app().doc().name(), "c.js");
}

#[test]
fn a_key_that_is_not_a_target_leaves_the_switcher_and_works() {
    let mut e = project(0);
    e.keys("ctrl+o ctrl+p");
    e.assert_screen_contains("Find file");
}

#[test]
fn the_status_line_hints_at_keys() {
    let mut e = Harness::builder()
        .size(130, 6)
        .file("a.js", "let a = 1;\n")
        .open("a.js")
        .start();
    e.assert_screen_contains("ctrl+w panes · ctrl+p files · ctrl+o switch · ctrl+h commands");
    e.keys("ctrl+t t");
    e.assert_screen_contains(
        "f files here · r rename · ctrl+n new · ctrl+o switch · ctrl+h commands",
    );
}

#[test]
fn legacy_terminals_send_ctrl_slash_as_ctrl_7() {
    let mut e = project(0);
    e.keys("ctrl+7");
    assert_eq!(e.editor_text(), "// let a = 1;\n");
}

#[test]
fn alt_keys_and_the_tmux_prefix_are_not_bound() {
    let mut e = project(0);
    e.keys("ctrl+b alt+e alt+w alt+s alt+m alt+p alt+f alt+up alt+i alt+left");
    e.assert_screen_contains("EDIT");
    assert!(!e.app().tree_visible);
    assert_eq!(e.editor_text(), "let a = 1;\n");
}

#[test]
fn ctrl_shift_keys_are_not_bound() {
    let mut e = project(0);
    e.keys("ctrl+shift+p ctrl+shift+e ctrl+shift+f ctrl+shift+g");
    e.assert_screen_contains("EDIT");
    assert!(!e.app().tree_visible);
}
