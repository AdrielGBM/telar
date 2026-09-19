use platform_core::{Cursor, Window};

use super::HeadlessWindow;

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
