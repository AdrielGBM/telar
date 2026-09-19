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

mod transactions {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use layout_core::{AvailableSpace, LayoutStyle};
    use platform_core::{Event, Key, ModifiersState, NamedKey, PointerButton, PointerSource};
    use reactive_core::{RwSignal, Transaction, dispose_owner, owner_scope, signal};
    use renderer_core::RectStyle;

    use super::super::*;
    use crate::context::{compute_layout, reset_layout_runtime};
    use crate::{Component, EventResult, LayoutItem, StyledContainer, box_item};

    fn key(named: NamedKey) -> Event {
        Event::KeyPressed {
            key: Key::Named(named),
            modifiers: ModifiersState::default(),
        }
    }

    fn click(card: &mut StyledContainer, x: f64, y: f64) {
        for event in [
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
                source: PointerSource::Mouse,
            },
            Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
                source: PointerSource::Mouse,
            },
        ] {
            card.on_event(&event);
        }
    }

    struct Popover {
        open: RwSignal<bool>,
        value: RwSignal<i32>,
        tx: Transaction<i32>,
        commits: Rc<RefCell<Vec<(i32, i32)>>>,
    }

    fn popover() -> Popover {
        super::reset();
        crate::focus::clear();
        let open = signal(false);
        let value = signal(1);
        let commits = Rc::new(RefCell::new(Vec::new()));
        let sink = commits.clone();
        let tx = Transaction::new(value).on_commit(move |b, a| sink.borrow_mut().push((*b, *a)));
        register_transaction(open, tx);
        Popover {
            open,
            value,
            tx,
            commits,
        }
    }

    /// A full-window backdrop that closes the popover when pressed, around a 20×20 panel that swallows its own presses: the shape every anchored popover in the catalogue has.
    fn backdrop(open: RwSignal<bool>) -> StyledContainer {
        reset_layout_runtime();
        let panel = StyledContainer::new(
            LayoutStyle::new().width(20.0).height(20.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(|| {});
        let card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![box_item(panel)],
        )
        .unwrap()
        .on_press(move || open.set(false));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        card
    }

    #[test]
    fn an_outside_click_commits_exactly_once() {
        let p = popover();
        let mut card = backdrop(p.open);
        p.open.set(true);
        p.tx.preview(|v| *v = 7).unwrap();

        click(&mut card, 10.0, 10.0);
        assert!(p.open.get(), "a click on the panel is not outside it");

        click(&mut card, 80.0, 80.0);
        assert!(!p.open.get());
        assert_eq!(*p.commits.borrow(), vec![(1, 7)]);
        assert_eq!(dismiss_depth(), 0);

        click(&mut card, 80.0, 80.0);
        p.open.set(false);
        assert_eq!(
            p.commits.borrow().len(),
            1,
            "a closed popover decides nothing"
        );
        assert_eq!(p.value.get(), 7);
    }

    #[test]
    fn escape_reverts_to_the_snapshot_taken_on_open() {
        let p = popover();
        p.value.set(3);
        p.open.set(true);
        p.tx.preview(|v| *v = 40).unwrap();
        p.tx.preview(|v| *v += 2).unwrap();

        assert_eq!(
            crate::dispatch_overlays(&key(NamedKey::Escape)),
            EventResult::Handled
        );
        assert_eq!(p.value.get(), 3);
        assert!(!p.open.get());
        assert!(p.commits.borrow().is_empty());
        assert!(!p.tx.is_open());
    }

    #[test]
    fn enter_commits() {
        let p = popover();
        p.open.set(true);
        p.tx.preview(|v| *v = 9).unwrap();

        assert_eq!(
            crate::dispatch_overlays(&key(NamedKey::Enter)),
            EventResult::Handled
        );
        assert!(!p.open.get());
        assert_eq!(*p.commits.borrow(), vec![(1, 9)]);
    }

    #[test]
    fn enter_is_left_to_a_focused_control_and_to_a_top_that_takes_no_confirmation() {
        let p = popover();
        p.open.set(true);

        let id = crate::focus::next_id();
        crate::focus::register_as(id, crate::focus::FocusKind::Widget);
        crate::focus::request(id);
        assert_eq!(
            crate::dispatch_overlays(&key(NamedKey::Enter)),
            EventResult::Ignored
        );
        crate::focus::clear();
        crate::focus::unregister(id);

        let dialog = DismissRegistration::new(Rc::new(|| {}));
        assert_eq!(
            crate::dispatch_overlays(&key(NamedKey::Enter)),
            EventResult::Ignored,
            "a dialog above the popover is what Enter would be about"
        );
        drop(dialog);
        assert!(p.open.get());
        assert!(p.commits.borrow().is_empty());
    }

    #[test]
    fn reopening_takes_a_fresh_snapshot() {
        let p = popover();
        p.open.set(true);
        p.tx.preview(|v| *v = 2).unwrap();
        p.open.set(false);
        p.open.set(true);
        p.tx.preview(|v| *v = 5).unwrap();
        crate::dispatch_overlays(&key(NamedKey::Escape));
        assert_eq!(p.value.get(), 2, "back to where this opening began");
        assert_eq!(*p.commits.borrow(), vec![(1, 2)]);
    }

    #[test]
    fn disposing_the_popover_while_open_reverts() {
        super::reset();
        crate::focus::clear();
        let value = signal(4);
        let commits = Rc::new(Cell::new(0));
        let sink = commits.clone();
        let tx = Transaction::new(value).on_commit(move |_, _| sink.set(sink.get() + 1));
        let scope = owner_scope();
        let open = signal(true);
        register_transaction(open, tx);
        let owner = scope.id();
        drop(scope);

        tx.preview(|v| *v = 8).unwrap();
        dispose_owner(owner);

        assert_eq!(value.get(), 4, "nobody confirmed the edit");
        assert_eq!(commits.get(), 0);
        assert_eq!(dismiss_depth(), 0);
        assert!(!tx.is_open());
    }

    #[test]
    fn escape_mid_drag_in_a_popover_cancels_only_the_drag() {
        let p = popover();
        p.open.set(true);
        p.tx.preview(|v| *v = 6).unwrap();

        reset_layout_runtime();
        let offset = signal(0.0f32);
        let drag_tx = Transaction::new(offset);
        let mut handle = StyledContainer::new(
            LayoutStyle::new().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_drag(move |x, _| {
            let _ = drag_tx.preview(|v| *v = x);
        })
        .drag_transaction(drag_tx);
        compute_layout(
            handle.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        handle.on_event(&Event::PointerPressed {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        handle.on_event(&Event::PointerMoved {
            x: 50.0,
            y: 10.0,
            source: PointerSource::Mouse,
        });

        crate::dispatch_overlays(&key(NamedKey::Escape));
        assert_eq!(offset.get(), 0.0, "the drag went back");
        assert!(p.open.get(), "and the popover stayed");
        assert_eq!(p.value.get(), 6);

        crate::dispatch_overlays(&key(NamedKey::Escape));
        assert!(!p.open.get());
        assert_eq!(p.value.get(), 1);
    }
}
