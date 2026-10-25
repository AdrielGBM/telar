//! [`FrameFocus`]: the one stop a frame holds in the tab order of the surface around it for a surface it draws, whose own focus answers behind it.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use geometry_core::Transform;
use layout_core::NodeId;
use platform_core::{Event, Key, NamedKey, Role, TextInput};
use reactive_core::{OwnerId, dispose_owner, effect, root_scope};

use crate::focus::{self, FocusId, FocusKind, Keyboard, Travel};
use crate::surface_context::HomeSurface;

/// The surface a frame draws, as its focus stop drives it: a nested [`SurfaceCanvas`](crate::SurfaceCanvas) or a [`ForeignSurface`](crate::ForeignSurface) alike.
pub(crate) trait Inside: Keyboard {
    fn arrive(&self, backwards: bool);

    /// Whether focus stepped out past either end; see [`focus::tab`].
    fn tab(&self, key: &Event) -> bool;

    fn blur(&self);

    fn holds_focus(&self) -> bool;

    /// The caret inside, in the drawn surface's coordinates, and how those map into the stop's box.
    fn text_input(&self) -> Option<(TextInput, Transform)>;
}

/// Where a key, or the input method's text, goes from a frame's focus stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyRoute {
    /// Tab or Shift+Tab while the stop holds the keys: for [`FrameFocus::tab`].
    Tab,
    /// Any other key while the stop holds the keys: the drawn surface's.
    Inside,
    /// The stop does not hold the keys, so the drawn surface hears none of them.
    Outside,
}

impl KeyRoute {
    /// `None` for an event that is neither a key nor the input method's text.
    pub fn of(event: &Event, holds_keys: bool) -> Option<Self> {
        let tab = match event {
            Event::KeyPressed { key, .. } => *key == Key::Named(NamedKey::Tab),
            Event::KeyReleased { .. } | Event::Ime(_) => false,
            _ => return None,
        };
        Some(match (holds_keys, tab) {
            (false, _) => Self::Outside,
            (true, true) => Self::Tab,
            (true, false) => Self::Inside,
        })
    }
}

/// The focus stop of a frame that draws another surface in its box, so the surface around has exactly one focus owner: the keyboard arrives inside through it, moves through and out of it on [`tab`](Self::tab), losing it clears focus inside, and while it holds focus the input method, shortcut tables and the dismiss stack hear about the control focused inside. Withdrawn from the surface it was registered on when dropped.
pub struct FrameFocus(Rc<Stop>);

pub(crate) struct Stop {
    id: FocusId,
    home: HomeSurface,
    inside: Rc<dyn Inside>,
    // A root of its own: the stop outlives the scope that registers it, and what follows its focus has to live exactly as long.
    owner: OwnerId,
}

impl FrameFocus {
    /// Registers the stop for the frame's box at `node`, driving `inside`.
    pub(crate) fn register(node: NodeId, inside: impl Inside + 'static) -> Self {
        let id = focus::next_id();
        focus::register_with_role(id, FocusKind::Widget, node, Role::Group);
        let inside = Rc::new(inside);
        focus::delegate_keyboard(id, inside.clone());
        let inside: Rc<dyn Inside> = inside;
        focus::provide_text_input(id, {
            let inside = Rc::clone(&inside);
            move || {
                let (input, placement) = inside.text_input()?;
                nested_text_input(id, node, placement, input)
            }
        });
        let scope = root_scope();
        follow_outer_focus(id, Rc::clone(&inside));
        let owner = scope.id();
        drop(scope);
        Self(Rc::new(Stop {
            id,
            home: HomeSurface::here(),
            inside,
            owner,
        }))
    }

    pub fn id(&self) -> FocusId {
        self.0.id
    }

    /// Whether the key being dispatched belongs to the surface the frame draws. See [`focus::is_key_target`].
    pub fn holds_keys(&self) -> bool {
        self.0.in_home(|| focus::is_key_target(self.0.id)) == Some(true)
    }

    pub fn key_route(&self, event: &Event) -> Option<KeyRoute> {
        KeyRoute::of(event, self.holds_keys())
    }

    /// Tab, or Shift+Tab, `key`, routed [`KeyRoute::Tab`]: through the surface inside and, past either end of it, on through the surface around from the stop.
    pub fn tab(&self, key: &Event) {
        let backwards = matches!(key, Event::KeyPressed { modifiers, .. } if modifiers.is_shift);
        if self.0.inside.tab(key) {
            self.0.in_home(|| {
                focus::step_towards(backwards);
                // The surface around wrapped round to this stop, its only one: carry on inside from the other end.
                if focus::is_focused(self.0.id) {
                    self.0.inside.arrive(backwards);
                }
            });
        }
    }

    /// Gives the stop focus once something inside holds it, however it got there, so the box that shows the focus is the one the keys reach.
    pub fn follow_inside(&self) {
        self.0.follow_inside();
    }

    pub(crate) fn stop(&self) -> Weak<Stop> {
        Rc::downgrade(&self.0)
    }
}

impl Stop {
    fn in_home<R>(&self, f: impl FnOnce() -> R) -> Option<R> {
        self.home.with_entered(f)
    }

    pub(crate) fn follow_inside(&self) {
        if self.inside.holds_focus() {
            self.in_home(|| {
                if !focus::is_focused(self.id) {
                    focus::request(self.id);
                }
            });
        }
    }
}

impl Drop for Stop {
    fn drop(&mut self) {
        self.in_home(|| {
            // Before what follows the stop goes, so whatever is drawn inside has its focus cleared as when the stop loses it.
            focus::release(self.id);
            dispose_owner(self.owner);
            focus::unregister(self.id);
        });
    }
}

/// Clears focus inside when the stop loses it, and when the keyboard brings it hands it inside, to the last focusable when Shift+Tab brought it.
fn follow_outer_focus(id: FocusId, inside: Rc<dyn Inside>) {
    let held_before = Cell::new(false);
    effect(move || {
        let held = focus::is_focused(id);
        let by_keyboard = held && focus::is_focus_visible(id);
        if held_before.replace(held) == held {
            return;
        }
        if !held {
            inside.blur();
        } else if by_keyboard {
            inside.arrive(focus::arrived_by(id) == Some(Travel::Backward));
        }
    });
}

/// `inside`, the caret of a field in a surface drawn in the box at `node` through `placement`, as the surface around shows it.
fn nested_text_input(
    owner: FocusId,
    node: NodeId,
    placement: Transform,
    inside: TextInput,
) -> Option<TextInput> {
    let input = TextInput {
        // Every surface numbers its fields from the same start, so the drawn surface's field is told apart by the stop that holds it.
        field: inside.field.rotate_left(32) ^ owner,
        caret: renderer_core::transform_clip_rect(placement.to_array(), inside.caret),
        secret: inside.secret,
    };
    crate::text_input::placed(node, input)
}

#[cfg(test)]
#[path = "frame_focus_test.rs"]
mod tests;
