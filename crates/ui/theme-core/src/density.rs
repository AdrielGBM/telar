//! One ambient answer to "how big are the controls here", which every catalogue component interprets for itself.
//!
//! The alternative is a size matrix: a `size` prop on every component, and a table of what each of its parts measures at each of them. That is N × M numbers to keep in step, and every one of them is a decision the component has already made once — a button's padding is 1.75 spacing units *whatever* size it is.
//!
//! So this scales the bases instead. A control size does not say "a small button is 24px tall"; it says the unit everything is derived from is smaller here, and each component's own proportions carry that through unchanged. One value to thread, N interpretations, and none of them written down twice.
//!
//! It is a signal like the theme, so a change re-runs the paint closures that read it — and, through `StyledContainer::styled_by`, re-resolves the layout styles derived from it too.
//!
//! The value is the thread's unless the active surface carries one of its own, which is how a canvas shows a dense variant beside the regular chrome around it, and a subtree can carry one of its own over both through a [`ScopedControlSize`].

use reactive_core::{RwSignal, signal};

/// How large the controls in this part of the tree are, as a preference the *container* expresses and each control interprets, not a size any one of them is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlSize {
    /// Dense chrome — a toolbar, an inspector, a status bar.
    Mini,
    Small,
    #[default]
    Regular,
    /// A touch target, or a control that is the point of the screen it is on.
    Large,
}

impl ControlSize {
    /// What the theme's metric bases are multiplied by here. Radius is deliberately not among them: a smaller control is smaller, not flatter — the corner is the design language's, and it does not change with the size of the thing wearing it.
    pub fn scale(self) -> f32 {
        match self {
            ControlSize::Mini => 0.75,
            ControlSize::Small => 0.875,
            ControlSize::Regular => 1.0,
            ControlSize::Large => 1.25,
        }
    }
}

reactive_core::surface_scoped! {
    /// The active surface's own control size, in place of the thread's while it has one.
    scoped control_size: ControlSize = ControlSize::Regular;
    context ControlSizeContext, ControlSizeGuard;
}

/// Sets the ambient control size of every surface without one of its own. Reactive: everything that read it re-runs, so a switch re-spaces the controls already on screen rather than waiting for whatever rebuilds them.
pub fn set_control_size(size: ControlSize) {
    control_size().set(size);
}

/// Gives the active surface a control size of its own, or with `None` hands it back to the thread's [`set_control_size`].
pub fn set_surface_control_size(size: Option<ControlSize>) {
    control_size().set_surface(size.as_ref());
}

/// Reactive read of the active surface's own control size, `None` where it follows the thread's.
pub fn use_surface_control_size() -> Option<ControlSize> {
    control_size().surface()
}

/// The control size in force here, subscribing the caller: the nearest [`ScopedControlSize`] provided above the current owner, else the active surface's, else the thread's.
pub fn use_control_size() -> ControlSize {
    match nearest_control_size() {
        Some(scoped) => scoped.get(),
        None => control_size().get(),
    }
}

/// Non-reactive read of the control size in force here, resolved like [`use_control_size`], for event handlers and the layout pass.
pub fn current_control_size() -> ControlSize {
    match nearest_control_size() {
        Some(scoped) => scoped.0.peek(),
        None => control_size().peek(),
    }
}

/// The factor the catalogue's metric bases carry here — [`use_control_size`] resolved to a number, which is the only form a component ever needs it in.
pub fn control_scale() -> f32 {
    use_control_size().scale()
}

/// A control size for one subtree, switchable in place: once [provided](Self::provide) to an owner, every read under that owner — in paint, layout and handlers that re-enter it later — resolves this size instead of the surface's or the thread's, and [`set`](Self::set) re-runs only the readers that resolved it.
///
/// What lets a form be [`Large`](ControlSize::Large) beside a toolbar that stays [`Regular`](ControlSize::Regular) in the same window. The nearest scope wins, a scope beats the surface's size, and the surface's beats the thread's.
#[derive(Clone, Copy)]
pub struct ScopedControlSize(RwSignal<ControlSize>);

impl ScopedControlSize {
    /// A scope belonging to the current owner, which is what frees it.
    pub fn new(size: ControlSize) -> Self {
        Self(signal(size))
    }

    /// Changes the size, re-running whatever resolved this scope. Setting the size it already holds re-runs nobody.
    pub fn set(&self, size: ControlSize) {
        if self.0.peek() != size {
            self.0.set(size);
        }
    }

    /// The scope's size, read reactively.
    pub fn get(&self) -> ControlSize {
        self.0.get()
    }

    /// Makes this the control size of everything built under the current owner, shadowing any provided above it.
    pub fn provide(self) {
        reactive_core::provide_context(ProvidedSize(self));
    }
}

impl From<ControlSize> for ScopedControlSize {
    fn from(size: ControlSize) -> Self {
        Self::new(size)
    }
}

/// A newtype so the key in an owner's context is this module's alone.
struct ProvidedSize(ScopedControlSize);

/// The nearest control size provided at or above the current owner, or `None` where the surface's or the thread's applies.
pub fn nearest_control_size() -> Option<ScopedControlSize> {
    reactive_core::with_context::<ProvidedSize, _>(|provided| provided.0)
}

#[cfg(test)]
#[path = "density_test.rs"]
mod tests;
