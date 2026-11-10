use yate::testing::Harness;

fn numbered(count: usize, prefix: &str) -> String {
    (1..=count).map(|i| format!("{prefix} {i}\n")).collect()
}

fn project() -> Harness {
    Harness::builder()
        .size(70, 10)
        .file("one.txt", &numbered(60, "one"))
        .file("two.txt", &numbered(60, "two"))
        .home_file("yate.json", r#"{ "options": { "scrollOff": 0 } }"#)
        .open("one.txt")
        .start()
}

#[test]
fn split_shows_two_panes_with_their_own_files() {
    let mut e = project();
    e.keys("ctrl+\\");
    e.keys("ctrl+p").type_text("two.txt").keys("enter");
    e.snapshot("split_two_files");
    e.keys("ctrl+o");
    assert_eq!(e.app().doc().name(), "one.txt");
    e.keys("ctrl+o");
    assert_eq!(e.app().doc().name(), "two.txt");
    e.keys("ctrl+\\");
    assert!(e.app().split.is_none());
}

#[test]
fn the_same_file_in_both_panes_keeps_two_cursors() {
    let mut e = project();
    e.keys("ctrl+\\ ctrl+g").type_text("40\n");
    e.keys("ctrl+o");
    assert_eq!(e.app().doc().cursor_line(), 0);
    e.keys("ctrl+o");
    assert_eq!(e.app().doc().cursor_line(), 39);
    e.snapshot("split_same_file");
}

#[test]
fn scroll_sync_moves_both_panes() {
    let mut e = project();
    e.keys("ctrl+\\");
    e.keys("ctrl+p").type_text("two.txt").keys("enter");
    e.keys("ctrl+j");
    for _ in 0..20 {
        e.keys("down");
    }
    e.snapshot("split_scroll_sync");
    e.keys("ctrl+j");
    e.keys("ctrl+home");
    e.assert_screen_contains("one 14");
    e.assert_screen_contains("two 1 ");
}

#[test]
fn clicking_the_other_pane_focuses_it() {
    let mut e = project();
    e.keys("ctrl+\\");
    e.keys("ctrl+p").type_text("two.txt").keys("enter");
    e.click(10, 3);
    assert_eq!(e.app().doc().name(), "one.txt");
    assert_eq!(e.app().doc().cursor_line(), 2);
}

#[test]
fn closing_the_file_of_the_other_pane_closes_the_split() {
    let mut e = project();
    e.keys("ctrl+\\");
    e.keys("ctrl+p").type_text("two.txt").keys("enter");
    e.keys("ctrl+o ctrl+w");
    assert!(e.app().split.is_none());
    assert_eq!(e.app().doc().name(), "two.txt");
}
