//! [`ForeignSurface`]: content drawn in this process's frame by a world this process cannot enter, as a guest library's tree is, and the [`ForeignBoundary`] that marks it there.
//!
//! A surface nested in the frame ([`DrawCommand::PushSurface`]) is read by entering its world. A foreign one has no world here: its boxes, its focus and its fields live in a runtime of their own, so what a reader, a screen reader's action, the input method and the keyboard need of it comes from the foreign side, through [`ForeignSurface`], and is placed where the frame shows it exactly as an entered surface's is.

use std::rc::{Rc, Weak};

use geometry_core::Transform;
use layout_core::NodeId;
use platform_core::{
    AccessAction, AccessAudit, AccessId, AccessNode, ConsumedKeys, Event, FocusId, SurfaceKey,
    TextInput,
};
use renderer_core::{Color, DrawCommand};
use rustc_hash::{FxHashMap, FxHashSet};
use ui_tree::RenderNode;

use crate::focus::Keyboard;
use crate::frame_focus::{FrameFocus, Inside, Stop};

/// What a surface drawn here by a world this process cannot enter reports back about itself, each answer in the surface's own coordinates and its own ids, and what it does with the keyboard of the one focus stop the host keeps for it ([`ForeignBoundary::focus_stop`]).
pub trait ForeignSurface {
    /// What a screen reader is told about the surface, as [`accessibility::snapshot`](crate::accessibility::snapshot) reads one surface: a control of a surface nested in it is named by the key it gives that surface, in its own numbering.
    fn access_snapshot(&self) -> Vec<AccessNode>;

    /// Carries out a screen reader's `action` on the control `id` names in its [`access_snapshot`](Self::access_snapshot), as [`accessibility::perform_action`](crate::accessibility::perform_action) does for the surface entered.
    fn perform_access_action(&self, id: AccessId, action: AccessAction);

    /// Where the field that holds its keyboard has its caret, or `None` while no field does, as [`text_input`](crate::text_input) reports it for the surface entered.
    fn text_input(&self) -> Option<TextInput>;

    /// See [`focus::arrive`](crate::focus::arrive).
    fn arrive(&self, backwards: bool) -> bool;

    /// See [`focus::tab`](crate::focus::tab).
    fn tab(&self, key: &Event) -> bool;

    fn blur(&self);

    fn holds_focus(&self) -> bool;

    /// What [`focus::text_entry_focused`](crate::focus::text_entry_focused) answers inside, which the host asks while its stop holds focus.
    fn text_entry_focused(&self) -> bool;

    /// What [`focus::focused_keeps`](crate::focus::focused_keeps) answers inside, which the host asks while its stop holds focus.
    fn keeps(&self, keys: ConsumedKeys) -> bool;

    /// The surface checked where its world is, for an inspector; `None` for one that cannot check itself, which an inspector reads unchecked.
    fn access_audit(&self, _background: Option<Color>) -> Option<AccessAudit> {
        None
    }
}

/// A [`ForeignSurface`] registered on this thread under a key no surface handle takes, while this lives: what marks its frame where the host draws it, so every reader of the host's frame reads that frame as the foreign surface's.
pub struct ForeignBoundary {
    key: SurfaceKey,
    source: Weak<dyn ForeignSurface>,
}

impl ForeignBoundary {
    /// Registers `surface`, held weakly: once it is gone, it reports nothing and takes no action.
    pub fn new(surface: &Rc<dyn ForeignSurface>) -> Self {
        let source = Rc::downgrade(surface);
        let key = mint();
        REGISTRY.with(|registry| {
            registry.borrow_mut().insert(
                key,
                Registered {
                    owner: key,
                    guest: None,
                    source: source.clone(),
                    stop: Weak::new(),
                },
            )
        });
        Self { key, source }
    }

    /// The key its frame is marked with, which [`DrawCommand::PushSurface`] carries.
    pub fn key(&self) -> SurfaceKey {
        self.key
    }

