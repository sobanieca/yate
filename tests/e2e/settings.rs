use yate::testing::Harness;

const CODE: &str = "// greeting\nconst name = \"yate\";\n";

#[test]
fn home_yate_json_sets_theme_colors_and_syntax() {
    let mut e = Harness::builder()
        .size(40, 5)
        .home_file(
            "yate.json",
            r##"{
              // night preset, with a custom keyword color
              "theme": {
                "preset": "tokyonight-night",
                "colors": { "orange": "#ff8800" },
                "syntax": { "keyword": { "fg": "#ff0000", "bold": true }, "comment": "green" },
              },
            }"##,
        )
        .file("a.js", CODE)
        .open("a.js")
        .start();
    assert_eq!(e.app().theme.name, "tokyonight-night");
    e.snapshot_styles("settings_theme_from_home");
}

#[test]
fn a_closer_yate_json_overrides_farther_ones() {
    let mut e = Harness::builder()
        .size(50, 6)
        .home_file(
            "yate.json",
            r#"{ "options": { "tabSize": 8, "lineNumbers": false } }"#,
        )
        .file("yate.json", r#"{ "options": { "tabSize": 4 } }"#)
        .file("app/yate.json", r#"{ "options": { "lineNumbers": true } }"#)
        .file("app/main.js", "")
        .working_dir("app")
        .open("app/main.js")
        .start();
    e.type_text("if (x) {\ny();");
    assert_eq!(e.editor_text(), "if (x) {\n    y();\n}");
    assert!(e.app().options.line_numbers);
    assert_eq!(e.app().settings_files.len(), 3);
}

#[test]
fn keys_from_yate_json() {
    let mut e = Harness::builder()
        .size(50, 6)
        .file(
            "yate.json",
            r#"{ "keys": { "editor": { "ctrl+y": "tree.toggle", "alt+z": "edit.redo" }, "global": { "ctrl+q": "" } } }"#,
        )
        .file("a.txt", "")
        .open("a.txt")
        .start();
    e.keys("ctrl+y");
    assert!(e.app().tree_visible);
    e.keys("esc ctrl+q");
    assert!(!e.app().quit);
}

#[test]
fn mistakes_in_yate_json_show_in_the_status_line() {
    Harness::builder()
        .size(120, 4)
        .file(
            "yate.json",
            "{ \"theme\": { \"preset\": \"solarized\" }, \"options\": { \"tabsize\": 4 } }",
        )
        .file("a.txt", "")
        .open("a.txt")
        .start()
        .snapshot("settings_errors");
    Harness::builder()
        .size(120, 4)
        .file(
            "yate.json",
            "{\n  \"options\": { \"tabSize\": 4 }\n  \"keys\": {}\n}",
        )
        .file("a.txt", "")
        .open("a.txt")
        .start()
        .assert_screen_contains("yate.json: line 3, column 3: expected ',' or '}'");
}

#[test]
fn saving_a_yate_json_applies_it() {
    let mut e = Harness::builder()
        .size(80, 8)
        .file("a.txt", "")
        .open("a.txt")
        .start();
    e.command("settings.open");
    assert_eq!(e.app().doc().name(), "yate.jsonc");
    e.snapshot("settings_template");
    e.keys("ctrl+a")
        .paste(r#"{ "theme": { "preset": "tokyonight-day" } }"#)
        .keys("ctrl+s");
    assert_eq!(e.app().theme.name, "tokyonight-day");
    e.assert_screen_contains("settings reloaded");
}

#[test]
fn the_nord_preset_colors_the_editor() {
    let mut e = Harness::builder()
        .size(40, 5)
        .home_file("yate.json", r#"{ "theme": { "preset": "nord" } }"#)
        .file("a.js", CODE)
        .open("a.js")
        .start();
    assert_eq!(e.app().theme.name, "nord");
    e.snapshot_styles("settings_theme_nord");
}
