use yate::testing::Harness;

#[test]
fn a_newer_release_is_announced_in_the_status_bar() {
    let mut e = Harness::builder()
        .size(100, 5)
        .file("a.txt", "hello\n")
        .open("a.txt")
        .start();
    assert_eq!(e.app().update_available, None);
    e.update_check(Some("9.9.9"));
    assert_eq!(e.app().update_available.as_deref(), Some("9.9.9"));
    e.snapshot("update_available");
}

#[test]
fn no_message_when_yate_is_up_to_date() {
    let mut e = Harness::builder()
        .size(70, 5)
        .file("a.txt", "hello\n")
        .open("a.txt")
        .start();
    e.update_check(None);
    assert_eq!(e.app().update_available, None);
    e.assert_screen_lacks("available");
}

#[test]
fn yate_json_turns_the_update_check_off() {
    let e = Harness::builder()
        .home_file("yate.json", r#"{ "options": { "updateCheck": false } }"#)
        .start();
    assert!(!e.app().options.update_check);
}