    /// `frame`, a frame the foreign surface drew in its own coordinates, marked as its: place it with [`composite_surface`](crate::composite_surface).
    ///
    /// A surface nested in the foreign one is marked in the foreign world's numbering, where its handle may name a surface here; each is marked again under a key of its own here, kept for as long as the frames this draws carry it.
    pub fn boundary(&self, frame: impl IntoIterator<Item = DrawCommand>) -> RenderNode {
        RenderNode::group(self.marked(frame).into_iter().map(RenderNode::Primitive))
    }

    /// [`boundary`](Self::boundary) as the commands it draws, for a frame the foreign surface fills whole, as a hot-reloaded application fills its window.
    pub fn marked(&self, frame: impl IntoIterator<Item = DrawCommand>) -> Vec<DrawCommand> {
        let mut drawn = FxHashSet::default();
        let mut commands = vec![DrawCommand::PushSurface { surface: self.key }];
        commands.extend(frame.into_iter().map(|command| match command {
            DrawCommand::PushSurface { surface: guest } => {
                drawn.insert(guest);
                DrawCommand::PushSurface {
                    surface: self.nested_key(guest),
                }
            }
            command => command,
        }));
        commands.push(DrawCommand::PopSurface);
        REGISTRY.with(|registry| {
            registry.borrow_mut().retain(|_, registered| {
                registered.owner != self.key
                    || registered.guest.is_none_or(|guest| drawn.contains(&guest))
            })
        });
        commands
    }

    /// `event`, a box event or a screen reader's action the window reported inside a surface its frames mark ([`Event::InSurface`]), as the foreign world is to hear it: the event itself for one of the foreign surface's own boxes, or still inside the surface nested in it, named by its handle there. `None` for an event of any other surface.
    pub fn foreign_event(&self, event: &Event) -> Option<Event> {
        let Event::InSurface { surface, event } = event else {
            return None;
        };
        if *surface == self.key {
            return Some((**event).clone());
        }
        let guest = REGISTRY.with(|registry| {
            registry
                .borrow()
                .get(surface)
                .filter(|registered| registered.owner == self.key)
                .and_then(|registered| registered.guest)
        })?;
        Some(Event::InSurface {
            surface: guest,
            event: event.clone(),
        })
    }

    fn nested_key(&self, guest: SurfaceKey) -> SurfaceKey {
        nested_key_of(self.key, guest).unwrap_or_else(|| {
            let key = mint();
            REGISTRY.with(|registry| {
                registry.borrow_mut().insert(
                    key,
                    Registered {
                        owner: self.key,
                        guest: Some(guest),
                        source: self.source.clone(),
                        stop: Weak::new(),
                    },
                )
            });
            key
        })
    }

    /// Registers the one focus stop the host keeps for the surface, in the box at `node` it is drawn in, `placement` mapping the surface's coordinates into that box's; see [`FrameFocus`].
    pub fn focus_stop(
        &self,
        node: NodeId,
        placement: impl Fn() -> Transform + 'static,
    ) -> FrameFocus {
        let focus = FrameFocus::register(
            node,
            Placed {
                source: self.source.clone(),
                placement: Box::new(placement),
            },
        );
        REGISTRY.with(|registry| {
            if let Some(registered) = registry.borrow_mut().get_mut(&self.key) {
                registered.stop = focus.stop();
            }
        });
        focus
    }
}

/// Gives the focus stop of the foreign surface `key` marks, or of the one a surface `key` marks is nested in, focus once something inside holds it, as a screen reader's action there may have moved it.
pub(crate) fn follow_focus_inside(key: SurfaceKey) {
    let stop = REGISTRY.with(|registry| {
        let registry = registry.borrow();
        let owner = registry.get(&key)?.owner;
        registry.get(&owner)?.stop.upgrade()
    });
    if let Some(stop) = stop {
        stop.follow_inside();
    }
}

