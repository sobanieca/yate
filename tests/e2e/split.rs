use yate::testing::Harness;

fn numbered(count: usize, prefix: &str) -> String {
    (1..=count).map(|i| format!("{prefix} {i}\n")).collect()
}

fn project_of_size(width: u16, height: u16) -> Harness {
    Harness::builder()
        .size(width, height)
        .file("one.txt", &numbered(60, "one"))
        .file("two.txt", &numbered(60, "two"))
        .file("three.txt", &numbered(60, "three"))
        .home_file(
            "yate.json",
            r#"{ "options": { "scrollOff": 0, "keyHintDelay": 0 } }"#,
        )
        .open("one.txt")
        .start()
}

fn project() -> Harness {
    project_of_size(70, 10)
}

fn open(e: &mut Harness, file: &str) {
    e.keys("ctrl+p").type_text(file).keys("enter");
}

fn pane_sizes(e: &Harness) -> Vec<(u16, u16)> {
    let panes = e.app().layout.panes.iter();
    panes.map(|pane| pane.size()).collect()
}

#[test]
fn ctrl_w_asks_what_to_do_with_the_panes() {
    let mut e = project();
    e.keys("ctrl+w");
    e.snapshot("split_menu");
    e.keys("esc");
    assert!(!e.app().panes.is_split());
    e.keys("ctrl+w ctrl+s");
    assert_eq!(pane_sizes(&e), [(70, 4), (70, 5)]);
    let below = e.app().panes.focused;
    e.keys("ctrl+w ctrl+w");
    assert_ne!(e.app().panes.focused, below);
    e.keys("ctrl+w down");
    assert_eq!(e.app().panes.focused, below);
}

#[test]
fn a_vertical_split_shows_two_panes_with_their_own_files() {
    let mut e = project();
    e.keys("ctrl+w v");
    open(&mut e, "two.txt");
    e.snapshot("split_two_files");
    e.keys("ctrl+w w");
    assert_eq!(e.app().doc().name(), "one.txt");
    e.keys("ctrl+w w");
    assert_eq!(e.app().doc().name(), "two.txt");
    e.keys("ctrl+w c");
    assert!(!e.app().panes.is_split());
    assert_eq!(e.app().doc().name(), "one.txt");
    assert_eq!(e.app().docs.len(), 1);
}

#[test]
fn a_horizontal_split_stacks_the_panes() {
    let mut e = project();
    e.keys("ctrl+w s");
    open(&mut e, "two.txt");
    e.snapshot("split_stacked");
    e.click(10, 2);
    assert_eq!(e.app().doc().name(), "one.txt");
    assert_eq!(e.app().doc().cursor_line(), 1);
}

#[test]
fn panes_split_the_same_way_share_the_space_evenly() {
    let mut e = project();
    e.keys("ctrl+w v");
    assert_eq!(pane_sizes(&e), [(34, 9), (35, 9)]);
    open(&mut e, "two.txt");
    e.keys("ctrl+w v");
    open(&mut e, "three.txt");
    assert_eq!(pane_sizes(&e), [(22, 9), (23, 9), (23, 9)]);
    e.snapshot("split_three_panes");
    e.keys("ctrl+w w");
    assert_eq!(e.app().doc().name(), "one.txt");
    e.keys("ctrl+w c");
    assert_eq!(e.app().doc().name(), "two.txt");
    assert_eq!(pane_sizes(&e), [(34, 9), (35, 9)]);
}

#[test]
fn a_pane_of_a_split_can_be_split_the_other_way() {
    let mut e = project_of_size(80, 16);
    e.keys("ctrl+w v");
    open(&mut e, "two.txt");
    e.keys("ctrl+w s");
    open(&mut e, "three.txt");
    e.snapshot("split_nested");
    assert_eq!(pane_sizes(&e), [(39, 15), (40, 7), (40, 8)]);
    let mut order = Vec::new();
    for _ in 0..3 {
        e.keys("ctrl+w w");
        order.push(e.app().doc().name());
    }
    assert_eq!(order, ["one.txt", "two.txt", "three.txt"]);
}

