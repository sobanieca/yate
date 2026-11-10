use yate::testing::Harness;

const TEXT: &str =
    "let total = 0;\nfor (const item of items) {\n  total += item.price;\n}\nconsole.log(total);\n";

fn editor() -> Harness {
    Harness::builder()
        .size(70, 10)
        .file("cart.js", TEXT)
        .open("cart.js")
        .start()
}

#[test]
fn find_highlights_matches_and_counts_them() {
    let mut e = editor();
    e.keys("ctrl+f").type_text("total");
    e.snapshot_styles("find_matches");
    e.keys("enter enter");
    assert_eq!(e.app().doc().cursor_line(), 4);
    e.keys("esc");
    e.assert_screen_lacks("FIND ");
}

#[test]
fn replace_one_then_all() {
    let mut e = editor();
    e.keys("ctrl+r")
        .type_text("total")
        .keys("tab")
        .type_text("sum")
        .keys("enter");
    assert!(e.buffer_text().starts_with("let sum = 0;"));
    e.snapshot("find_replace_bar");
    e.keys("alt+a");
    assert!(!e.buffer_text().contains("total"));
    e.assert_screen_contains("Replaced 2 matches");
}

#[test]
fn invalid_regex_is_shown_in_red_without_matches() {
    let mut e = editor();
    e.keys("ctrl+f alt+r").type_text("(item");
    e.assert_screen_contains("no matches");
}

#[test]
fn go_to_line() {
    let mut e = editor();
    e.keys("ctrl+g").type_text("3").keys("enter");
    assert_eq!(e.app().doc().cursor_line(), 2);
    assert_eq!(e.screen().cursor, Some((7, 3)));
}

#[test]
fn every_match_is_highlighted_while_finding_and_none_after() {
    let text: String = (1..=40)
        .map(|i| format!("line {i}: needle and Needle\n"))
        .collect();
    let mut e = Harness::builder()
        .size(60, 10)
        .file("long.txt", &text)
        .open("long.txt")
        .start();
    e.keys("ctrl+f").type_text("needle");
    e.assert_screen_contains("1 of 80");
    e.keys("down down down");
    e.assert_screen_contains("4 of 80");
    assert_eq!(e.app().doc().cursor_line(), 1);
    e.keys("up");
    e.assert_screen_contains("3 of 80");
    e.keys("esc ctrl+g").type_text("30").keys("enter ctrl+f");
    e.snapshot_styles("find_matches_scrolled");
    e.keys("esc");
    e.snapshot_styles("find_closed_without_highlights");
}
