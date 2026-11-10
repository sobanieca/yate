use yate::testing::Harness;

fn project() -> Harness {
    Harness::builder()
        .size(70, 14)
        .file(".gitignore", "dist/\n*.log\n")
        .file("src/app.ts", "export const app = 1;\n")
        .file("src/lib/util.ts", "export const util = 2;\n")
        .file("README.md", "# demo\n")
        .file("dist/bundle.js", "built\n")
        .file("debug.log", "log\n")
        .git()
        .start()
}

#[test]
fn folders_come_first_and_expand_on_enter() {
    let mut e = project();
    e.snapshot("tree_initial");
    e.keys("enter");
    e.snapshot("tree_src_expanded");
}

#[test]
fn ignored_files_are_hidden_until_toggled() {
    let mut e = project();
    e.assert_screen_lacks("bundle");
    e.keys("i");
    e.snapshot("tree_with_ignored_files");
}

#[test]
fn only_changed_files_filter() {
    let mut e = project();
    e.write_file("src/lib/util.ts", "export const util = 3;\n")
        .write_file("new.ts", "new\n")
        .focus_gained();
    e.keys("m");
    e.keys("right down right");
    e.snapshot("tree_changed_only");
}

#[test]
fn enter_on_a_file_opens_it_and_closes_the_tree_on_narrow_terminals() {
    let mut e = project();
    e.keys("end enter");
    assert_eq!(e.app().doc().name(), "README.md");
    e.assert_screen_lacks("SRC");
    e.keys("ctrl+t");
    e.snapshot("tree_reveal_current_file");
}

#[test]
fn new_rename_and_delete() {
    let mut e = project();
    e.keys("a");
    e.snapshot("tree_new_file_prompt");
    e.type_text("routes/home.ts").keys("enter");
    assert!(e.file_exists("src/routes/home.ts"));
    assert_eq!(e.app().doc().name(), "home.ts");

    e.keys("ctrl+t r ctrl+u")
        .type_text("src/routes/index.ts")
        .keys("enter");
    assert!(e.file_exists("src/routes/index.ts"));
    assert_eq!(e.app().doc().name(), "index.ts");

    e.keys("d");
    e.assert_screen_contains("Delete src/routes/index.ts? (y/n)");
    e.keys("y");
    assert!(!e.file_exists("src/routes/index.ts"));
}

#[test]
fn mouse_click_on_a_folder_toggles_it() {
    let mut e = project();
    e.click(4, 1);
    e.assert_screen_contains("lib");
    e.click(4, 1);
    e.assert_screen_lacks("lib");
}

#[test]
fn f_and_g_search_in_the_selected_folder() {
    let mut e = Harness::builder()
        .size(90, 14)
        .file("src/app.ts", "export const app = util();\n")
        .file("src/lib/util.ts", "export const util = () => app;\n")
        .file("src/lib/util.test.ts", "util();\n")
        .file("docs/app.md", "app\n")
        .start();
    e.keys("down f");
    e.assert_screen_contains("Find file in src/");
    e.assert_screen_lacks("docs/app.md");
    e.keys("ctrl+f").type_text("!*.test.ts");
    e.snapshot("tree_files_in_folder");
    e.keys("esc enter down g").type_text("app");
    e.assert_screen_contains("Live grep in src/lib/");
    e.assert_screen_contains("src/lib/util.ts:1");
    e.assert_screen_lacks("src/app.ts:1");
    e.assert_screen_lacks("util.test.ts");
    e.snapshot("tree_grep_in_folder");
}

#[test]
fn the_tree_title_and_status_label_take_the_tree_color_when_focused() {
    let mut e = Harness::builder().size(40, 5).file("a.txt", "a\n").start();
    e.snapshot_styles("tree_focus_color");
}
