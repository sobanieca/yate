use yate::testing::Harness;

const MAIN_RS: &str =
    "fn main() {\n    let name = \"world\";\n    println!(\"hello {name}\");\n}\n";

fn repo() -> Harness {
    Harness::builder()
        .size(80, 14)
        .file("src/main.rs", MAIN_RS)
        .file("Cargo.toml", "[package]\nname = \"demo\"\n")
        .git()
        .open("src/main.rs")
        .start()
}

#[test]
fn gutter_signs_follow_unsaved_edits() {
    let mut e = repo();
    e.keys("down end").type_text("\nlet greeting = 1;");
    e.keys("down down ctrl+k");
    e.snapshot("git_gutter_signs");
}

#[test]
fn review_shows_changed_files_and_the_diff_against_head() {
    let mut e = repo();
    e.keys("down home shift+end")
        .type_text("let name = \"yate\";")
        .keys("ctrl+s");
    e.write_file("NOTES.md", "todo\n").focus_gained();
    e.keys("alt+g");
    e.snapshot_styles("git_review");
    e.keys("up");
    e.snapshot("git_review_untracked_file");
}

#[test]
fn review_enter_opens_the_file_at_the_change() {
    let mut e = repo();
    e.write_file("Cargo.toml", "[package]\nname = \"renamed\"\n")
        .focus_gained();
    e.keys("alt+g");
    e.assert_screen_contains("CHANGES (1)");
    e.keys("tab enter");
    assert_eq!(e.app().doc().name(), "Cargo.toml");
    assert_eq!(e.app().doc().cursor_line(), 1);
}

#[test]
fn committing_outside_clears_the_review() {
    let mut e = repo();
    e.write_file("Cargo.toml", "changed\n").focus_gained();
    e.keys("alt+g");
    e.assert_screen_contains("CHANGES (1)");
    e.git(&["commit", "-qam", "change"]);
    e.focus_gained();
    e.assert_screen_contains("CHANGES (0)");
    e.assert_screen_contains("Working tree clean");
}
