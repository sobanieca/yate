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
    e.keys("ctrl+f").type_text("total");
    e.keys("tab tab tab enter");
    assert!(e.app().find.replace_open);
    e.type_text("sum").keys("enter");
    assert!(e.editor_text().starts_with("let sum = 0;"));
    e.snapshot("find_replace_bar");
    e.keys("right right");
    e.snapshot_styles("find_replace_all_button");
    e.keys("enter");
    assert!(!e.editor_text().contains("total"));
    e.assert_screen_contains("Replaced 2 matches");
    e.keys("shift+tab shift+tab space");
    assert!(!e.app().find.replace_open);
    assert_eq!(e.app().focus, yate::app::Focus::Find);
    e.assert_screen_contains("[ ] replace");
}

#[test]
fn tab_goes_from_the_query_to_the_replacement() {
    let mut e = editor();
    e.command("find.replace")
        .type_text("total")
        .keys("tab")
        .type_text("sum")
        .keys("down enter");
    assert!(e.editor_text().contains("  sum += item.price;"));
    assert!(e.editor_text().starts_with("let total = 0;"));
}

#[test]
fn checkboxes_set_case_and_regex() {
    let mut e = Harness::builder()
        .size(70, 8)
        .file("words.txt", "needle Needle\nneedles\n")
        .open("words.txt")
        .start();
    e.keys("ctrl+f").type_text("needle");
    e.assert_screen_contains("[ ] Aa  [ ] .*  [ ] replace  1 of 3");
    e.keys("right space");
    assert!(e.app().find.case_sensitive);
    e.assert_screen_contains("[x] Aa  [ ] .*  [ ] replace  1 of 2");
    e.keys("right enter").type_text(".");
    assert!(e.app().find.regex);
    assert_eq!(e.app().find.input.text, "needle.");
    e.snapshot_styles("find_checkboxes");
    e.keys("tab tab space");
    e.assert_screen_contains("[ ] .*  [ ] replace  no matches");
}

#[test]
fn clicks_tick_checkboxes_and_press_buttons() {
    let mut e = editor();
    e.keys("ctrl+f").type_text("total");
    let column_of = |e: &mut Harness, text: &str, row: u16| {
        let line = e.screen().line(row);
        let at = line.find(text).expect("the control is on the row");
        line[..at].chars().count() as u16
    };
    let column = column_of(&mut e, "[ ] replace", 8);
    e.click(column + 1, 8);
    assert!(e.app().find.replace_open);
    e.type_text("sum");
    let column = column_of(&mut e, "Replace all", 8);
    e.click(column, 8);
    assert!(!e.editor_text().contains("total"));
    let column = column_of(&mut e, "[ ] .*", 7);
    e.click(column, 7);
    assert!(e.app().find.regex);
}

#[test]
fn invalid_regex_is_shown_in_red_without_matches() {
    let mut e = editor();
    e.keys("ctrl+f tab tab space").type_text("(item");
    assert!(e.app().find.regex);
    e.assert_screen_contains("no matches");
}

#[test]
fn go_to_line() {
    let mut e = editor();
    e.keys("ctrl+g l").type_text("3").keys("enter");
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
    e.keys("esc ctrl+g l").type_text("30").keys("enter ctrl+f");
    e.snapshot_styles("find_matches_scrolled");
    e.keys("esc");
    e.snapshot_styles("find_closed_without_highlights");
}

#[test]
fn the_find_bar_fits_a_narrow_terminal() {
    let mut e = Harness::builder()
        .size(50, 8)
        .file("cart.js", TEXT)
        .open("cart.js")
        .start();
    e.keys("ctrl+f").type_text("total");
    e.assert_screen_contains("[ ] Aa [ ] .* [ ] replace 1 of 3");
}
