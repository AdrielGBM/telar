//! Keeping a node mounted while it leaves: [`Presence`] for one child behind a condition, and the same mechanism for the rows a transitioned [`ReactiveList`](crate::ReactiveList) removes. A [`Mount`] lives in the child's own owner, so it goes when the child is disposed; its host watches departures with [`watch_exits`] and disposes each child once its exit has settled, making the animation's own settling the timer.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use motion_core::{Animated, Easing, Tween, tween};
use platform_core::Event;
use reactive_core::{Effect, ReadSignal, RwSignal, effect, on_cleanup, signal};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{mark_dirty, new_container, set_children, set_display, track_layout};
use crate::layout_item::{BuildPass, Child, LayoutItem, dispose_child};
use crate::pointer::dispatch_container_event;
use crate::serial::Serial;
use crate::surface::{ENTER_MS, Edge, IDENTITY, slide_matrix};

reactive_core::surface_local! {
    /// Per surface, because what a host waits on before unmapping is its own window's exits.
    slot EXITS: RwSignal<usize> = signal(0);
    access with_exits, with_exits_ref;
    context ExitsContext, ExitsGuard;
}

/// How many nodes on the active surface are still playing their exit; a window may unmap once this reads 0.
pub fn exits_in_flight() -> ReadSignal<usize> {
    with_exits_ref(|exits| exits.read_only())
}

pub(crate) fn reset_exits() {
    let exits = with_exits_ref(|exits| *exits);
    if exits.is_alive() {
        exits.set(0);
    } else {
        with_exits(|exits| *exits = reactive_core::in_surface_world(|| signal(0)));
    }
}

/// How a child arrives and leaves: it fades, and optionally slides in from an edge, over one tween that the exit plays in reverse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    slide: Option<(Edge, f32)>,
    tween: Tween,
}

impl Transition {
    /// Fades in and out. A zero-length tween means no animation: the child appears and goes at once.
    pub fn fade(tween: Tween) -> Self {
        Self { slide: None, tween }
    }

    /// Fades while travelling `distance` px in from `edge`, and back out toward it.
    pub fn slide(edge: Edge, distance: f32, tween: Tween) -> Self {
        Self {
            slide: Some((edge, distance)),
            tween,
        }
    }

    /// How long either half takes.
    pub fn duration(&self) -> Duration {
        self.tween.duration
    }

    fn frame(&self, progress: f32) -> ([f32; 6], f32) {
        let p = progress.clamp(0.0, 1.0);
        let matrix = self.slide.map_or(IDENTITY, |(edge, distance)| {
            slide_matrix(edge, distance * (1.0 - p))
        });
        (matrix, p)
    }
}

impl Default for Transition {
    /// The surface transition's timing, faded.
    fn default() -> Self {
        Self::fade(tween(Duration::from_millis(ENTER_MS), Easing::EaseOut))
    }
}

/// One mounted child's arrival and departure. Created under the child's owner, so its state and the exit it counts are released with the child however the child goes.
#[derive(Clone, Copy)]
pub(crate) struct Mount {
    transition: Transition,
    progress: Animated<f32>,
    leaving: RwSignal<bool>,
    /// The counter this mount's exit was added to, taken when the exit is taken back. Kept rather than looked up again, because the child may be disposed while another surface is active.
    counted: RwSignal<Option<RwSignal<usize>>>,
}

impl Mount {
    /// A mount that plays its entrance when `entering`, and starts settled otherwise.
    pub(crate) fn new(transition: Transition, entering: bool) -> Self {
        let animates = entering && !transition.tween.duration.is_zero();
        // Built away from its goal and retargeted, because an `Animated` born settled never registers with the ticker and nothing would schedule the frames that carry it in.
        let progress = Animated::new(if animates { 0.0 } else { 1.0 }, transition.tween);
        progress.retarget(1.0);
        let mount = Self {
            transition,
            progress,
            leaving: signal(false),
            counted: signal(None),
        };
        on_cleanup(move || mount.uncount());
        mount
    }