#[test]
fn alt_h_j_k_l_focus_the_pane_on_that_side() {
    let mut e = project_of_size(80, 16);
    e.keys("ctrl+w v");
    open(&mut e, "two.txt");
    e.keys("ctrl+w s");
    open(&mut e, "three.txt");
    e.keys("ctrl+w k");
    assert_eq!(e.app().doc().name(), "two.txt");
    e.keys("ctrl+w h");
    assert_eq!(e.app().doc().name(), "one.txt");
    e.keys("ctrl+g l").type_text("12\n").keys("ctrl+w l");
    assert_eq!(e.app().doc().name(), "three.txt");
    e.keys("ctrl+w j");
    e.assert_screen_contains("No pane on that side");
    assert_eq!(e.app().doc().name(), "three.txt");
}

#[test]
fn a_pane_is_resized_by_typing_columns_and_rows() {
    let mut e = project_of_size(80, 16);
    e.keys("ctrl+w v");
    open(&mut e, "two.txt");
    e.keys("ctrl+w s ctrl+w r");
    e.snapshot("split_resize_prompt");
    e.type_text("10,-2\n");
    assert_eq!(pane_sizes(&e), [(29, 15), (50, 9), (50, 6)]);
    e.snapshot("split_resized");
    e.keys("ctrl+w w ctrl+w r").type_text("-100\n");
    e.assert_screen_contains("No room for more: resized by -19,0");
    assert_eq!(pane_sizes(&e), [(10, 15), (69, 9), (69, 6)]);
    e.keys("ctrl+w r").type_text("wide\n");
    e.assert_screen_contains("wide: type the columns,rows to add");
    e.keys("ctrl+w =");
    assert_eq!(pane_sizes(&e), [(39, 15), (40, 7), (40, 8)]);
}

#[test]
fn a_new_pane_makes_its_neighbors_even_again() {
    let mut e = project();
    e.keys("ctrl+w v ctrl+w r").type_text("8\n");
    assert_eq!(pane_sizes(&e), [(26, 9), (43, 9)]);
    e.keys("ctrl+w v");
    assert_eq!(pane_sizes(&e), [(22, 9), (23, 9), (23, 9)]);
}

#[test]
fn a_split_needs_room_for_the_new_pane() {
    let mut e = project();
    e.keys("ctrl+w s ctrl+w s");
    assert_eq!(pane_sizes(&e), [(70, 3), (70, 3), (70, 3)]);
    e.keys("ctrl+w s");
    e.assert_screen_contains("No room for another pane");
    assert_eq!(e.app().panes.ids().len(), 3);
    e.resize(70, 8);
    assert_eq!(pane_sizes(&e), [(70, 7)]);
    e.assert_screen_lacks("two 1");
    e.resize(70, 10);
    assert_eq!(pane_sizes(&e).len(), 3);
}

#[test]
fn split_commands_without_a_split_say_how_to_split() {
    let mut e = project();
    e.keys("ctrl+w r");
    e.assert_screen_contains("Not split: ctrl+w splits the editor");
    e.keys("esc ctrl+w w");
    e.assert_screen_contains("Not split: ctrl+w splits the editor");
    e.keys("ctrl+w l");
    e.assert_screen_contains("Not split: ctrl+w splits the editor");
}

#[test]
fn the_same_file_in_both_panes_keeps_two_cursors() {
    let mut e = project();
    e.keys("ctrl+w v ctrl+g l").type_text("40\n");
    e.keys("ctrl+w w");
    assert_eq!(e.app().doc().cursor_line(), 0);
    e.keys("ctrl+w w");
    assert_eq!(e.app().doc().cursor_line(), 39);
    e.snapshot("split_same_file");
}

