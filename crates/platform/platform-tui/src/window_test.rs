use super::*;

#[test]
fn size_is_reported_in_logical_pixels() {
    let w = TuiWindow::new(80, 24, 8.0, 16.0);
    assert_eq!(w.width(), 640);
    assert_eq!(w.height(), 384);
}

#[test]
fn a_resize_is_visible_through_a_clone() {
    let w = TuiWindow::new(80, 24, 8.0, 16.0);
    let clone = w.clone();
    w.set_grid(100, 30);
    assert_eq!(clone.width(), 800);
}

#[test]
fn a_redraw_request_is_taken_once() {
    let w = TuiWindow::new(80, 24, 8.0, 16.0);
    assert!(
        w.take_redraw_request(),
        "a fresh window owes its first frame"
    );
    assert!(
        !w.take_redraw_request(),
        "nothing has asked for a redraw yet"
    );
    w.request_redraw();
    assert!(w.take_redraw_request(), "the request is there to take");
}

#[test]
fn a_title_is_written_as_one_window_title_sequence() {
    assert_eq!(title_sequence("Credits — Portfolio"), "\x1b]0;Credits — Portfolio\x07");
}

#[test]
fn a_control_character_cannot_end_the_title_sequence_early() {
    assert_eq!(title_sequence("a\x07b\x1b]0;c\nd"), "\x1b]0;ab]0;cd\x07");
}
