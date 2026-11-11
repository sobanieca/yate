use yate::testing::Harness;

fn project(width: u16) -> Harness {
    Harness::builder()
        .size(width, 8)
        .file("a.js", "let a = 1;\n")
        .file("b.js", "let b = 2;\n")
        .file("c.js", "let c = 3;\n")
        .file("lib/a.js", "export const a = 4;\n")
        .home_file("yate.json", r#"{ "options": { "keyHintDelay": 0 } }"#)
        .open("a.js")
        .start()
}

fn open(e: &mut Harness, file: &str) {
    e.keys("ctrl+p").type_text(file).keys("enter");
}

fn tab_names(e: &Harness) -> Vec<String> {
    let tabs = e.app().tabs.iter();
    tabs.map(|path| e.app().relative(path)).collect()
}

/// The screen cell of the first character of `text` in the tab bar.
fn tab_cell(e: &mut Harness, text: &str) -> (u16, u16) {
    let area = e.app().layout.tabs.expect("a tab bar");
    let line = e.screen().line(area.y);
    let index = line.find(text).expect("the text in the tab bar");
    (line[..index].chars().count() as u16, area.y)
}

#[test]
fn opened_files_get_tabs_right_of_the_current_one() {
    let mut e = project(60);
    open(&mut e, "c.js");
    e.keys("ctrl+o 2");
    open(&mut e, "b.js");
    assert_eq!(tab_names(&e), ["a.js", "b.js", "c.js"]);
    e.snapshot("tabs_bar");
}

#[test]
fn tabs_with_the_same_file_name_show_their_folder() {
    let mut e = project(60);
    open(&mut e, "lib/a.js");
    let root = e.root().file_name().unwrap().to_string_lossy().into_owned();
    e.assert_screen_contains(&format!(" {root}/a.js "))
        .assert_screen_contains(" lib/a.js ");
}

#[test]
fn a_click_on_a_tab_shows_its_file_and_on_its_cross_closes_it() {
    let mut e = project(60);
    open(&mut e, "b.js");
    open(&mut e, "c.js");
    let (x, y) = tab_cell(&mut e, "a.js");
    e.click(x, y);
    assert_eq!(e.app().doc().name(), "a.js");
    let (x, y) = tab_cell(&mut e, "b.js ×");
    e.click(x + 5, y);
    assert_eq!(tab_names(&e), ["a.js", "c.js"]);
    assert_eq!(e.app().doc().name(), "a.js");
    let (x, y) = tab_cell(&mut e, "a.js ×");
    e.click(x + 5, y);
    assert_eq!(tab_names(&e), ["c.js"]);
    assert_eq!(e.app().doc().name(), "c.js");
    assert_eq!(e.app().docs.len(), 1);
}

#[test]
fn closing_the_last_tab_leaves_an_empty_editor_without_a_tab_bar() {
    let mut e = project(60);
    e.keys("ctrl+w x");
    assert!(e.app().tabs.is_empty());
    assert!(e.app().layout.tabs.is_none());
    assert_eq!(e.editor_text(), "");
    e.assert_screen_lacks("a.js");
}

#[test]
fn closing_a_tab_with_unsaved_changes_asks_first() {
    let mut e = project(60);
    open(&mut e, "b.js");
    e.type_text("x");
    e.assert_screen_contains("b.js ●");
    e.keys("ctrl+w x");
    e.assert_screen_contains("b.js has unsaved changes");
    e.keys("esc");
    assert_eq!(tab_names(&e), ["a.js", "b.js"]);
    e.keys("ctrl+w x d");
    assert_eq!(tab_names(&e), ["a.js"]);
    assert_eq!(e.read_file("b.js"), "let b = 2;\n");
}

#[test]
fn closing_the_tabs_not_on_screen_keeps_the_shown_files() {
    let mut e = project(60);
    open(&mut e, "b.js");
    open(&mut e, "c.js");
    e.keys("ctrl+w v");
    open(&mut e, "a.js");
    e.keys("ctrl+w t");
    assert_eq!(tab_names(&e), ["a.js", "c.js"]);
}

#[test]
fn ctrl_o_numbers_the_tabs() {
    let mut e = Harness::builder()
        .size(60, 8)
        .file("a.js", "")
        .file("b.js", "")
        .home_file("yate.json", r#"{ "options": { "keyHintDelay": 60000 } }"#)
        .open("a.js")
        .start();
    open(&mut e, "b.js");
    e.keys("ctrl+o");
    e.snapshot("tabs_numbered");
    e.keys("2");
    assert_eq!(e.app().doc().name(), "a.js");
}

#[test]
fn the_current_tab_stays_in_view_when_the_tabs_do_not_fit() {
    let mut e = Harness::builder()
        .size(30, 5)
        .file("first.js", "")
        .file("second.js", "")
        .file("third.js", "")
        .file("fourth.js", "")
        .open("first.js")
        .start();
    for name in ["second.js", "third.js", "fourth.js"] {
        open(&mut e, name);
    }
    e.snapshot("tabs_overflow");
}

#[test]
fn renaming_and_deleting_a_file_follow_in_the_tabs() {
    let mut e = project(60);
    open(&mut e, "b.js");
    e.keys("ctrl+t r r ctrl+u").type_text("d.js").keys("enter");
    assert_eq!(tab_names(&e), ["a.js", "d.js"]);
    e.keys("d left enter");
    assert!(!e.file_exists("d.js"));
    assert_eq!(tab_names(&e), ["a.js"]);
}