#[test]
fn scroll_sync_moves_every_pane() {
    let mut e = project();
    e.keys("ctrl+w v");
    open(&mut e, "two.txt");
    e.keys("ctrl+w v");
    open(&mut e, "three.txt");
    e.keys("ctrl+w y");
    for _ in 0..20 {
        e.keys("down");
    }
    e.snapshot("split_scroll_sync");
    e.keys("ctrl+w y");
    e.keys("ctrl+home");
    e.assert_screen_contains("one 14");
    e.assert_screen_contains("two 14");
    e.assert_screen_contains("three 1 ");
}

#[test]
fn the_wheel_scrolls_the_pane_under_the_mouse() {
    let mut e = project();
    e.keys("ctrl+w v");
    open(&mut e, "two.txt");
    e.scroll(10, 3, 1);
    e.assert_screen_contains("one 4");
    e.assert_screen_contains("two 1 ");
    assert_eq!(e.app().doc().name(), "two.txt");
}

#[test]
fn clicking_another_pane_focuses_it() {
    let mut e = project();
    e.keys("ctrl+w v");
    open(&mut e, "two.txt");
    e.click(10, 3);
    assert_eq!(e.app().doc().name(), "one.txt");
    assert_eq!(e.app().doc().cursor_line(), 2);
    e.click(50, 0);
    assert_eq!(e.app().doc().name(), "two.txt");
}

#[test]
fn closing_a_pane_asks_about_unsaved_changes() {
    let mut e = project();
    e.keys("ctrl+w v");
    open(&mut e, "two.txt");
    e.type_text("x").keys("ctrl+w c");
    e.assert_screen_contains("two.txt has unsaved changes");
    e.keys("esc");
    assert!(e.app().panes.is_split());
    e.keys("ctrl+w c s");
    assert!(!e.app().panes.is_split());
    assert_eq!(e.app().doc().name(), "one.txt");
    assert_eq!(e.app().docs.len(), 1);
    assert!(e.read_file("two.txt").starts_with("xtwo 1\n"));
}

#[test]
fn closing_the_other_panes_asks_about_each_unsaved_file() {
    let mut e = project_of_size(80, 16);
    e.keys("ctrl+w v");
    open(&mut e, "two.txt");
    e.type_text("x").keys("ctrl+w s");
    open(&mut e, "three.txt");
    e.type_text("y").keys("ctrl+w w ctrl+w o");
    e.assert_screen_contains("two.txt has unsaved changes");
    e.keys("d");
    e.assert_screen_contains("three.txt has unsaved changes");
    assert_eq!(e.app().panes.ids().len(), 2);
    e.keys("s");
    assert!(!e.app().panes.is_split());
    assert_eq!(e.app().doc().name(), "one.txt");
    assert_eq!(e.app().docs.len(), 1);
    assert!(e.read_file("two.txt").starts_with("two 1\n"));
    assert!(e.read_file("three.txt").starts_with("ythree 1\n"));
}

#[test]
fn a_file_shown_in_two_panes_can_be_left_in_one_without_saving() {
    let mut e = project();
    e.keys("ctrl+w v").type_text("x");
    open(&mut e, "two.txt");
    assert_eq!(e.app().doc().name(), "two.txt");
    e.keys("ctrl+w w");
    assert!(e.editor_text().starts_with("xone 1\n"));
    e.keys("ctrl+w c");
    e.assert_screen_contains("one.txt has unsaved changes");
}

#[test]
fn deleting_a_file_closes_its_panes() {
    let mut e = project();
    e.keys("ctrl+w v ctrl+w s");
    open(&mut e, "two.txt");
    e.command("file.delete").keys("y");
    assert_eq!(e.app().panes.ids().len(), 2);
    assert_eq!(e.app().doc().name(), "one.txt");
    e.command("file.delete").keys("y");
    assert!(!e.app().panes.is_split());
    assert_eq!(e.app().doc().name(), "untitled");
}
