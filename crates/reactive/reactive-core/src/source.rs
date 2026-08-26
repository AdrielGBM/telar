//! Reading a value that may or may not be a signal.

use crate::{Memo, ReadSignal, RwSignal, memo};

/// Anything a derivation — or a widget — can read reactively: either handle on a signal, or a value already
/// derived once.
///
/// One trait rather than a `derive`/`derive_from`/`map` family, because the difference between them was never
/// about behaviour: a widget reading a service, a read handle, or something derived all want the same thing,
/// and three spellings of it only meant picking the wrong one and chasing a type error.
///
/// A widget that takes `impl Source<Value = T>` instead of `RwSignal<T>` can be fed a derivation. One that
/// takes the signal cannot, and that is how a catalogue ends up re-implemented next to it — a card wanting a
/// percentage computed from two services has nothing to hand a widget that insists on a signal it can write.
pub trait Source {
    type Value;
    fn read(&self) -> Self::Value;
}

impl<T: Clone + 'static> Source for RwSignal<T> {
    type Value = T;
    fn read(&self) -> T {
        self.get()
    }
}

impl<T: Clone + 'static> Source for ReadSignal<T> {
    type Value = T;
    fn read(&self) -> T {
        self.get()
    }
}

impl<T: Clone + 'static> Source for Memo<T> {
    type Value = T;
    fn read(&self) -> T {
        self.get()
    }
}

/// A plain value reads as itself, so a widget taking a [`Source`] still accepts a constant without the caller
/// wrapping it in a signal that will never change.
impl Source for f32 {
    type Value = f32;
    fn read(&self) -> f32 {
        *self
    }
}

impl Source for bool {
    type Value = bool;
    fn read(&self) -> bool {
        *self
    }
}

/// A value derived from another, recomputed when its source moves.
///
/// **A derivation is a [`Memo`], never a signal written by an effect**, though the reason has changed. It used
/// to be that an unbound `effect(…)` deregistered where it was made and the derived value never moved again.
/// Both now belong to the owner that built them and live exactly as long, so what is left is the plain one: a
/// memo *is* the derived value, where a signal written by an effect is a second copy of it that has to be
/// kept in step.
pub fn derive<S, U>(source: S, map: impl Fn(S::Value) -> U + 'static) -> Memo<U>
where
    S: Source + 'static,
    U: PartialEq + 'static,
{
    memo(move || map(source.read()))
}

/// [`derive`] over two sources, recomputed when either moves — a label that reads a level and whether it is
/// charging, and has to follow both.
pub fn derive_pair<A, B, U>(
    first: A,
    second: B,
    map: impl Fn(A::Value, B::Value) -> U + 'static,
) -> Memo<U>
where
    A: Source + 'static,
    B: Source + 'static,
    U: PartialEq + 'static,
{
    memo(move || map(first.read(), second.read()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{reset_runtime, signal};

    #[test]
    fn a_derived_value_follows_its_source() {
        reset_runtime();
        let source = signal(2i32);
        let doubled = derive(source, |n| n * 2);
        assert_eq!(doubled.get(), 4, "seeded from the source, not a default");
        source.set(5);
        assert_eq!(doubled.get(), 10);
    }

    #[test]
    fn a_pair_recomputes_when_either_half_moves() {
        reset_runtime();
        let level = signal(10i32);
        let charging = signal(false);
        let label = derive_pair(
            level.read_only(),
            charging.read_only(),
            |level, charging| format!("{level}{}", if charging { "+" } else { "" }),
        );
        assert_eq!(label.get(), "10");
        charging.set(true);
        assert_eq!(label.get(), "10+");
        level.set(11);
        assert_eq!(label.get(), "11+");
    }

    /// The regression this exists for. Deriving through a signal written by an effect seeds correctly and then
    /// goes dead the moment the handle drops.
    #[test]
    fn a_derivation_outlives_the_call_that_made_it() {
        reset_runtime();
        let source = signal(1i32);
        let derived = derive(source, |n| n * 10);
        let read: Box<dyn Fn() -> i32> = Box::new(move || derived.get());
        source.set(7);
        assert_eq!(read(), 70, "whatever holds the derivation keeps it alive");
    }

    /// A constant is a source too, so widening a widget's parameter does not cost every caller a signal.
    #[test]
    fn a_plain_value_reads_as_itself() {
        assert_eq!(Source::read(&0.5f32), 0.5);
        assert!(Source::read(&true));
    }

    /// A derivation is a source, which is what lets one feed a widget that used to insist on a signal.
    #[test]
    fn a_derivation_is_itself_a_source() {
        reset_runtime();
        let source = signal(3i32);
        let once = derive(source, |n| n + 1);
        let twice = derive(once, |n| n * 2);
        assert_eq!(twice.get(), 8);
        source.set(4);
        assert_eq!(twice.get(), 10);
    }
}
