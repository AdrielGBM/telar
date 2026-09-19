use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::*;
use crate::{dispose_owner, effect, owner_scope, signal};

#[test]
fn revert_restores_the_snapshot_bit_for_bit() {
    let value = signal(0.1f32 + 0.2f32);
    let original = value.peek().to_bits();
    let tx = Transaction::new(value);

    tx.begin().unwrap();
    tx.preview(|v| *v *= 3.7).unwrap();
    tx.preview(|v| *v += 1e-7).unwrap();
    assert_ne!(value.peek().to_bits(), original);

    tx.revert().unwrap();
    assert_eq!(value.peek().to_bits(), original);
    assert!(!tx.is_open());
}

#[test]
fn commit_answers_before_and_after_exactly_once() {
    let value = signal(10);
    let heard = Rc::new(RefCell::new(Vec::new()));
    let sink = heard.clone();
    let tx = Transaction::new(value).on_commit(move |b, a| sink.borrow_mut().push((*b, *a)));

    tx.begin().unwrap();
    tx.preview(|v| *v = 20).unwrap();
    tx.preview(|v| *v = 30).unwrap();

    assert_eq!(tx.commit(), Ok((10, 30)));
    assert_eq!(tx.commit(), Err(TransactionError::NotOpen));
    assert_eq!(tx.revert(), Err(TransactionError::NotOpen));
    assert_eq!(*heard.borrow(), vec![(10, 30)]);
    assert_eq!(value.peek(), 30, "a commit keeps what was previewed");
}

#[test]
fn a_preview_is_live_but_a_revert_of_nothing_writes_nothing() {
    let value = signal(1);
    let runs = Rc::new(Cell::new(0));
    let seen = runs.clone();
    effect(move || {
        value.get();
        seen.set(seen.get() + 1);
    });
    let tx = Transaction::new(value);

    tx.begin().unwrap();
    tx.revert().unwrap();
    assert_eq!(runs.get(), 1, "nothing previewed, nothing written");

    tx.begin().unwrap();
    tx.preview(|v| *v = 2).unwrap();
    assert_eq!(runs.get(), 2, "readers follow the preview");
}

#[test]
fn a_second_transaction_on_the_same_signal_is_refused() {
    let value = signal(0);
    let outer = Transaction::new(value);
    let inner = Transaction::new(value);

    outer.begin().unwrap();
    assert_eq!(outer.begin(), Err(TransactionError::AlreadyOpen));
    assert_eq!(inner.begin(), Err(TransactionError::AlreadyOpen));
    assert_eq!(inner.preview(|v| *v = 5), Err(TransactionError::NotOpen));
    assert_eq!(value.peek(), 0);

    outer.commit().unwrap();
    assert_eq!(inner.begin(), Ok(()), "released by the commit");
    inner.revert().unwrap();

    let other = Transaction::new(signal(0));
    other.begin().unwrap();
    assert_eq!(outer.begin(), Ok(()), "other signals are independent");
}

#[test]
fn an_outside_write_moves_the_base_instead_of_being_clobbered() {
    let value = signal(1);
    let tx = Transaction::new(value);

    tx.begin().unwrap();
    tx.preview(|v| *v = 2).unwrap();
    value.set(100);
    tx.revert().unwrap();
    assert_eq!(
        value.peek(),
        100,
        "a revert keeps what the other writer left"
    );

    tx.begin().unwrap();
    tx.preview(|v| *v = 2).unwrap();
    value.set(100);
    tx.preview(|v| *v += 1).unwrap();
    assert_eq!(tx.before(), Some(100));
    assert_eq!(
        tx.commit(),
        Ok((100, 101)),
        "and a commit reports only the gesture's change"
    );
}

#[test]
fn a_write_the_preview_itself_causes_is_the_previews() {
    let value = signal(0);
    effect(move || {
        if value.get() > 10 {
            value.set(10);
        }
    });
    let tx = Transaction::new(value);

    tx.begin().unwrap();
    tx.preview(|v| *v = 50).unwrap();
    assert_eq!(value.peek(), 10, "clamped by the effect");
    tx.revert().unwrap();
    assert_eq!(value.peek(), 0, "the clamp did not become the snapshot");
}

#[test]
fn disposing_the_owner_mid_gesture_reverts_and_releases_the_signal() {
    let value = signal(7);
    let scope = owner_scope();
    let tx = Transaction::new(value);
    let owner = scope.id();
    drop(scope);

    tx.begin().unwrap();
    tx.preview(|v| *v = 99).unwrap();
    dispose_owner(owner);

    assert_eq!(value.peek(), 7, "nobody confirmed the edit");
    assert_eq!(tx.commit(), Err(TransactionError::Disposed));
    let next = Transaction::new(value);
    assert_eq!(next.begin(), Ok(()), "the signal is free again");
}

#[test]
fn a_signal_disposed_mid_gesture_closes_the_transaction() {
    let scope = owner_scope();
    let value = signal(3);
    let owner = scope.id();
    drop(scope);
    let tx = Transaction::new(value);

    tx.begin().unwrap();
    dispose_owner(owner);
    assert_eq!(tx.preview(|v| *v = 4), Err(TransactionError::Disposed));
    assert!(!tx.is_open());
}

#[test]
fn the_commit_listener_subscribes_nothing() {
    let value = signal(0);
    let other = signal(0);
    let tx = Transaction::new(value).on_commit(move |_, _| {
        other.get();
    });
    let runs = Rc::new(Cell::new(0));
    let seen = runs.clone();
    let trigger = signal(false);
    effect(move || {
        seen.set(seen.get() + 1);
        if trigger.get() {
            tx.begin().unwrap();
            tx.commit().unwrap();
        }
    });
    trigger.set(true);
    let before = runs.get();
    other.set(1);
    assert_eq!(runs.get(), before);
}
