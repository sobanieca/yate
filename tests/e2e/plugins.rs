use yate::testing::Harness;

#[test]
fn a_plugin_switched_on_in_yate_json_registers_a_command_with_a_key() {
    let mut e = Harness::builder()
        .size(60, 8)
        .home_file("yate.jsonc", r#"{ "plugins": { "shout": true } }"#)
        .config(
            "plugins/shout.js",
            r#"
            yate.registerCommand("shout", "Shout the selection", () => {
              yate.editor.replaceSelection(yate.editor.selection().text.toUpperCase() + "!");
            });
            yate.bindKey("alt+s", "shout");
            "#,
        )
        .file("a.txt", "hello world\n")
        .open("a.txt")
        .start();
    e.keys("ctrl+d alt+s");
    assert_eq!(e.editor_text(), "HELLO! world\n");
    e.keys("ctrl+h").type_text("shout");
    e.snapshot("plugins_command_in_palette");
}

#[test]
fn options_from_a_plugin_apply() {
    let mut e = Harness::builder()
        .size(50, 8)
        .home_file("yate.json", r#"{ "plugins": { "settings": true } }"#)
        .config(
            "plugins/settings.js",
            r#"yate.setOption("tabSize", 4); yate.setOption("lineNumbers", false);"#,
        )
        .file("a.js", "")
        .open("a.js")
        .start();
    e.type_text("if (x) {\ny();");
    assert_eq!(e.editor_text(), "if (x) {\n    y();\n}");
    e.snapshot("plugins_options");
}

#[test]
fn prompts_and_events_reach_plugins() {
    let mut e = Harness::builder()
        .size(80, 8)
        .home_file("yate.json", r#"{ "plugins": { "greet": true } }"#)
        .config(
            "plugins/greet.js",
            r#"
            yate.registerCommand("greet", () => {
              yate.prompt("Your name", (name) => yate.editor.insert(`hi ${name}`), "you");
            });
            yate.on("save", (path) => yate.notify(`saved ${path.split("/").pop()} from a plugin`));
            "#,
        )
        .file("a.txt", "")
        .open("a.txt")
        .start();
    e.command("greet");
    e.snapshot("plugins_prompt");
    e.keys("ctrl+u").type_text("Ann").keys("enter");
    assert_eq!(e.editor_text(), "hi Ann");
    e.keys("ctrl+s");
    e.assert_screen_contains("saved a.txt from a plugin");
}

#[test]
fn plugin_errors_show_in_the_status_line() {
    Harness::builder()
        .size(110, 6)
        .home_file("yate.json", r#"{ "plugins": { "broken": true } }"#)
        .config(
            "plugins/broken.js",
            "yate.setOption('tabSize', 'wide');\nundefinedFunction();",
        )
        .file("a.txt", "")
        .open("a.txt")
        .start()
        .snapshot("plugins_error");
}

#[test]
fn plugins_not_switched_on_do_not_load_and_bundled_ones_can_be_switched_off() {
    let e = Harness::builder()
        .home_file("yate.json", r#"{ "plugins": { "git": false } }"#)
        .config("plugins/idle.js", "yate.registerCommand('idle', () => {});")
        .file("a.txt", "")
        .open("a.txt")
        .start();
    let commands = &e.app().plugin_commands;
    assert!(commands.iter().any(|(id, _)| id == "format.prettier"));
    assert!(commands.iter().all(|(id, _)| id != "git.blame"));
    assert!(commands.iter().all(|(id, _)| id != "idle"));
}
