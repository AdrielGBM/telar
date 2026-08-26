//! One ambient answer to "how big are the controls here", which every catalogue component interprets for
//! itself.
//!
//! The alternative is a size matrix: a `size` prop on every component, and a table of what each of its parts
//! measures at each of them. That is N × M numbers to keep in step, and every one of them is a decision the
//! component has already made once — a button's padding is 1.75 spacing units *whatever* size it is.
//!
//! So this scales the bases instead. A control size does not say "a small button is 24px tall"; it says the
//! unit everything is derived from is smaller here, and each component's own proportions carry that through
//! unchanged. One value to thread, N interpretations, and none of them written down twice.
//!
//! It is a signal like the theme, so a change re-runs the paint closures that read it — and, through
//! `StyledContainer::styled_by`, re-resolves the layout styles derived from it too.

use std::mem::ManuallyDrop;

use reactive_core::{RwSignal, detached, signal};

/// How large the controls in this part of the tree are, in the sense SwiftUI's `controlSize` means: a
/// preference the *container* expresses and each control interprets, not a size any one of them is given.
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
    /// What the theme's metric bases are multiplied by here. Radius is deliberately not among them: a smaller
    /// control is smaller, not flatter — the corner is the design language's, and it does not change with the
    /// size of the thing wearing it.
    pub fn scale(self) -> f32 {
        match self {
            ControlSize::Mini => 0.75,
            ControlSize::Small => 0.875,
            ControlSize::Regular => 1.0,
            ControlSize::Large => 1.25,
        }
    }
}

thread_local! {
    // `ManuallyDrop` for the same reason the theme signals are: no TLS destructor, cleanup goes through the
    // runtime being dropped.
    static CONTROL_SIZE: ManuallyDrop<RwSignal<ControlSize>> =
        ManuallyDrop::new(detached(|| signal(ControlSize::Regular)));
}

/// Sets the ambient control size. Reactive: everything that read it re-runs, so a switch re-spaces the
/// controls already on screen rather than waiting for whatever rebuilds them.
pub fn set_control_size(size: ControlSize) {
    CONTROL_SIZE.with(|s| s.set(size));
}

/// The ambient control size, subscribing the caller.
pub fn use_control_size() -> ControlSize {
    CONTROL_SIZE.with(|s| s.get())
}

/// The factor the catalogue's metric bases carry here — [`use_control_size`] resolved to a number, which is
/// the only form a component ever needs it in.
pub fn control_scale() -> f32 {
    use_control_size().scale()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ambient_size_scales_the_bases_and_regular_leaves_them_alone() {
        assert_eq!(ControlSize::Regular.scale(), 1.0);
        assert!(ControlSize::Mini.scale() < 1.0);
        assert!(ControlSize::Large.scale() > 1.0);

        assert_eq!(use_control_size(), ControlSize::Regular, "the default");
        set_control_size(ControlSize::Mini);
        assert_eq!(control_scale(), ControlSize::Mini.scale());
        set_control_size(ControlSize::Regular);
    }
}
