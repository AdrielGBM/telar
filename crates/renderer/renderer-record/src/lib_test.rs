use super::*;

#[test]
fn a_recorder_keeps_each_frame_it_was_handed() {
    let recording = Recording::new();
    let mut backend = recording.backend();

    backend.begin_frame(320, 240, 2.0, 7).unwrap();
    backend
        .render_frame(&[DrawCommand::PopClip], Some(Color::BLACK))
        .unwrap();

    let frame = recording.last_frame().expect("a frame was recorded");
    assert_eq!((frame.width, frame.height), (320, 240));
    assert_eq!(frame.generation, 7);
    assert_eq!(frame.commands.len(), 1);
    assert_eq!(recording.frame_count(), 1);
}

// Accepting a frame with no `begin_frame` would make the recording lie about the size it was drawn at.
#[test]
fn commands_without_a_begun_frame_are_an_error() {
    let recording = Recording::new();
    let mut backend = recording.backend();
    assert!(
        backend.render_frame(&[DrawCommand::PopClip], None).is_err(),
        "a command outside a frame is an error, not a silent no-op"
    );
    assert_eq!(recording.frame_count(), 0);
}
