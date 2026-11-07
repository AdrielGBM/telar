//! How a tree takes the keyboard and lets it go.

use platform_core::{Event, Key, NamedKey, PointerButton};
use ui_tree::EventResult;

use crate::focus;

/// How a tree takes the keyboard and lets it go: a [`WindowRoot`](crate::WindowRoot) and a [`SurfaceCanvas`](crate::SurfaceCanvas) apply [`Standalone`](Self::Standalone), a [`NestedCanvas`](crate::NestedCanvas) [`Nested`](Self::Nested), to every event that reaches the tree past its overlays.
///
/// Both modes let the keyboard go when a primary press lands on nothing focusable: focus is only ever *taken*, by a press on a focusable or by Tab, so without this a field clicked away from keeps the caret, eats the keys and tells an application asking [`text_entry_focused`](crate::focus::text_entry_focused) that somebody is still typing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusMode {
    /// The tree holds a keyboard of its own, as a window's does: Tab with nothing focused moves to the first focusable, or Shift+Tab to the last, unless the tree takes the key itself, and Tab past the last focusable wraps.
    Standalone,
    /// The tree is one focus stop of a tree around it, which carries the keyboard in and out through [`arrive`](crate::NestedCanvas::arrive) and [`tab`](crate::NestedCanvas::tab): Tab past the last focusable, or Shift+Tab past the first, leaves nothing focused here instead of wrapping, and a modal up inside still holds Tab within itself.
    Nested,
}

impl FocusMode {
    /// Marks the surface entered now as this mode's.
    pub(crate) fn enter(self) {
        if self == Self::Nested {
            focus::host();
        }
    }

    /// Hands `event` to the tree through `dispatch`, applying this mode's rules on the way.
    pub(crate) fn dispatch(
        self,
        event: &Event,
        dispatch: impl FnOnce(&Event) -> EventResult,
    ) -> EventResult {
        // Before the press goes down the tree, so whatever is pressed takes focus back on the way through and only a press with nothing focusable under it leaves the keyboard unheld.
        if let Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            ..
        } = event
        {
            focus::blur_from_pointer(*x as f32, *y as f32);
        }
        match event {
            // Tab is answered by the box that holds focus, which leaves the first one: with nothing focused the key reaches nobody. Asked last, so a control that wants Tab for itself has already taken it.
            Event::KeyPressed {
                key: Key::Named(NamedKey::Tab),
                modifiers,
                ..
            } if self == Self::Standalone && focus::current().is_none() => {
                if dispatch(event) != EventResult::Handled {
                    focus::step_towards(modifiers.is_shift);
                }
                EventResult::Handled
            }
            _ => dispatch(event),
        }
    }
}
