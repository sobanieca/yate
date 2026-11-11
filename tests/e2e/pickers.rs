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
    e.keys("alt+f").type_text("render");
    e.snapshot("pickers_grep");
    e.keys("down enter");
    assert_eq!(e.app().doc().name(), "app.ts");
    assert_eq!(e.app().doc().cursor_line(), 0);
}

#[test]
fn live_grep_glob_filter() {
    let mut e = project();
    e.keys("alt+f")
        .type_text("render")
        .keys("tab")
        .type_text("*.ts,!*.test.ts");
    e.keys("tab").type_text("(");
    e.snapshot("pickers_grep_glob");
}

#[test]
fn the_selection_prefills_grep_and_typing_replaces_it() {
    let mut e = project();
    e.keys("ctrl+d alt+f");
    e.assert_screen_contains("❯ Call");
    e.type_text("lines");
    e.assert_screen_contains("❯ lines");
    e.assert_screen_contains("src/editor/model.ts:1");
}

#[test]
fn command_palette_filters_and_runs_commands() {
    let mut e = project();
    e.keys("alt+p").type_text("toggle tree");
    e.snapshot("pickers_command_palette");
    e.keys("enter");
    assert!(e.app().tree_visible);
}

#[test]
fn escape_closes_and_alt_r_resumes_the_last_picker() {
    let mut e = project();
    e.keys("ctrl+p").type_text("model").keys("esc");
    e.assert_screen_lacks("Find file");
    e.keys("alt+r");
    e.assert_screen_contains("❯ model");
}

#[test]
fn recent_files_come_first_and_bookmarks_are_listed() {
    let mut e = project();
    e.keys("ctrl+p").type_text("app.ts").keys("enter");
    e.keys("down alt+m 1");
    e.keys("ctrl+p").type_text("model").keys("enter");
    e.keys("ctrl+p");
    e.snapshot("pickers_recent_files");
    e.keys("esc alt+m 4");
    e.snapshot("pickers_bookmarks");
    e.keys("enter");
    assert_eq!(e.app().doc().name(), "app.ts");
    assert_eq!(e.app().doc().cursor_line(), 1);
}

#[test]
fn ctrl_f_filters_found_files_with_globs() {
    let mut e = project();
    e.keys("ctrl+p");
    e.assert_screen_contains("ctrl+f to filter");
    e.keys("ctrl+f");
    e.assert_screen_contains("glob *.ts, !test/**");
    e.type_text("*.ts, !src/editor/**");
    e.keys("ctrl+f").type_text("view");
    e.snapshot("pickers_files_glob");
    e.keys("enter");
    assert_eq!(e.app().doc().name(), "view.test.ts");
}

#[test]
fn the_glob_filter_is_kept_for_the_next_search() {
    let mut e = project();
    e.keys("ctrl+p ctrl+f").type_text("!test").keys("esc");
    e.keys("alt+f").type_text("render");
    e.assert_screen_contains("glob !test");
    e.assert_screen_lacks("test/view.test.ts");
    e.assert_screen_contains("src/app.ts:1");
}

#[test]
fn alt_m_groups_the_bookmark_commands() {
    let mut e = project();
    e.keys("ctrl+p").type_text("view.ts").keys("enter");
    e.keys("alt+m 1 down down alt+m 1 ctrl+home alt+m");
    e.snapshot("pickers_bookmark_menu");
    e.keys("2");
    assert_eq!(e.app().doc().cursor_line(), 2);
    e.keys("alt+m 3");
    assert_eq!(e.app().doc().cursor_line(), 0);
    e.keys("alt+m 5 alt+m 4");
    e.assert_screen_contains("Bookmarks");
    e.assert_screen_contains("0/0");
}

#[test]
fn the_command_palette_lists_the_commands_of_the_focused_panel() {
    let mut e = project();
    e.keys("alt+p").type_text("line");
    e.assert_screen_contains("Commands (editor)");
    e.assert_screen_contains("Delete line");
    e.keys("ctrl+u").type_text("wider");
    e.assert_screen_contains("No results");
    e.keys("esc alt+e alt+p");
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
    e.keys("alt+p");
    e.snapshot("pickers_command_palette_sections");
    e.type_text("tree");
    e.assert_screen_contains("File tree  Toggle");
    e.assert_screen_contains("File tree  Reveal current file");
}

#[test]
fn the_command_palette_lists_only_what_can_be_done_now() {
    let mut e = project();
    e.keys("alt+p").type_text("pane");
    e.assert_screen_lacks("Focus the next pane");
    e.assert_screen_lacks("Close this pane");
    e.keys("esc alt+s 1 alt+p").type_text("pane");
    e.assert_screen_contains("Focus the next pane");
    e.assert_screen_contains("Close this pane");
    e.keys("esc alt+p").type_text("bookmarks");
    e.assert_screen_contains("Toggle on this line");
    e.assert_screen_lacks("Clear in this file");
    e.keys("enter alt+p").type_text("bookmarks");
    e.assert_screen_contains("Clear in this file");
    e.keys("esc alt+p").type_text("undo");
    e.assert_screen_contains("Edit       Undo");
    e.keys("esc alt+p").type_text("palette");
    e.assert_screen_lacks("Command palette");
}
