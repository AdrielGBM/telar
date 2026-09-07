//! A prop that follows state, in the one shape a call site never has to think about.

use std::rc::Rc;

use crate::{Memo, ReadSignal, RwSignal};

/// A value a widget re-reads every time it renders: a constant, a signal, or a derivation, erased into one type so the widget's field does not have to name which.
///
/// **What this is for.** A component that declares `RwSignal<T>` can only be fed a signal, so an application with a value computed from two services has nothing to hand it and re-implements the widget instead. One that declares `T` freezes at construction. `Reactive<T>` is the field type for a prop that must *read* state, and the call site writes whatever it has: `label:"Save"`, `label:name`, `label:total`.
///
/// **Reading, never writing.** A prop the widget writes back — a checkbox's `checked`, a field's `value` — stays `RwSignal<T>`, because there is nothing for a reading closure to write to. That distinction is the component's own declaration, which is the point: the caller writes the handle either way and the compiler decides what it meant.
///
/// **Why the conversions are a list and not `impl Source`.** [`crate::Source`] is the same idea as a trait, and `impl<S: Source<Value = T>> From<S> for Reactive<T>` would be the tidy spelling — but it overlaps `From<T>` the moment a plain value implements `Source`, which `f32` and `bool` already do, and Rust has no way to prove two impls disjoint. So the conversions are enumerated. A closure is the one case the list cannot hold, for the same coherence reason, and it gets [`Reactive::of`].
///
/// **A constant is held as itself, not as a closure returning it.** Most props are given a literal, and boxing one would put an allocation and a virtual call on every `gap:8` in the corpus for a value that cannot change. The enum is what lets a prop be uniformly readable without charging for it.
pub enum Reactive<T> {
    Const(T),
    Read(Rc<dyn Fn() -> T>),
}

impl<T> Reactive<T> {
    /// A reading derived on the spot — `Reactive::of(move || a.get() + b.get())`.
    pub fn of(read: impl Fn() -> T + 'static) -> Self {
        Self::Read(Rc::new(read))
    }
}

impl<T: Clone> Reactive<T> {
    pub fn get(&self) -> T {
        match self {
            Self::Const(value) => value.clone(),
            Self::Read(read) => read(),
        }
    }
}

impl<T: Clone> Clone for Reactive<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Const(value) => Self::Const(value.clone()),
            Self::Read(read) => Self::Read(Rc::clone(read)),
        }
    }
}

impl<T: Default> Default for Reactive<T> {
    fn default() -> Self {
        Self::Const(T::default())
    }
}

impl<T> From<T> for Reactive<T> {
    fn from(value: T) -> Self {
        Self::Const(value)
    }
}

impl<T: Clone + 'static> From<RwSignal<T>> for Reactive<T> {
    fn from(signal: RwSignal<T>) -> Self {
        Self::of(move || signal.get())
    }
}

impl<T: Clone + 'static> From<ReadSignal<T>> for Reactive<T> {
    fn from(signal: ReadSignal<T>) -> Self {
        Self::of(move || signal.get())
    }
}

impl<T: Clone + 'static> From<Memo<T>> for Reactive<T> {
    fn from(memo: Memo<T>) -> Self {
        Self::of(move || memo.get())
    }
}

/// The literal a call site actually writes. Without it every `label:"Save"` would need a `.to_string()`, which is the six lines of preamble the old `string_fields` table existed to avoid.
impl From<&'static str> for Reactive<String> {
    fn from(text: &'static str) -> Self {
        Self::of(move || text.to_string())
    }
}

#[cfg(test)]
#[path = "reactive_test.rs"]
mod tests;
