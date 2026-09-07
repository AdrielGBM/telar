use std::cell::Cell;

use super::*;

fn reset() {
    STACK.with(|s| s.borrow_mut().clear());
}

#[test]
fn dismisses_the_most_recently_opened_first() {
    reset();
    let log = Rc::new(RefCell::new(Vec::new()));
    for name in ["dialog", "drawer"] {
        let log = log.clone();
        register_dismiss(Rc::new(move || log.borrow_mut().push(name)));
    }
    assert_eq!(dismiss_depth(), 2);

    assert!(dismiss_top(), "the newest entry is the one that goes first");
    assert_eq!(*log.borrow(), vec!["drawer"], "the last opened goes first");
    assert!(dismiss_top(), "and then the one under it");
    assert_eq!(*log.borrow(), vec!["drawer", "dialog"]);

    assert!(!dismiss_top(), "an empty stack reports nothing dismissed");
    assert_eq!(dismiss_depth(), 0);
}

#[test]
fn withdrawing_out_of_order_skips_that_entry() {
    reset();
    let hit = Rc::new(Cell::new(0));
    let first = {
        let hit = hit.clone();
        register_dismiss(Rc::new(move || hit.set(hit.get() + 1)))
    };
    let log = Rc::new(RefCell::new(Vec::new()));
    {
        let log = log.clone();
        register_dismiss(Rc::new(move || log.borrow_mut().push("top")));
    }
    unregister_dismiss(first);
    assert_eq!(dismiss_depth(), 1);

    assert!(dismiss_top(), "the top entry still dismisses normally");
    assert_eq!(*log.borrow(), vec!["top"]);
    assert_eq!(hit.get(), 0, "the withdrawn entry is never invoked");
    assert!(
        !dismiss_top(),
        "the withdrawn entry is gone, so there is nothing left to dismiss"
    );
}

#[test]
fn escape_dismisses_only_when_nothing_holds_focus() {
    use platform_core::{Event, Key, ModifiersState, NamedKey};

    reset();
    crate::focus::clear();
    let closed = Rc::new(Cell::new(false));
    {
        let closed = closed.clone();
        register_dismiss(Rc::new(move || closed.set(true)));
    }
    let esc = Event::KeyPressed {
        key: Key::Named(NamedKey::Escape),
        modifiers: ModifiersState::default(),
    };

    let id = crate::focus::next_id();
    crate::focus::register_as(id, crate::focus::FocusKind::Widget);
    crate::focus::request(id);
    assert_eq!(crate::dispatch_overlays(&esc), crate::EventResult::Ignored);
    assert!(!closed.get(), "the focused field consumes the first Escape");

    crate::focus::clear();
    assert_eq!(crate::dispatch_overlays(&esc), crate::EventResult::Handled);
    assert!(
        closed.get(),
        "with nothing holding focus, escape reaches the stack"
    );
    crate::focus::unregister(id);
}

#[test]
fn a_handler_that_reenters_the_stack_is_safe() {
    reset();
    let id = Rc::new(Cell::new(None::<DismissId>));
    let inner = id.clone();
    let handle = register_dismiss(Rc::new(move || {
        if let Some(i) = inner.get() {
            unregister_dismiss(i);
        }
    }));
    id.set(Some(handle));
    assert!(
        dismiss_top(),
        "a handler that pushes while unwinding must not corrupt the stack"
    );
    assert_eq!(dismiss_depth(), 0);
}
