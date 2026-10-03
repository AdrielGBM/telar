use platform_core::{Cursor, Window};

use super::{HeadlessWindow, TitleSink};

#[test]
fn a_cursor_request_is_recorded_and_seen_through_every_clone() {
    let window = HeadlessWindow::new(10, 10);
    let seen = window.clone();
    assert_eq!(seen.cursor(), Cursor::Default);

    window.set_cursor(Cursor::EwResize);
    assert_eq!(seen.cursor(), Cursor::EwResize);

    window.set_cursor(Cursor::Default);
    assert_eq!(seen.cursor(), Cursor::Default);
}

#[test]
fn a_title_is_kept_and_every_one_is_recorded() {
    let window = HeadlessWindow::new(8, 8);
    assert_eq!(window.title(), "");
    let sink = TitleSink::default();
    window.record_titles_into(sink.clone());
    window.set_title("Portfolio");
    window.clone().set_title("Credits — Portfolio");
    assert_eq!(window.title(), "Credits — Portfolio");
    assert_eq!(*sink.lock().unwrap(), ["Portfolio", "Credits — Portfolio"]);
}