    /// Starts the exit, answering whether there is one to wait for. `false` means there is nothing to play and the caller removes the child now.
    pub(crate) fn leave(&self) -> bool {
        if self.transition.tween.duration.is_zero() || self.progress.read().peek() <= 0.0 {
            return false;
        }
        if !self.leaving.peek() {
            self.leaving.set(true);
            let exits = with_exits_ref(|exits| *exits);
            exits.update(|n| *n += 1);
            self.counted.set(Some(exits));
        }
        self.progress.retarget(0.0);
        true
    }

    /// Takes the exit back from wherever it had got to.
    pub(crate) fn come_back(&self) {
        if !self.leaving.peek() {
            return;
        }
        self.leaving.set(false);
        self.uncount();
        self.progress.retarget(1.0);
    }

    fn uncount(&self) {
        if !self.counted.is_alive() {
            return;
        }
        let Some(exits) = self.counted.peek() else {
            return;
        };
        self.counted.set(None);
        if exits.is_alive() {
            exits.update(|n| *n = n.saturating_sub(1));
        }
    }

    /// Reactive, and `false` once the mount has been freed.
    pub(crate) fn is_leaving(&self) -> bool {
        self.leaving.try_get().unwrap_or(false)
    }

    pub(crate) fn is_leaving_now(&self) -> bool {
        self.leaving.is_alive() && self.leaving.peek()
    }

    /// Whether the exit has played to its end. Reactive over the progress, so a watcher re-runs on every frame of the exit.
    fn has_left(&self) -> bool {
        let at = self.progress.get();
        self.leaving.get() && at <= 0.0 && self.progress.is_settled()
    }

    /// The transform and opacity to draw the child with, subscribing the caller to the progress.
    pub(crate) fn frame(&self) -> ([f32; 6], f32) {
        if !self.progress.is_alive() {
            return (IDENTITY, 1.0);
        }
        self.transition.frame(self.progress.get())
    }

    pub(crate) fn frame_now(&self) -> ([f32; 6], f32) {
        if !self.progress.is_alive() {
            return (IDENTITY, 1.0);
        }
        self.transition.frame(self.progress.read().peek())
    }
}

/// Watches what a host holds on its way out and hands back, by the id the host gave it, each one whose exit has finished. `leaving` must read whatever the host bumps when it starts a new exit, so the watcher subscribes to that exit's progress.
pub(crate) fn watch_exits(
    leaving: impl Fn() -> Vec<(u64, Mount)> + 'static,
    finished: impl Fn(Vec<u64>) + 'static,
) -> Effect {
    effect(move || {
        let done: Vec<u64> = leaving()
            .into_iter()
            .filter(|(_, mount)| mount.has_left())
            .map(|(id, _)| id)
            .collect();
        if !done.is_empty() {
            finished(done);
        }
    })
}

type Build = Rc<dyn Fn() -> Result<Box<dyn LayoutItem>, LayoutError>>;

struct PresenceState {
    node: NodeId,
    child: Option<Child>,
}

/// A child mounted while `visible` holds and for as long as its exit takes after, then removed — `SurfaceTransition`'s enter and leave, for a node inside a tree rather than a whole surface. From the moment it starts leaving the child takes no input, so what is under it answers at once; shown again mid-exit it turns back from where it had got to, as the same node with the same state.
pub struct Presence {
    node: NodeId,
    rect: RwSignal<Rect>,
    state: Rc<RefCell<PresenceState>>,
    version: RwSignal<u64>,
    _visible: Effect,
    _exits: Effect,
}

impl Presence {
    /// A presence laid out as a column around its child.
    pub fn new(
        visible: impl Fn() -> bool + 'static,
        transition: Transition,
        child: impl Fn() -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    ) -> Result<Self, LayoutError> {
        Self::with_style(LayoutStyle::new().flex_column(), visible, transition, child)
    }

