use yate::testing::Harness;

const APP_JS: &str = "const colors = {\n  bg: \"#24283b\",\n};\n\nfunction greet(name) {\n  return `hello ${name}`;\n}\n";

fn app_js() -> Harness {
    Harness::builder()
        .size(60, 12)
        .file("app.js", APP_JS)
        .open("app.js")
        .start()
}

#[test]
fn welcome_screen_when_started_in_a_folder() {
    Harness::builder()
        .size(70, 16)
        .file("src/main.rs", "fn main() {}\n")
        .file("README.md", "# demo\n")
        .start()
        .snapshot("editing_welcome");
}

#[test]
fn typing_closes_brackets_and_indents_blocks() {
    let mut e = app_js();
    e.keys("ctrl+end")
        .type_text("function add(a, b) {\nreturn a + b");
    assert!(
        e.buffer_text()
            .ends_with("function add(a, b) {\n  return a + b\n}")
    );
    e.snapshot("editing_auto_pairs_and_indent");
}

#[test]
fn selection_and_syntax_colors() {
    let mut e = app_js();
    e.keys("ctrl+home down shift+down shift+end");
    e.snapshot_styles("editing_selection_styles");
}

#[test]
fn comment_toggle_then_undo_and_redo() {
    let mut e = app_js();
    e.keys("ctrl+home shift+down shift+down ctrl+/");
    assert!(e.buffer_text().starts_with("// const colors = {\n//   bg:"));
    e.keys("ctrl+z");
    assert_eq!(e.buffer_text(), APP_JS);
    e.keys("ctrl+y");
    assert!(e.buffer_text().starts_with("// const colors"));
}

#[test]
fn cut_line_without_selection_and_paste_it_above() {
    let mut e = app_js();
    e.keys("ctrl+home ctrl+x");
    assert_eq!(e.clipboard(), "const colors = {\n");
    e.keys("down ctrl+v");
    assert!(
        e.buffer_text()
            .starts_with("  bg: \"#24283b\",\nconst colors = {\n};")
    );
    e.keys("up alt+up");
    assert!(e.buffer_text().starts_with("const colors = {\n  bg:"));
}

#[test]
fn save_writes_the_file_and_reports_it() {
    let mut e = app_js();
    e.keys("ctrl+end")
        .type_text("export default greet;")
        .keys("ctrl+s");
    assert!(e.read_file("app.js").ends_with("}\nexport default greet;"));
    e.snapshot("editing_saved");
}

#[test]
fn quitting_with_unsaved_changes_asks_first() {
    let mut e = app_js();
    e.type_text("x").keys("ctrl+q");
    e.snapshot("editing_quit_prompt");
    e.keys("n");
    assert!(!e.app().quit);
    e.keys("ctrl+q y");
    assert!(e.app().quit);
}

#[test]
fn an_untitled_buffer_is_saved_as_a_new_file() {
    let mut e = Harness::builder().size(60, 10).start();
    e.keys("esc").type_text("notes").keys("ctrl+s");
    e.assert_screen_contains("Save as");
    e.type_text("docs/notes.md").keys("enter");
    assert_eq!(e.read_file("docs/notes.md"), "notes");
    e.assert_screen_contains("Saved docs/notes.md");
}

#[test]
fn files_changed_on_disk_are_reloaded() {
    let mut e = app_js();
    e.write_file("app.js", "const changed = true;\n")
        .focus_gained();
    assert_eq!(e.buffer_text(), "const changed = true;\n");
    e.snapshot("editing_reloaded_from_disk");
}

#[test]
fn mouse_click_moves_the_cursor_and_drag_selects() {
    let mut e = app_js();
    e.click(10, 6);
    let cursor = e.screen().cursor;
    assert_eq!(cursor, Some((10, 6)));
    e.drag((5, 1), (10, 1));
    assert_eq!(e.app().doc().selected_text(), "const");
    e.double_click(17, 5);
    assert_eq!(e.app().doc().selected_text(), "greet");
}

#[test]
fn wide_characters_and_tabs_line_up() {
    Harness::builder()
        .size(40, 6)
        .file("notes.txt", "名前: 東京\n\tindented\nemoji 🎉 end\n")
        .open("notes.txt")
        .start()
        .snapshot("editing_wide_characters");
}

#[test]
fn the_cursor_line_stays_centered() {
    let text: String = (1..=100).map(|i| format!("line {i}\n")).collect();
    let mut e = Harness::builder()
        .size(40, 10)
        .file("long.txt", &text)
        .open("long.txt:50")
        .start();
    e.snapshot("editing_centered_cursor");
    e.keys("ctrl+end");
    e.assert_screen_contains("line 100");
}

#[test]
fn ctrl_arrows_jump_by_characters_and_lines() {
    let line = "abcdefghijklmnopqrstuvwxyz\n";
    let mut e = Harness::builder()
        .size(60, 10)
        .file("a.txt", &line.repeat(30))
        .open("a.txt")
        .start();
    e.keys("ctrl+right");
    assert_eq!(e.app().doc().selection.head, 10);
    e.keys("ctrl+right ctrl+right");
    assert_eq!(e.app().doc().selection.head, 26);
    e.keys("ctrl+right");
    assert_eq!(e.app().doc().selection.head, 27);
    e.keys("ctrl+left");
    assert_eq!(e.app().doc().selection.head, 26);
    e.keys("ctrl+shift+left");
    assert_eq!(e.app().doc().selected_text(), "qrstuvwxyz");
    e.keys("ctrl+down");
    assert_eq!(e.app().doc().cursor_line(), 5);

    let mut e = Harness::builder()
        .size(60, 10)
        .file("a.txt", &line.repeat(30))
        .home_file(
            "yate.json",
            r#"{ "options": { "jumpChars": 3, "jumpLines": 12 } }"#,
        )
        .open("a.txt")
        .start();
    e.keys("ctrl+right ctrl+down");
    assert_eq!(e.app().doc().cursor_line(), 12);
    assert_eq!(e.app().doc().line_col(e.app().doc().selection.head).1, 3);
}

#[test]
fn ctrl_backspace_deletes_one_character() {
    let mut e = app_js();
    e.keys("end ctrl+backspace");
    assert!(e.buffer_text().starts_with("const colors = \n"));
}
