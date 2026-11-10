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
fn buffers_and_bookmarks() {
    let mut e = project();
    e.keys("ctrl+p").type_text("app.ts").keys("enter");
    e.keys("down alt+m");
    e.keys("ctrl+p").type_text("model").keys("enter");
    e.keys("ctrl+e");
    e.snapshot("pickers_buffers");
    e.keys("esc ctrl+l");
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
