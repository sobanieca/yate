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
    e.keys("ctrl+n");
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

#[test]
fn deep_names_scroll_the_tree_sideways_to_fit() {
    let mut e = Harness::builder()
        .size(60, 10)
        .file(
            "packages/web/src/components/forms/inputs/DatePicker.tsx",
            "x\n",
        )
        .file("README.md", "# demo\n")
        .start();
    e.keys("right down right down right down right down right down right down");
    e.snapshot("tree_deep_name_scrolled");
    e.keys("home");
    e.snapshot("tree_deep_name_scrolled_back");
}

fn tree_width(e: &mut Harness) -> u16 {
    e.screen();
    e.app().layout.sidebar.map(|area| area.w).unwrap_or(0)
}

#[test]
fn brackets_and_dragging_the_edge_resize_the_tree() {
    let mut e = project();
    let automatic = tree_width(&mut e);
    e.keys("]");
    assert_eq!(tree_width(&mut e), automatic + 4);
    e.keys("[ [");
    assert_eq!(tree_width(&mut e), automatic - 4);
    e.drag((automatic - 5, 3), (33, 3));
    e.snapshot("tree_resized");
    assert_eq!(tree_width(&mut e), 34);
    e.drag((34, 3), (2, 3));
    assert_eq!(tree_width(&mut e), 12);
    e.drag((11, 3), (69, 3));
    assert_eq!(tree_width(&mut e), 50);
    e.command("tree.autoWidth");
    assert_eq!(tree_width(&mut e), automatic);
    e.click(automatic - 1, 3);
    assert_eq!(e.app().focus, yate::app::Focus::Tree);
    assert_eq!(e.app().doc().name(), "untitled");
}

#[test]
fn tree_width_is_an_option() {
    let mut e = Harness::builder()
        .size(70, 6)
        .file("a.txt", "a\n")
        .home_file("yate.json", r#"{ "options": { "treeWidth": 30 } }"#)
        .start();
    assert_eq!(tree_width(&mut e), 30);
}
