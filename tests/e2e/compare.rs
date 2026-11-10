use yate::testing::Harness;

fn project() -> Harness {
    Harness::builder()
        .size(80, 12)
        .file("old/app.js", "let a = 1;\nlet b = 2;\n")
        .file("old/gone.js", "bye\n")
        .file("old/same.js", "same\n")
        .file("new/app.js", "let a = 1;\nlet b = 3;\n")
        .file("new/added.js", "hello\n")
        .file("new/same.js", "same\n")
        .start()
}

#[test]
fn folders_marked_in_the_tree_are_compared() {
    let mut e = project();
    e.keys("down c");
    e.assert_screen_contains("compare…");
    e.keys("up c");
    e.snapshot("compare_folders");
    e.keys("down");
    e.assert_screen_contains("old/app.js → new/app.js");
    e.keys("esc");
    assert!(e.app().review.is_none());
}

#[test]
fn files_marked_in_the_tree_are_compared() {
    let mut e = project();
    e.keys("enter down down down down enter down c");
    e.keys("up up up c");
    e.snapshot("compare_files");
}

#[test]
fn the_current_file_is_compared_with_a_picked_file() {
    let mut e = Harness::builder()
        .size(80, 10)
        .file("a.txt", "one\ntwo\n")
        .file("b.txt", "one\n2\n")
        .open("b.txt")
        .start();
    e.keys("end").type_text(" edited");
    e.command("file.compareWith")
        .type_text("a.txt")
        .keys("enter");
    e.assert_screen_contains("a.txt → b.txt");
    e.assert_screen_contains("one edited");
}

#[test]
fn a_file_is_not_compared_with_a_folder() {
    let mut e = project();
    e.keys("down c up enter down c");
    e.assert_screen_contains("Compare two files or two folders");
}

#[test]
fn yate_diff_compares_two_folders_from_the_command_line() {
    let mut e = Harness::builder()
        .size(80, 12)
        .file("old/app.js", "let a = 1;\nlet b = 2;\n")
        .file("old/same.js", "same\n")
        .file("new/app.js", "let a = 1;\nlet b = 3;\n")
        .file("new/same.js", "same\n")
        .diff("old", "new")
        .start();
    e.snapshot("diff_folders_from_the_command_line");
    e.keys("esc");
    assert!(e.app().review.is_none());
}

#[test]
fn yate_diff_compares_two_files_by_path_from_a_subfolder() {
    let mut e = Harness::builder()
        .size(80, 10)
        .file("src/a.txt", "one\ntwo\n")
        .file("lib/b.txt", "one\n2\n")
        .working_dir("src")
        .diff("a.txt", "../lib/b.txt")
        .start();
    e.snapshot("diff_files_from_the_command_line");
}

#[test]
fn yate_diff_refuses_a_file_with_a_folder() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let error = yate::app::compared_paths(root, "src", "Cargo.toml").unwrap_err();
    assert!(error.contains("not a file with a folder"));
    let error = yate::app::compared_paths(root, "missing-a", "missing-b").unwrap_err();
    assert_eq!(error, "no such file or folder: missing-a");
}

#[test]
fn the_project_of_yate_diff_is_the_folder_that_holds_both_paths() {
    use std::path::Path;
    assert_eq!(
        yate::app::common_folder(Path::new("/x/a/one"), Path::new("/x/b/two")),
        Path::new("/x")
    );
}
