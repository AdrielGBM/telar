use std::cell::{Cell, RefCell};
use std::rc::Rc;

use layout_core::AvailableSpace;
use platform_core::{ModifiersState, PointerButton, PointerSource};
use reactive_core::{Transaction, dispose_owner, owner_scope, signal};

use super::*;
use crate::context::{compute_layout, reset_layout_runtime};

fn press(x: f64, y: f64, button: PointerButton) -> Event {
    Event::PointerPressed {
        x,
        y,
        button,
        source: PointerSource::Mouse,
    }
}

fn release(x: f64, y: f64, button: PointerButton) -> Event {
    Event::PointerReleased {
        x,
        y,
        button,
        source: PointerSource::Mouse,
    }
}

fn to(x: f64, y: f64) -> Event {
    Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    }
}

fn key(named: NamedKey) -> Event {
    Event::KeyPressed {
        key: Key::Named(named),
        modifiers: ModifiersState::default(),
    }
}

struct Rig {
    card: StyledContainer,
    value: RwSignal<f32>,
    commits: Rc<RefCell<Vec<(f32, f32)>>>,
    ends: Rc<Cell<u32>>,
    cancels: Rc<Cell<u32>>,
}

/// A 100×100 box whose drag scrubs `value` by an awkward factor, so a restore that went through arithmetic instead of the snapshot would show in the bits.
fn rig(threshold: f32) -> (Rig, Transaction<f32>) {
    reset_layout_runtime();
    crate::focus::clear();
    let value = signal(0.1f32 + 0.2f32);
    let commits = Rc::new(RefCell::new(Vec::new()));
    let sink = commits.clone();
    let tx = Transaction::new(value).on_commit(move |b, a| sink.borrow_mut().push((*b, *a)));
    let ends = Rc::new(Cell::new(0));
    let cancels = Rc::new(Cell::new(0));
    let (end_sink, cancel_sink) = (ends.clone(), cancels.clone());
    let card = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .drag_threshold(threshold)
    .on_drag(move |x, _| {
        let _ = tx.preview(|v| *v = x * 0.037_3);
    })
    .on_drag_end(move |_, _| end_sink.set(end_sink.get() + 1))
    .on_drag_cancel(move || cancel_sink.set(cancel_sink.get() + 1))
    .drag_transaction(tx);
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    (
        Rig {
            card,
            value,
            commits,
            ends,
            cancels,
        },
        tx,
    )
}

#[test]
fn escape_mid_drag_restores_the_state_before_it_bit_for_bit() {
    let (mut rig, tx) = rig(0.0);
    let original = rig.value.peek().to_bits();

    rig.card
        .on_event(&press(10.0, 10.0, PointerButton::Primary));
    rig.card.on_event(&to(63.0, 10.0));
    assert_ne!(
        rig.value.peek().to_bits(),
        original,
        "the drag previews live"
    );

    assert_eq!(
        crate::dispatch_overlays(&key(NamedKey::Escape)),
        EventResult::Handled
    );
    assert_eq!(rig.value.peek().to_bits(), original);
    assert!(!tx.is_open());
    assert_eq!(rig.cancels.get(), 1);

    rig.card.on_event(&to(80.0, 10.0));
    rig.card
        .on_event(&release(80.0, 10.0, PointerButton::Primary));
    assert_eq!(
        rig.value.peek().to_bits(),
        original,
        "the stroke is over: later moves preview nothing"
    );
    assert!(
        rig.commits.borrow().is_empty(),
        "and the release commits nothing"
    );
    assert_eq!(rig.ends.get(), 0, "a cancelled drag does not also end");
    assert_eq!(
        crate::dispatch_overlays(&key(NamedKey::Escape)),
        EventResult::Ignored,
        "nothing is left for a second Escape to cancel"
    );
}

#[test]
fn the_secondary_button_mid_drag_reverts() {
    let (mut rig, _tx) = rig(0.0);
    let original = rig.value.peek().to_bits();

    rig.card
        .on_event(&press(10.0, 10.0, PointerButton::Primary));
    rig.card.on_event(&to(70.0, 40.0));
    assert_eq!(
        crate::dispatch_overlays(&press(70.0, 40.0, PointerButton::Secondary)),
        EventResult::Handled,
        "the abort is consumed, so no context menu opens under it"
    );
    assert_eq!(rig.value.peek().to_bits(), original);
    assert_eq!(rig.cancels.get(), 1);

    rig.card
        .on_event(&release(70.0, 40.0, PointerButton::Primary));
    assert!(rig.commits.borrow().is_empty());
}

