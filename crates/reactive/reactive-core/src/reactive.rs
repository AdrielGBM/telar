//! A prop that follows state, in the one shape a call site never has to think about.

use std::rc::Rc;

use crate::{Memo, ReadSignal, RwSignal};

/// A value a widget re-reads every time it renders: a constant, a signal, or a derivation, erased into one type so the widget's field does not have to name which.
///
/// **What this is for.** A component that declares `RwSignal<T>` can only be fed a signal, so an application with a value computed from two services has nothing to hand it and re-implements the widget instead. One that declares `T` freezes at construction. `Reactive<T>` is the field type for a prop that must *read* state, and the call site writes whatever it has: `label:"Save"`, `label:name`, `label:total`.
///
/// **Reading, never writing.** A prop the widget writes back — a checkbox's `checked`, a field's `value` — stays `RwSignal<T>`, because there is nothing for a reading closure to write to. That distinction is the component's own declaration, which is the point: the caller writes the handle either way and the compiler decides what it meant.
///
/// **What converts into one is [`IntoReading`].** A value, a signal, a memo, another `Reactive`, and whatever a type says stands for one; a closure is the one case it cannot hold, since a closure type could be any of those too, and it gets [`Reactive::of`].
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

impl<T: Clone + 'static> Reactive<T> {
    /// What `read` yields, followed only when working it out read state: a constant when it read none, so a value that cannot change is held as itself. What `read` returns is anything that reads as a `T` ([`IntoReading`]), so a reading built to be kept (`Reactive::of(…)`, a signal) arrives as it was made.
    pub fn follow<U: IntoReading<T>>(read: impl Fn() -> U + 'static) -> Self {
        match crate::reads_state(&read) {
            (first, false) => first.into_reading(),
            (_, true) => Self::of(move || read().into_reading().get()),
        }
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

/// What reads as a `T`: the value itself, a reading of one ([`Reactive`], a signal, a memo), or a value its type says stands for one, as a colour stands for a tone. What a `Reactive<T>` prop takes, so a caller hands it whichever it has.
///
/// A trait of its own rather than `Into<Reactive<T>>`, because a type that stands for a `T` (a colour for a tone) cannot say so through `From`: neither `From` nor `Reactive` is its crate's to implement.
pub trait IntoReading<T> {
    fn into_reading(self) -> Reactive<T>;
}

impl<T> IntoReading<T> for T {
    fn into_reading(self) -> Reactive<T> {
        Reactive::Const(self)
    }
}

impl<T> IntoReading<T> for Reactive<T> {
    fn into_reading(self) -> Reactive<T> {
        self
    }
}

impl<T: Clone + 'static> IntoReading<T> for RwSignal<T> {
    fn into_reading(self) -> Reactive<T> {
        Reactive::of(move || self.get())
    }
}

impl<T: Clone + 'static> IntoReading<T> for ReadSignal<T> {
    fn into_reading(self) -> Reactive<T> {
        Reactive::of(move || self.get())
    }
}

impl<T: Clone + 'static> IntoReading<T> for Memo<T> {
    fn into_reading(self) -> Reactive<T> {
        Reactive::of(move || self.get())
    }
}

/// The literal a call site actually writes. Without it every `label:"Save"` would need a `.to_string()`.
impl IntoReading<String> for &'static str {
    fn into_reading(self) -> Reactive<String> {
        Reactive::Const(self.to_string())
    }
}

#[cfg(test)]
#[path = "reactive.test.rs"]
mod tests;
