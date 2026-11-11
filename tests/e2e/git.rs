use yate::testing::Harness;

const MAIN_RS: &str =
    "fn main() {\n    let name = \"world\";\n    println!(\"hello {name}\");\n}\n";

fn repo() -> Harness {
    Harness::builder()
        .size(80, 14)
        .file("src/main.rs", MAIN_RS)
        .file("Cargo.toml", "[package]\nname = \"demo\"\n")
        .git()
        .open("src/main.rs")
        .start()
}

#[test]
fn gutter_signs_follow_unsaved_edits() {
    let mut e = repo();
    e.keys("down end").type_text("\nlet greeting = 1;");
    e.keys("down down ctrl+k");
    e.snapshot("git_gutter_signs");
}
