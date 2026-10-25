use super::*;

// The M3 hazard: many surfaces share one UI thread, so a `Close` pushed by one window's title bar must land only in that window's queue — otherwise a sibling drains it and the wrong window closes.
#[test]
fn command_pushed_under_one_context_does_not_leak_to_another() {
    let root = WindowCommandContext::new();
    let child = WindowCommandContext::new();

    root.with_entered(|| {
        push_window_command(WindowCommand::Close);
    });
    child.with_entered(|| {
        assert!(
            take_window_commands().is_empty(),
            "a fresh context starts with no commands queued"
        );
    });
    root.with_entered(|| {
        assert_eq!(take_window_commands(), vec![WindowCommand::Close]);
    });
}

// Leaving an entry restores the queue that was active before it, so nested contexts unwind in order.
#[test]
fn dropping_a_guard_restores_the_previous_surfaces_queue() {
    let outer = WindowCommandContext::new();
    let inner = WindowCommandContext::new();

    outer.with_entered(|| {
        push_window_command(WindowCommand::Minimize);
        inner.with_entered(|| {
            push_window_command(WindowCommand::Close);
            assert_eq!(take_window_commands(), vec![WindowCommand::Close]);
        });
        assert_eq!(take_window_commands(), vec![WindowCommand::Minimize]);
    })
}