    /// [`new`](Self::new) with the presence's own node laid out by `style`. The node takes no space while nothing is mounted.
    pub fn with_style(
        style: LayoutStyle,
        visible: impl Fn() -> bool + 'static,
        transition: Transition,
        child: impl Fn() -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    ) -> Result<Self, LayoutError> {
        let node = new_container(style, &[])?;
        let rect = track_layout(node).expect("presence container is registered");
        let state = Rc::new(RefCell::new(PresenceState { node, child: None }));
        let version = signal(0u64);
        let mounted = signal(0u64);
        let build: Build = Rc::new(child);

        let shown = Rc::clone(&state);
        let initial = Cell::new(true);
        let serial = Serial::new();
        let _visible = effect(move || {
            let show = visible();
            let entering = !initial.replace(false);
            serial.run((show, entering), |(show, entering)| {
                let changed = if show {
                    appear(&shown, &build, transition, entering, mounted)
                } else {
                    disappear(&shown)
                };
                if changed {
                    version.update(|v| *v = v.wrapping_add(1));
                }
            });
        });

        let watched = Rc::clone(&state);
        let removed = Rc::clone(&state);
        let _exits = watch_exits(
            move || {
                mounted.get();
                watched
                    .borrow()
                    .child
                    .as_ref()
                    .and_then(Child::mount)
                    .map(|mount| vec![(0, mount)])
                    .unwrap_or_default()
            },
            move |_| {
                if unmount(&removed) {
                    version.update(|v| *v = v.wrapping_add(1));
                }
            },
        );

        Ok(Self {
            node,
            rect,
            state,
            version,
            _visible,
            _exits,
        })
    }

    /// Whether a child is mounted, including one on its way out.
    pub fn is_mounted(&self) -> bool {
        self.state.borrow().child.is_some()
    }
}

fn appear(
    state: &Rc<RefCell<PresenceState>>,
    build: &Build,
    transition: Transition,
    entering: bool,
    mounted: RwSignal<u64>,
) -> bool {
    let (held, existing) = {
        let st = state.borrow();
        (st.child.is_some(), st.child.as_ref().and_then(Child::mount))
    };
    if held {
        if let Some(mount) = existing {
            mount.come_back();
        }
        return false;
    }
    // A build that fails frees what it created, nodes and owner both, as it unwinds.
    let mut pass = BuildPass::start();
    let mut child = pass.build_child(|| build()).expect("presence child build");
    pass.keep();
    child.present(transition, entering);
    let node = {
        let mut st = state.borrow_mut();
        let node = st.node;
        let _ = set_children(node, &[child.node()]);
        st.child = Some(child);
        node
    };
    set_display(node, true);
    mark_dirty(node).ok();
    mounted.update(|m| *m = m.wrapping_add(1));
    true
}

fn disappear(state: &Rc<RefCell<PresenceState>>) -> bool {
    let mount = state.borrow().child.as_ref().and_then(Child::mount);
    match mount {
        Some(mount) if mount.leave() => false,
        Some(_) => unmount(state),
        None => {
            let node = state.borrow().node;
            set_display(node, false);
            false
        }
    }
}

fn unmount(state: &Rc<RefCell<PresenceState>>) -> bool {
    let (node, child) = {
        let mut st = state.borrow_mut();
        (st.node, st.child.take())
    };
    let Some(child) = child else {
        return false;
    };
    let _ = set_children(node, &[]);
    set_display(node, false);
    mark_dirty(node).ok();
    dispose_child(&child);
    true
}

impl LayoutItem for Presence {
    fn layout_node(&self) -> NodeId {
        self.node
    }

    fn occludes(&self) -> bool {
        let st = self.state.borrow();
        st.child.as_ref().is_some_and(|child| {
            !child.mount().is_some_and(|mount| mount.is_leaving_now())
                && child.item.try_borrow().is_ok_and(|item| item.occludes())
        })
    }
}

impl Component for Presence {
    fn view(&self) -> RenderNode {
        self.version.get();
        let _ = self.rect.get();
        let st = self.state.borrow();
        let Some(child) = st.child.as_ref() else {
            return RenderNode::Empty;
        };
        crate::element::wrap(self.node, RenderNode::group([child.boundary()]))
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // A snapshot, because the child's own handler may hide it, and the exit writes this state while the dispatch is under way.
        let mut children: Vec<Child> = self.state.borrow().child.iter().cloned().collect();
        dispatch_container_event(&mut children, event)
    }

    fn debug_name(&self) -> &'static str {
        "Presence"
    }
}

#[cfg(test)]
#[path = "presence_test.rs"]
mod tests;
