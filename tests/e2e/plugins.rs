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
fn a_message_that_fits_the_status_line_stays_there() {
    let mut e = Harness::builder()
        .size(60, 6)
        .home_file("yate.json", r#"{ "plugins": { "say": true } }"#)
        .config(
            "plugins/say.js",
            r#"
            yate.registerCommand("say.short", () => yate.notify("Done"));
            yate.registerCommand("say.medium", () => yate.notify("Sorted 12 lines of the selection, a to z"));
            "#,
        )
        .file("a.txt", "hello\n")
        .open("a.txt")
        .start();
    e.command("say.short");
    assert!(e.app().dialog.is_none());
    e.assert_screen_contains("Done")
        .assert_screen_contains("utf-8");
    e.command("say.medium");
    assert!(e.app().dialog.is_none());
    e.snapshot("plugins_message_takes_the_file_info_room");
}

#[test]
fn a_message_too_long_for_the_status_line_opens_a_dialog() {
    let mut e = Harness::builder()
        .size(60, 10)
        .home_file("yate.json", r#"{ "plugins": { "say": true } }"#)
        .config(
            "plugins/say.js",
            r#"
            yate.registerCommand("say.long", () => {
              yate.notify("The quick brown fox jumps over the lazy dog, then it rests under the old oak tree");
            });
            yate.registerCommand("say.lines", () => yate.error("first problem\nsecond problem"));
            "#,
        )
        .file("a.txt", "hello\n")
        .open("a.txt")
        .start();
    e.command("say.long");
    e.snapshot("plugins_long_message_dialog");
    e.type_text("x").paste("pasted");
    assert_eq!(e.editor_text(), "hello\n");
    e.keys("esc");
    assert!(e.app().dialog.is_none());
    e.assert_screen_lacks("quick brown fox");
    e.type_text("x");
    assert_eq!(e.editor_text(), "xhello\n");
    e.command("say.lines");
    e.snapshot("plugins_error_dialog");
    e.keys("enter");
    assert!(e.app().dialog.is_none());
}

#[test]
fn a_plugin_shows_a_dialog_that_scrolls_and_closes_on_a_click() {
    let mut e = Harness::builder()
        .size(50, 10)
        .home_file("yate.json", r#"{ "plugins": { "list": true } }"#)
        .config(
            "plugins/list.js",
            r#"
            yate.registerCommand("list", () => {
              const lines = Array.from({ length: 12 }, (_, i) => `item ${i + 1}`);
              yate.show("Items", lines.join("\n"));
            });
            "#,
        )
        .file("a.txt", "")
        .open("a.txt")
        .start();
    e.command("list");
    e.snapshot("plugins_dialog");
    e.keys("down down");
    e.assert_screen_contains("item 7")
        .assert_screen_lacks("item 2 ");
    e.keys("end");
    e.assert_screen_contains("item 12")
        .assert_screen_contains("8-12 of 12");
    e.scroll(25, 4, -1);
    e.assert_screen_lacks("item 12");
    e.click(25, 4);
    assert!(e.app().dialog.is_some());
    e.click(1, 1);
    assert!(e.app().dialog.is_none());
    e.command("list").click(13, 7);
    assert!(e.app().dialog.is_none());
}

#[test]
fn git_blame_shows_the_commit_of_the_line_in_a_dialog() {
    let mut e = Harness::builder()
        .size(60, 12)
        .file("a.txt", "hello\nworld\n")
        .git()
        .open("a.txt")
        .start();
    e.keys("down").command("git.blame");
    e.assert_screen_contains("Git blame: line 2")
        .assert_screen_contains("Author  yate")
        .assert_screen_contains("initial");
    e.keys("esc");
    e.assert_screen_lacks("Git blame");
}

#[test]
fn plugin_errors_show_in_a_dialog() {
    Harness::builder()
        .size(110, 8)
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