#[test]
fn the_release_commits_once_with_the_state_before_and_after() {
    let (mut rig, tx) = rig(0.0);
    let before = rig.value.peek();

    rig.card
        .on_event(&press(10.0, 10.0, PointerButton::Primary));
    rig.card.on_event(&to(50.0, 10.0));
    rig.card
        .on_event(&release(50.0, 10.0, PointerButton::Primary));

    assert_eq!(*rig.commits.borrow(), vec![(before, 50.0 * 0.037_3)]);
    assert_eq!(rig.ends.get(), 1);
    assert!(!tx.is_open());
    assert_eq!(
        crate::dispatch_overlays(&key(NamedKey::Escape)),
        EventResult::Ignored,
        "a decided stroke cannot be cancelled after the fact"
    );
}

#[test]
fn a_click_under_the_threshold_opens_nothing() {
    let (mut rig, tx) = rig(4.0);
    rig.card
        .on_event(&press(10.0, 10.0, PointerButton::Primary));
    assert!(!tx.is_open(), "nothing has been dragged yet");
    rig.card
        .on_event(&release(10.0, 10.0, PointerButton::Primary));
    assert!(rig.commits.borrow().is_empty());
}

#[test]
fn losing_window_focus_mid_drag_reverts_instead_of_committing() {
    let (mut rig, _tx) = rig(0.0);
    let original = rig.value.peek().to_bits();

    rig.card
        .on_event(&press(10.0, 10.0, PointerButton::Primary));
    rig.card.on_event(&to(40.0, 10.0));
    rig.card
        .on_event(&Event::FocusChanged { is_focused: false });

    assert_eq!(rig.value.peek().to_bits(), original);
    assert!(rig.commits.borrow().is_empty());
    assert_eq!(rig.cancels.get(), 1);
}

#[test]
fn a_box_dropped_mid_drag_reverts() {
    let (mut rig, tx) = rig(0.0);
    let original = rig.value.peek().to_bits();

    rig.card
        .on_event(&press(10.0, 10.0, PointerButton::Primary));
    rig.card.on_event(&to(40.0, 10.0));
    drop(rig.card);

    assert_eq!(rig.value.peek().to_bits(), original);
    assert!(!tx.is_open());
    assert_eq!(
        crate::dispatch_overlays(&key(NamedKey::Escape)),
        EventResult::Ignored,
        "and it is no longer live to be cancelled"
    );
}

#[test]
fn disposing_the_transactions_owner_mid_drag_reverts_and_the_release_commits_nothing() {
    reset_layout_runtime();
    crate::focus::clear();
    let value = signal(5.0f32);
    let commits = Rc::new(Cell::new(0));
    let scope = owner_scope();
    let sink = commits.clone();
    let tx = Transaction::new(value).on_commit(move |_, _| sink.set(sink.get() + 1));
    let owner = scope.id();
    drop(scope);
    let mut card = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(100.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_drag(move |x, _| {
        let _ = tx.preview(|v| *v = x);
    })
    .drag_transaction(tx);
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    card.on_event(&press(10.0, 10.0, PointerButton::Primary));
    card.on_event(&to(40.0, 10.0));
    dispose_owner(owner);
    assert_eq!(value.peek(), 5.0);

    card.on_event(&to(60.0, 10.0));
    card.on_event(&release(60.0, 10.0, PointerButton::Primary));
    assert_eq!(value.peek(), 5.0, "a disposed transaction previews nothing");
    assert_eq!(commits.get(), 0);
}

#[test]
fn a_drag_on_a_transaction_already_open_joins_it() {
    let (mut rig, tx) = rig(0.0);
    let original = rig.value.peek();
    tx.begin().unwrap();

    rig.card
        .on_event(&press(10.0, 10.0, PointerButton::Primary));
    rig.card.on_event(&to(30.0, 10.0));
    assert_eq!(
        crate::dispatch_overlays(&key(NamedKey::Escape)),
        EventResult::Ignored,
        "the stroke is not its own to cancel"
    );
    rig.card
        .on_event(&release(30.0, 10.0, PointerButton::Primary));
    assert!(tx.is_open(), "nor its own to commit");
    assert!(rig.commits.borrow().is_empty());

    tx.revert().unwrap();
    assert_eq!(rig.value.peek(), original, "whoever opened it decides");
}
