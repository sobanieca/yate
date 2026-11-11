use yate::testing::Harness;

const APP_TS: &str = "import { render } from './editor/view';
import { rows } from '@/editor';
import * as model from './editor/model';

const app = { lines: rows };
render(app);
model.reset();
";

fn project() -> Harness {
    Harness::builder()
        .size(80, 16)
        .file(
            "tsconfig.json",
            r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@/*": ["src/*"] } } }"#,
        )
        .file("src/app.ts", APP_TS)
        .file(
            "src/editor/index.ts",
            "export * from './view';\nexport { lines as rows } from './model';\n",
        )
        .file(
            "src/editor/view.ts",
            "export function render(view) {\n  return view.lines;\n}\n",
        )
        .file(
            "src/editor/model.ts",
            "export const lines = [];\nexport function reset() {}\n",
        )
        .file("lib/legacy.js", "// old\nfunction render() {}\n")
        .file("test/helpers.ts", "\nexport const render = () => null;\n")
        .file("README.md", "See src/editor/view.ts:2 for the details.\n")
        .open("src/app.ts")
        .start()
}

fn cursor(e: &Harness) -> (String, (usize, usize)) {
    let doc = e.app().doc();
    (
        e.app().relative(doc.path.as_ref().unwrap()),
        doc.line_col(doc.selection.head),
    )
}

#[test]
fn an_import_path_opens_the_module_and_alt_left_comes_back() {
    let mut e = project();
    e.click(31, 0).keys("ctrl+g 2");
    assert_eq!(cursor(&e).0, "src/editor/view.ts");
    e.keys("alt+left");
    assert_eq!(cursor(&e), ("src/app.ts".to_string(), (0, 26)));
}

#[test]
fn an_imported_name_jumps_to_its_definition_in_the_module() {
    let mut e = project();
    e.click(5, 5).keys("ctrl+g 2");
    assert_eq!(cursor(&e), ("src/editor/view.ts".to_string(), (0, 16)));
    e.snapshot("definitions_imported_function");
}

#[test]
fn reexports_and_path_aliases_are_followed() {
    let mut e = project();
    e.click(26, 4).keys("ctrl+g 2");
    assert_eq!(cursor(&e), ("src/editor/model.ts".to_string(), (0, 13)));
}

#[test]
fn a_name_defined_in_the_same_file_moves_the_cursor() {
    let mut e = project();
    e.ctrl_click(12, 5);
    assert_eq!(cursor(&e), ("src/app.ts".to_string(), (4, 6)));
}

#[test]
fn a_member_is_found_in_the_project() {
    let mut e = project();
    e.click(11, 6).keys("ctrl+g 2");
    assert_eq!(cursor(&e), ("src/editor/model.ts".to_string(), (1, 16)));
}

#[test]
fn at_a_definition_the_other_definitions_are_listed() {
    let mut e = project();
    e.click(5, 5).keys("ctrl+g 2").keys("ctrl+g 2");
    e.snapshot("definitions_picker");
    e.keys("down enter");
    assert_eq!(cursor(&e), ("test/helpers.ts".to_string(), (1, 13)));
}

#[test]
fn a_path_with_a_line_number_in_text_opens_there() {
    let mut e = project();
    e.keys("ctrl+p").type_text("README").keys("enter");
    e.ctrl_click(12, 0);
    assert_eq!(cursor(&e), ("src/editor/view.ts".to_string(), (1, 0)));
}

#[test]
fn ctrl_p_accepts_a_line_number_after_the_file() {
    let mut e = project();
    e.keys("ctrl+p").type_text("model.ts:2").keys("enter");
    assert_eq!(cursor(&e), ("src/editor/model.ts".to_string(), (1, 0)));
}

#[test]
fn an_unknown_name_says_so() {
    let mut e = project();
    e.keys("ctrl+end")
        .type_text("missing();")
        .keys("home ctrl+g 2");
    e.assert_screen_contains("No definition of missing found");
}

#[test]
fn ctrl_g_asks_for_a_line_or_the_definition() {
    let mut e = project();
    e.click(5, 5).keys("ctrl+g");
    e.snapshot("definitions_go_to_menu");
    e.keys("2");
    assert_eq!(cursor(&e), ("src/editor/view.ts".to_string(), (0, 16)));
    e.keys("ctrl+g 1").type_text("3").keys("enter");
    assert_eq!(cursor(&e), ("src/editor/view.ts".to_string(), (2, 0)));
}

#[test]
fn the_go_to_menu_takes_arrows_enter_and_escape() {
    let mut e = project();
    e.click(5, 5).keys("ctrl+g esc");
    e.assert_screen_lacks("Go to");
    assert_eq!(cursor(&e).0, "src/app.ts");
    e.keys("ctrl+g x 9");
    e.assert_screen_contains("Go to");
    assert_eq!(e.editor_text(), APP_TS);
    e.keys("down enter");
    assert_eq!(cursor(&e), ("src/editor/view.ts".to_string(), (0, 16)));
}