/// A foreign surface as its focus stop drives it, placed in the stop's box: gone, it holds nothing and takes nothing.
struct Placed {
    source: Weak<dyn ForeignSurface>,
    placement: Box<dyn Fn() -> Transform>,
}

impl Keyboard for Placed {
    fn text_entry_focused(&self) -> Option<bool> {
        Some(self.source.upgrade()?.text_entry_focused())
    }

    fn keeps(&self, keys: ConsumedKeys) -> Option<bool> {
        Some(self.source.upgrade()?.keeps(keys))
    }
}

impl Inside for Placed {
    fn arrive(&self, backwards: bool) {
        if let Some(source) = self.source.upgrade() {
            source.arrive(backwards);
        }
    }

    fn tab(&self, key: &Event) -> bool {
        self.source.upgrade().is_some_and(|source| source.tab(key))
    }

    fn blur(&self) {
        if let Some(source) = self.source.upgrade() {
            source.blur();
        }
    }

    fn holds_focus(&self) -> bool {
        self.source
            .upgrade()
            .is_some_and(|source| source.holds_focus())
    }

    fn text_input(&self) -> Option<(TextInput, Transform)> {
        Some((self.source.upgrade()?.text_input()?, (self.placement)()))
    }
}

impl Drop for ForeignBoundary {
    fn drop(&mut self) {
        REGISTRY.with(|registry| {
            registry
                .borrow_mut()
                .retain(|_, registered| registered.owner != self.key)
        });
    }
}

/// One key of the registry: a foreign surface's own, or one marking a surface nested in it.
struct Registered {
    /// The key of the foreign surface it belongs to; its own for the foreign surface itself.
    owner: SurfaceKey,
    /// The key the foreign world gave a surface nested in it; `None` for the foreign surface itself.
    guest: Option<SurfaceKey>,
    source: Weak<dyn ForeignSurface>,
    stop: Weak<Stop>,
}

reactive_core::dropless_thread_local! {
    static REGISTRY: released FxHashMap<SurfaceKey, Registered> = FxHashMap::default();
}

/// A key from the counter surface handles come from, so no foreign key is ever a surface's.
fn mint() -> SurfaceKey {
    SurfaceKey(crate::surface_context::next_handle().raw())
}

/// The key marking the surface the foreign surface at `owner` numbers `guest`, while its frames carry it.
pub(crate) fn nested_key_of(owner: SurfaceKey, guest: SurfaceKey) -> Option<SurfaceKey> {
    REGISTRY.with(|registry| {
        registry
            .borrow()
            .iter()
            .find(|(_, registered)| registered.owner == owner && registered.guest == Some(guest))
            .map(|(key, _)| *key)
    })
}

/// Whether `key` marks a foreign surface, or a surface nested in one.
pub(crate) fn is_foreign(key: SurfaceKey) -> bool {
    REGISTRY.with(|registry| registry.borrow().contains_key(&key))
}

/// The foreign surface `key` marks, while it is registered and alive; `None` for a key marking a surface nested in one.
pub(crate) fn source(key: SurfaceKey) -> Option<Rc<dyn ForeignSurface>> {
    REGISTRY.with(|registry| {
        let registry = registry.borrow();
        let registered = registry.get(&key).filter(|r| r.guest.is_none())?;
        registered.source.upgrade()
    })
}

/// The foreign surface a control with `id` in the surface `key` marks belongs to, with the id that surface knows the control by.
pub(crate) fn target(key: SurfaceKey, id: FocusId) -> Option<(Rc<dyn ForeignSurface>, AccessId)> {
    REGISTRY.with(|registry| {
        let registry = registry.borrow();
        let registered = registry.get(&key)?;
        let own = AccessId {
            surface: registered.guest,
            id,
        };
        Some((registered.source.upgrade()?, own))
    })
}

#[cfg(test)]
#[path = "foreign_surface.test.rs"]
mod tests;
