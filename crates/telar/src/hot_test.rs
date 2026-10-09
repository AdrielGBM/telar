use super::*;

/// The build error shown in the window is a rustc-shaped code frame, and a frame is mostly `|` — which is exactly what the old protocol substituted for a newline. Round-tripping one proves the frame survives instead of being cut apart at every gutter.
#[test]
fn a_code_frame_survives_the_hot_reload_channel() {
    let frame =
        "error: mismatched types\n --> src/home.rsx:4\n  |\n4 | text \"{n}\" size:no\n  |\n";
    let escaped = frame
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "");
    assert!(
        !escaped.contains('\n'),
        "the wire carries one line per event"
    );
    assert_eq!(unescape_lines(&escaped), frame);
}

/// A backslash in the message (a Windows path, an escaped quote rustc quoted back) is not a line break.
#[test]
fn a_literal_backslash_is_not_read_as_an_escape() {
    let message = "cannot find `C:\\src\\home.rsx`";
    let escaped = message.replace('\\', "\\\\").replace('\n', "\\n");
    assert_eq!(unescape_lines(&escaped), message);
}

#[cfg(feature = "dev")]
#[test]
fn each_line_of_the_channel_is_the_event_its_prefix_names() {
    assert_eq!(
        hot_event("hot:/target/debug/libapp.so"),
        Some(HotEvent::Reload("/target/debug/libapp.so".into()))
    );
    assert_eq!(
        hot_event("err:line\\none"),
        Some(HotEvent::BuildError("line\none".to_string()))
    );
    assert_eq!(
        hot_event("goto:/preview/demo--card--a?args=label:%22Hi%22"),
        Some(HotEvent::Goto(
            "/preview/demo--card--a?args=label:%22Hi%22".to_string()
        ))
    );
    assert_eq!(hot_event("/target/debug/libapp.so"), None);
}
