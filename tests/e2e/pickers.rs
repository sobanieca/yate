use yate::testing::Harness;

fn project() -> Harness {
    Harness::builder()
        .size(80, 16)
        .file(
            "src/editor/view.ts",
            "export function render(view) {\n  return view.lines;\n}\n",
        )
        .file("src/editor/model.ts", "export const lines = [];\n")
        .file(
            "src/app.ts",
            "import { render } from './editor/view';\nrender(app);\n",
        )
        .file("test/view.test.ts", "render({ lines: [] });\n")
        .file("README.md", "Call render() to draw.\n")
        .home_file("yate.json", r#"{ "options": { "keyHintDelay": 0 } }"#)
        .open("README.md")
        .start()
}

#[test]
fn ctrl_p_finds_files_fuzzily_and_opens_them() {
    let mut e = project();
    e.keys("ctrl+p").type_text("edview");
    e.snapshot("pickers_files_fuzzy");
    e.keys("enter");
    assert_eq!(e.app().doc().name(), "view.ts");
}

#[test]
fn live_grep_lists_matches_in_file_order() {
    let mut e = project();
    e.keys("ctrl+r").type_text("render");
    e.snapshot("pickers_grep");
    e.keys("down enter");
    assert_eq!(e.app().doc().name(), "app.ts");
    assert_eq!(e.app().doc().cursor_line(), 0);
}

#[test]
fn live_grep_glob_filter() {
    let mut e = project();
    e.keys("ctrl+r")
        .type_text("render")
        .keys("tab")
        .type_text("*.ts,!*.test.ts");
    e.keys("tab").type_text("(");
    e.snapshot("pickers_grep_glob");
}

#[test]
fn the_selection_prefills_grep_and_typing_replaces_it() {
    let mut e = project();
    e.keys("ctrl+d ctrl+r");
    e.assert_screen_contains("❯ Call");
    e.type_text("lines");
    e.assert_screen_contains("❯ lines");
    e.assert_screen_contains("src/editor/model.ts:1");
}

#[test]
fn command_palette_filters_and_runs_commands() {
    let mut e = project();
    e.keys("ctrl+h").type_text("toggle tree");
    e.snapshot("pickers_command_palette");
    e.keys("enter");
    assert!(e.app().tree_visible);
}

#[test]
fn escape_closes_and_up_brings_back_the_last_search() {
    let mut e = project();
    e.keys("ctrl+p").type_text("model").keys("esc");
    e.assert_screen_lacks("Find file");
    e.keys("ctrl+p up");
    e.assert_screen_contains("❯ model");
    e.keys("esc ctrl+r up");
    e.assert_screen_lacks("❯ model");
    e.type_text("render").keys("esc ctrl+r ctrl+p");
    e.assert_screen_contains("❯ render");
    e.assert_screen_contains("src/app.ts:1");
    e.keys("esc");
    e.command("picker.resume");
    e.assert_screen_contains("❯ render");
}

#[test]
fn recent_files_come_first_and_bookmarks_are_listed() {
    let mut e = project();
    e.keys("ctrl+p").type_text("app.ts").keys("enter");
    e.keys("down ctrl+k t");
    e.keys("ctrl+p").type_text("model").keys("enter");
    e.keys("ctrl+p");
    e.snapshot("pickers_recent_files");
    e.keys("esc ctrl+k l");
    e.snapshot("pickers_bookmarks");
    e.keys("enter");
    assert_eq!(e.app().doc().name(), "app.ts");
    assert_eq!(e.app().doc().cursor_line(), 1);
}

#[test]
fn tab_filters_found_files_with_globs() {
    let mut e = project();
    e.keys("ctrl+p");
    e.assert_screen_contains("tab to filter");
    e.keys("tab");
    e.assert_screen_contains("glob *.ts, !test/**");
    e.type_text("*.ts, !src/editor/**");
    e.keys("tab").type_text("view");
    e.snapshot("pickers_files_glob");
    e.keys("enter");
    assert_eq!(e.app().doc().name(), "view.test.ts");
}

#[test]
fn the_glob_filter_is_kept_for_the_next_search() {
    let mut e = project();
    e.keys("ctrl+p tab").type_text("!test").keys("esc");
    e.keys("ctrl+r").type_text("render");
    e.assert_screen_contains("glob !test");
    e.assert_screen_lacks("test/view.test.ts");
    e.assert_screen_contains("src/app.ts:1");
}

#[test]
fn ctrl_k_groups_the_bookmark_commands() {
    let mut e = project();
    e.keys("ctrl+p").type_text("view.ts").keys("enter");
    e.keys("ctrl+k t down down ctrl+k t ctrl+home ctrl+k");
    e.snapshot("pickers_bookmark_menu");
    e.keys("down");
    assert_eq!(e.app().doc().cursor_line(), 2);
    e.keys("ctrl+up");
    assert_eq!(e.app().doc().cursor_line(), 0);
    e.keys("ctrl+k c ctrl+k l");
    e.assert_screen_contains("Bookmarks");
    e.assert_screen_contains("0/0");
}

#[test]
fn the_command_palette_lists_the_commands_of_the_focused_panel() {
    let mut e = project();
    e.keys("ctrl+h").type_text("line");
    e.assert_screen_contains("Commands (editor)");
    e.assert_screen_contains("Delete line");
    e.assert_screen_contains("ctrl+l k");
    e.keys("ctrl+u").type_text("wider");
    e.assert_screen_contains("No results");
    e.keys("esc ctrl+t t ctrl+h");
    e.snapshot("pickers_command_palette_tree");
    e.type_text("line");
    e.assert_screen_lacks("Delete line");
}

#[test]
fn the_command_palette_groups_commands_into_sections() {
    let mut e = Harness::builder()
        .size(80, 62)
        .file("README.md", "Call render() to draw.\n")
        .open("README.md")
        .start();
    e.keys("ctrl+h");
    e.snapshot("pickers_command_palette_sections");
    e.type_text("tree");
    e.assert_screen_contains("File tree  Toggle");
    e.assert_screen_contains("File tree  Reveal current file");
    e.assert_screen_contains("ctrl+t r");
    e.keys("ctrl+u").type_text("pick a button");
    e.assert_screen_contains("Keys       Dialog: pick a button");
    e.keys("enter");
    e.assert_screen_contains("EDIT");
}

#[test]
fn the_command_palette_lists_only_what_can_be_done_now() {
    let mut e = project();
    e.keys("ctrl+h").type_text("pane");
    e.assert_screen_lacks("Focus the next pane");
    e.assert_screen_lacks("Close this pane");
    e.keys("esc ctrl+w v ctrl+h").type_text("pane");
    e.assert_screen_contains("Focus the next pane");
    e.assert_screen_contains("Close this pane");
    e.keys("esc ctrl+h").type_text("bookmarks");
    e.assert_screen_contains("Toggle on this line");
    e.assert_screen_lacks("Clear in this file");
    e.keys("enter ctrl+h").type_text("bookmarks");
    e.assert_screen_contains("Clear in this file");
    e.keys("esc ctrl+h").type_text("undo");
    e.assert_screen_contains("Edit       Undo");
    e.keys("esc ctrl+h").type_text("palette");
    e.assert_screen_lacks("Command palette");
}
