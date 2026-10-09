//! Keyboard focus: which widget receives key events. A base primitive with no styling of its own — a focusable widget (e.g. [`crate::Input`]) requests focus on tap and consults it in `on_event`/`view`.
//!
//! Key events are broadcast to every widget (see `dispatch_container_event`), so focus is *self-filtering*: a widget handles a key only when [`is_key_target`] holds for its id — there is no central router. Focus is a reactive signal, so a widget that reads [`current`]/[`is_focused`] inside its `view()` re-renders when focus moves (e.g. to show or hide its caret). State is per-surface (each surface owns its own focus via [`FocusContext`], activated by the runner), so focus never crosses windows; preserving focus across a hot-reload dylib swap is out of scope.

use std::rc::Rc;

use layout_core::NodeId;
use platform_core::{ConsumedKeys, Key, ModifiersState, NamedKey, NumericValue, Orientation};
use reactive_core::{Effect, RwSignal, effect, signal};
use renderer_core::Focusable;
use rustc_hash::FxHashSet;

/// An opaque focus identity, one per focusable widget. Allocate with [`next_id`].
pub type FocusId = u64;

/// What kind of widget a focusable is, as far as the keyboard is concerned.
///
/// The distinction exists for one question: whether the keys arriving now are *text*. Key events are broadcast, so an app-level shortcut handler and a focused field see the same press, and without this the `3` typed into a dimension field also fires the app's `3` shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusKind {
    /// Takes keys as commands: a button, a tab, a slider.
    Widget,
    /// Takes keys as text: a field, an editor.
    TextEntry,
}

/// What a focusable *is*, for the reader that has to say it out loud — a separate question from [`FocusKind`], which asks what the widget does with a key.
///
/// Defined in `platform-core` because it is the vocabulary the UI and the platform share, the same way [`Key`] is. Re-exported here because this is where it is *authored*: a widget declares its role at the moment it declares itself focusable, and the two are one call.
pub use platform_core::{Role, SetPosition};

/// A cheap, `Copy` handle to a focusable widget's identity, so a caller that has moved the widget into a container (and no longer holds a reference to it) can still drive its focus — e.g. autofocus a hosted editor when its tab activates. Obtain one from the widget (see [`crate::TextArea::focus_handle`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusHandle(FocusId);

impl FocusHandle {
    /// Gives focus to the handle's widget.
    pub fn request(self) {
        request(self.0);
    }

    /// Removes focus from the handle's widget, only if it currently holds it.
    pub fn release(self) {
        release(self.0);
    }

    /// Whether the handle's widget currently holds focus.
    pub fn is_focused(self) -> bool {
        is_focused(self.0)
    }
}

/// Wraps a raw [`FocusId`] in a [`FocusHandle`]. A focusable widget hands out a handle to its own id.
pub fn handle(id: FocusId) -> FocusHandle {
    FocusHandle(id)
}

/// Identifies one registered `Scope`, so a closing overlay can withdraw exactly its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScopeId(u64);

/// A region of the tree whose focusables are only reachable while it is showing.
///
/// Declared by whatever can hide content without taking it out of the tree — an [`Overlay`](crate::Overlay) kept mounted across a close, today. It names a *node*, not a set of ids, and that is the whole trick: an overlay's children are built before the overlay that will host them, so it never learns which focusables are its own. Ancestry answers instead.
struct Scope {
    id: ScopeId,
    node: NodeId,
    showing: Rc<dyn Fn() -> bool>,
    /// Whether the scope holds focus in while it shows — a modal, as against a tooltip layer.
    traps: bool,
    reason: ScopeReason,
}

/// Why a scope's focusables are out of reach, which the keyboard does not care about and a screen reader does.
///
/// Tab treats the two the same — neither is a stop — but they are opposite things to say out loud. A control inside a closed dialog is *not there*; a disabled one is there and unavailable, and a reader that omitted it would leave the user wondering where the button went.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScopeReason {
    NotShowing,
    Disabled,
}

/// One entry in the tab order.
struct Entry {
    id: FocusId,
    /// The widget's layout node, which is what makes reachability answerable. `None` for a focus id that belongs to no widget (the dismiss stack takes one as a token).
    node: Option<NodeId>,
    role: Role,
    /// Whether Tab stops here. `false` for a control that is driven some other way and still has to be announced: the rows of a menu answer to arrow keys, and putting each one in the tab order would make Tab walk a list the user opened precisely so as not to.
    tabbable: bool,
    /// For a scroll area, whether its content can scroll now: it is a stop only then, and only while nothing inside it takes focus. See [`register_scroller`].
    scrolls: Option<Rc<dyn Fn() -> bool>>,
    /// A checked state, for the controls that have one. A closure and not a flag, for the same reason "reachable" is one: a checkbox toggles without being rebuilt, and a reader asking a moment later has to get the answer that is true then.
    toggled: Option<Rc<dyn Fn() -> bool>>,
    value: Option<Rc<dyn Fn() -> NumericValue>>,
    orientation: Option<Orientation>,
    outline: Outline,
    active_descendant: Option<Rc<dyn Fn() -> Option<FocusId>>>,
    /// The surface that answers for this entry's keyboard while it holds focus. See [`delegate_keyboard`].
    keyboard: Option<KeyboardDelegate>,
}

type KeyboardDelegate = Rc<dyn Fn() -> Option<crate::SurfaceGuard>>;

/// Per-surface keyboard-focus state: the id allocator, the focused-widget signal, and the tab order.
struct FocusState {
    next_id: FocusId,
    next_scope: u64,
    focused: RwSignal<Option<FocusId>>,
    // A signal, not a plain flag: Tab onto the widget just clicked moves this without moving `focused`, and a ring that missed it would be stale exactly when the keyboard took over.
    pointer_focus: RwSignal<bool>,
    // In registration order; Tab walks it in document order (see `in_document_order`).
    order: Vec<Entry>,
    // Regions that can hide their contents without unregistering them; consulted when stepping.
    scopes: Vec<Scope>,
    // A set rather than a field on each entry, because it is the minority and the only kind anyone asks about.
    text_entries: FxHashSet<FocusId>,
    // Bumped when a gate or an ownership link is registered, so the guard re-reads an ancestry it could not have subscribed to yet.
    reach: RwSignal<u64>,
    guard: Option<Effect>,
    // The one request still waiting for its batch to settle, stamped so a request that lost to a later give, blur or request does not act when it is finally judged.
    deferred: Option<(u64, FocusId)>,
    requests: u64,
    // Who the key being dispatched belongs to, fixed as its dispatch starts. See [`is_key_target`].
    keystroke: Option<Option<FocusId>>,
    // A surface shown inside another tree's frame: stepping past either end of its tab order leaves it rather than wrapping. See [`host`].
    hosted: bool,
    departed: Option<Travel>,
    // The focusable the latest request was for, when a step made it, and which way that step went.
    arrival: Option<(FocusId, Travel)>,
}

/// Which way the keyboard moved through the tab order: Tab, or Shift+Tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Travel {
    Forward,
    Backward,
}

impl FocusState {
    fn new() -> Self {
        Self {
            next_id: 1,
            next_scope: 1,
            focused: signal(None),
            pointer_focus: signal(false),
            order: Vec::new(),
            scopes: Vec::new(),
            text_entries: FxHashSet::default(),
            reach: signal(0),
            guard: None,
            deferred: None,
            requests: 0,
            keystroke: None,
            hosted: false,
            departed: None,
            arrival: None,
        }
    }

    fn entry(&self, id: FocusId) -> Option<&Entry> {
        self.order.iter().find(|e| e.id == id)
    }

    fn entry_mut(&mut self, id: FocusId) -> Option<&mut Entry> {
        self.order.iter_mut().find(|e| e.id == id)
    }
}

reactive_core::surface_local! {
    /// Per-surface focus state. The runner activates each surface's [`FocusContext`] around its build/event/frame, so focus never crosses windows.
    slot FOCUS: FocusState = FocusState::new();
    access with_focus, with_focus_ref;
    context FocusContext, FocusGuard;
}

/// The active surface's focused-widget signal, cloned out of the slot so callers never hold the slot borrow across a `.set()` — its flush re-enters the slot when an effect reads [`current`].
fn focused_signal() -> RwSignal<Option<FocusId>> {
    with_focus_ref(|s| s.focused)
}

/// Allocates a fresh focus id for a focusable widget.
pub fn next_id() -> FocusId {
    with_focus(|s| {
        let id = s.next_id;
        s.next_id += 1;
        id
    })
}

/// The currently focused widget, or `None`. Reactive: reading this inside a `view()` re-renders the caller when focus changes.
pub fn current() -> Option<FocusId> {
    focused_signal().get()
}

/// Whether `id` currently holds focus.
pub fn is_focused(id: FocusId) -> bool {
    current() == Some(id)
}

/// Whether the key being dispatched is `id`'s to handle: `id` held focus when the key arrived. Outside a key's dispatch, whether `id` holds focus now.
///
/// What a widget gates its keys on, rather than [`is_focused`]. Keys are offered to every widget in turn, so a handler that moves focus — `/` taking the caret to a search field — would otherwise hand the rest of that same keystroke to the widget it just focused, which then types the `/` it was summoned by.
pub fn is_key_target(id: FocusId) -> bool {
    key_target() == Some(id)
}

/// Who the key being dispatched belongs to, or who holds focus now outside a key's dispatch.
fn key_target() -> Option<FocusId> {
    with_focus_ref(|s| s.keystroke).unwrap_or_else(current)
}

/// Fixes [`is_key_target`] to whoever holds focus now, until the guard drops. `None` inside a dispatch that already fixed it, so the outermost walk decides.
pub(crate) fn deliver_keystroke() -> Option<KeystrokeGuard> {
    let held = focused_signal().peek();
    with_focus(|s| match s.keystroke {
        Some(_) => None,
        None => {
            s.keystroke = Some(held);
            Some(KeystrokeGuard)
        }
    })
}

pub(crate) struct KeystrokeGuard;

impl Drop for KeystrokeGuard {
    fn drop(&mut self) {
        with_focus(|s| s.keystroke = None);
    }
}

// The three commands below `peek` the signal they write: an effect may well issue one ("focus the selected row's field"), and a reactive read would subscribe it to the focus it sets, taking focus straight back on the next change anywhere. Same rule as `ScrollViewport::reveal`.

/// Gives focus to `id`: a no-op if it already holds it, or if it sits in a subtree that takes no input, though a subtree hidden when asked is judged again once its batch settles.
pub fn request(id: FocusId) {
    take(id, false);
}

/// [`request`] for focus a *tap* is giving, which is the one case that should not draw a focus ring.
///
/// The distinction CSS spent years arriving at as `:focus-visible`. A ring on every click is noise — the user already knows where they clicked — and the ring drawn anyway is why so many stylesheets used to turn outlines off altogether, taking the keyboard's only cue with them. Focus taken any other way (Tab, or an application focusing something itself) shows it.
pub fn request_from_pointer(id: FocusId) {
    take(id, true);
}

fn take(id: FocusId, from_pointer: bool) {
    take_arriving(id, from_pointer, None);
}

/// Only the latest word on focus counts: a request judged after its batch settles acts only if nothing gave, released or requested focus since it was made.
fn take_arriving(id: FocusId, from_pointer: bool, travel: Option<Travel>) {
    with_focus(|s| s.arrival = travel.map(|travel| (id, travel)));
    if !refuses(id) {
        give(id, from_pointer);
        return;
    }
    let request = with_focus(|s| {
        s.requests += 1;
        s.deferred = Some((s.requests, id));
        s.requests
    });
    let surface = reactive_core::current_surface();
    reactive_core::after_settle(move || {
        // The id is only meaningful in the surface that minted it, and that surface may be gone by now.
        if reactive_core::current_surface() != surface {
            return;
        }
        let current = with_focus(|s| match s.deferred {
            Some((pending, _)) if pending == request => s.deferred.take().is_some(),
            _ => false,
        });
        if current && is_registered(id) && !refuses(id) {
            give(id, from_pointer);
        }
    });
}

fn supersede_deferred() {
    with_focus(|s| s.deferred = None);
}

fn refuses(id: FocusId) -> bool {
    node_of(id).is_some_and(|node| !crate::input_region::receives_input(node))
}

fn give(id: FocusId, from_pointer: bool) {
    supersede_deferred();
    guard_focus();
    set_pointer_focus(from_pointer);
    let focused = focused_signal();
    if focused.peek() != Some(id) {
        focused.set(Some(id));
    }
}

pub(crate) fn node_of(id: FocusId) -> Option<NodeId> {
    with_focus_ref(|s| s.entry(id).and_then(|e| e.node))
}

/// Whether `id` holds focus *and* should show it. Reactive, like [`current`].
///
/// A text entry shows it however it was reached, as `:focus-visible` does for a field: the caret alone blinks out half the time, and a person who tapped a field is about to type into it.
pub fn is_focus_visible(id: FocusId) -> bool {
    is_focused(id)
        && (!pointer_focus_signal().get() || with_focus_ref(|s| s.text_entries.contains(&id)))
}

fn pointer_focus_signal() -> RwSignal<bool> {
    with_focus_ref(|s| s.pointer_focus)
}

fn set_pointer_focus(from_pointer: bool) {
    let flag = pointer_focus_signal();
    if flag.peek() != from_pointer {
        flag.set(from_pointer);
    }
}

/// Removes focus from `id`, but only if it currently holds it — so a widget blurring itself never steals focus away from another — and withdraws a request for `id` still waiting on its batch.
pub fn release(id: FocusId) {
    let held = focused_signal().peek() == Some(id);
    let pending = with_focus_ref(|s| s.deferred.is_some_and(|(_, waiting)| waiting == id));
    if held || pending {
        supersede_deferred();
    }
    drop_focus(id);
}

fn drop_focus(id: FocusId) {
    let focused = focused_signal();
    if focused.peek() == Some(id) {
        focused.set(None);
    }
}

fn guard_focus() {
    if with_focus_ref(|s| s.guard.is_some()) {
        return;
    }
    let guard = reactive_core::in_surface_world(|| {
        effect(|| {
            with_focus_ref(|s| s.reach).get();
            let Some(id) = focused_signal().get() else {
                return;
            };
            let Some(node) = node_of(id) else {
                return;
            };
            if let Some(rect) = layout_reactive::track_layout(node) {
                rect.get();
            }
            if !crate::input_region::receives_input(node) {
                drop_focus(id);
            }
        })
    });
    with_focus(|s| s.guard = Some(guard));
}

pub(crate) fn reach_changed() {
    if focused_signal().peek().is_none() {
        return;
    }
    let reach = with_focus_ref(|s| s.reach);
    reach.set(reach.peek().wrapping_add(1));
}

/// Takes the keyboard away when a press lands on nothing that wants it.
///
/// **The rule every platform has, and the one a toolkit cannot leave to its applications.** Focus was only ever *taken* here — by a tap on a focusable, by Tab — so a field kept the caret until something else asked for it, and clicking away from a form left it sitting there looking editable, eating the keys, and telling an application asking [`text_entry_focused`] that somebody was still typing.
///
/// Asked before the press is dispatched, so a focusable that is pressed takes focus back on its way through and only a press with no focusable under it clears anything. The test is the on-screen shape — where the widget is drawn rather than where it was laid out — so a field inside a scrolled viewport answers about the place the pointer actually is.
pub fn blur_from_pointer(x: f32, y: f32) {
    if current().is_none() {
        return;
    }
    // Collected before the rects are asked for: reading layout under the focus borrow would hold one runtime across a call into another.
    // A scroll area is not one: the keyboard reaches it, a press does not.
    let nodes: Vec<NodeId> = with_focus_ref(|s| {
        s.order
            .iter()
            .filter(|entry| entry.scrolls.is_none())
            .filter_map(|entry| entry.node)
            .collect()
    });
    let on_a_focusable = nodes
        .into_iter()
        .any(|node| crate::input_region::pointable(node, x, y));
    if !on_a_focusable {
        clear();
    }
}

/// Clears focus entirely, whoever holds it, and withdraws any request still waiting on its batch.
pub fn clear() {
    supersede_deferred();
    let focused = focused_signal();
    if focused.peek().is_some() {
        focused.set(None);
    }
}

/// Adds `id` to the tab order (at the end), if not already present, as a widget that says what it does with the keyboard — a text field registers as [`FocusKind::TextEntry`], which is what makes [`text_entry_focused`] answerable. A focusable widget calls this on creation; registration order is the traversal order.
pub fn register_as(id: FocusId, kind: FocusKind) {
    register_node(id, kind, None, default_role(kind), true);
}

/// [`register_as`] for a widget that can say which layout node it is, which is what lets Tab skip it while it is not on screen. Every focusable widget should use this; the node-less forms remain for a focus id that stands for something other than a widget.
pub fn register_at(id: FocusId, kind: FocusKind, node: NodeId) {
    register_node(id, kind, Some(node), default_role(kind), true);
}

/// [`register_at`] for a widget that is not simply "a thing you activate" — a checkbox, a tab, a slider. The role is what a screen reader says this is; see [`Role`].
pub fn register_with_role(id: FocusId, kind: FocusKind, node: NodeId, role: Role) {
    register_node(id, kind, Some(node), role, true);
}

/// Registers a control that is announced but is not a Tab stop, because something else drives it.
///
/// The rows of a menu are the case: they answer to arrow keys and type-ahead, and a reader that could not see them would be handed an open menu it could not describe — while a Tab order containing every row would walk the user through a list they opened in order to *avoid* walking it.
pub fn register_presented(id: FocusId, node: NodeId, role: Role) {
    register_node(id, FocusKind::Widget, Some(node), role, false);
}

/// Registers a scroll area as a stop of its own, so the keyboard can scroll it — but only while `scrolls` reads true and nothing reachable inside `node` takes focus.
///
/// A region that holds controls is scrolled by moving through them, and a stop of its own there would be one more Tab between the field above it and the first control in it. One that holds only text has nothing else the keyboard could reach it by.
pub(crate) fn register_scroller(id: FocusId, node: NodeId, scrolls: impl Fn() -> bool + 'static) {
    register_node(id, FocusKind::Widget, Some(node), Role::ScrollArea, true);
    let scrolls: Rc<dyn Fn() -> bool> = Rc::new(scrolls);
    with_focus(|s| {
        if let Some(entry) = s.entry_mut(id) {
            entry.scrolls = Some(scrolls);
        }
    });
}

/// What a widget is taken to be when it has not said: the reading that matches what the keyboard does with it.
fn default_role(kind: FocusKind) -> Role {
    match kind {
        FocusKind::Widget => Role::Button,
        FocusKind::TextEntry => Role::TextInput,
    }
}

fn register_node(id: FocusId, kind: FocusKind, node: Option<NodeId>, role: Role, tabbable: bool) {
    with_focus(|s| {
        match s.entry_mut(id) {
            // Re-registering only adds knowledge: a widget that learns its node later keeps its place.
            Some(existing) => {
                existing.node = existing.node.or(node);
                // A re-registration repeating the default has said nothing, and must not overwrite a declared role.
                if role != default_role(kind) {
                    existing.role = role;
                }
                existing.tabbable &= tabbable;
            }
            None => s.order.push(Entry {
                id,
                node,
                role,
                tabbable,
                scrolls: None,
                toggled: None,
                value: None,
                orientation: None,
                outline: Outline::default(),
                active_descendant: None,
                keyboard: None,
            }),
        }
        if kind == FocusKind::TextEntry {
            s.text_entries.insert(id);
        }
    });
}

/// Declares a region whose focusables are only reachable while `showing` reads true, and — when `traps` — that holds focus inside itself while it is up.
///
/// The counterpart of the pointer barrier an overlay already puts up. Without it the tab order is a list built when widgets were *constructed*, which says nothing about what is on screen now: a dialog kept mounted across a close leaves its fields as Tab stops, and one that is open does not stop Tab walking out behind it.
pub fn register_scope(node: NodeId, showing: impl Fn() -> bool + 'static, traps: bool) -> ScopeId {
    register_scope_because(node, showing, traps, ScopeReason::NotShowing)
}

/// [`register_scope`] for a region that says *why* its focusables are out of reach. See [`ScopeReason`].
pub fn register_scope_because(
    node: NodeId,
    showing: impl Fn() -> bool + 'static,
    traps: bool,
    reason: ScopeReason,
) -> ScopeId {
    with_focus(|s| {
        let id = ScopeId(s.next_scope);
        s.next_scope += 1;
        s.scopes.push(Scope {
            id,
            node,
            showing: Rc::new(showing),
            traps,
            reason,
        });
        id
    })
}

/// Withdraws a scope registered with [`register_scope`].
pub fn unregister_scope(id: ScopeId) {
    with_focus(|s| s.scopes.retain(|scope| scope.id != id));
}

/// Removes `id` from the tab order and drops its focus if it held it. A focusable widget calls this on drop, so a destroyed widget never lingers in traversal or as the focused id.
pub fn unregister(id: FocusId) {
    with_focus(|s| {
        s.order.retain(|e| e.id != id);
        s.text_entries.remove(&id);
    });
    release(id);
}

/// Whether the focused widget takes keys as text — during a key's dispatch, the widget the key belongs to (see [`is_key_target`]). Reactive, like [`current`].
///
/// The guard an app-level shortcut table needs: without it, typing into a field also runs the shortcuts that share its letters. Prefer [`text_entry_takes_key`], which lets through the presses no editor wants.
pub fn text_entry_focused() -> bool {
    match key_target() {
        Some(id) => delegated(id, text_entry_focused)
            .unwrap_or_else(|| with_focus_ref(|s| s.text_entries.contains(&id))),
        None => false,
    }
}

/// Hands the keyboard questions about `id` on to another surface while `id` holds focus: whether a text entry has the keys, and which keys are kept, are answered by whatever holds focus in the surface `enter` activates.
///
/// A nested surface's frame is the case. It holds focus here while a field inside it has the caret, and a shortcut table here asking whether somebody is typing has to hear about that field, not about the frame.
pub(crate) fn delegate_keyboard(
    id: FocusId,
    enter: impl Fn() -> Option<crate::SurfaceGuard> + 'static,
) {
    let enter: KeyboardDelegate = Rc::new(enter);
    with_focus(|s| {
        if let Some(entry) = s.entry_mut(id) {
            entry.keyboard = Some(enter);
        }
    });
}

fn delegated<R>(id: FocusId, ask: impl FnOnce() -> R) -> Option<R> {
    let enter = with_focus_ref(|s| s.entry(id).and_then(|e| e.keyboard.clone()))?;
    let _inside = enter()?;
    Some(ask())
}

/// Whether the focused widget — during a key's dispatch, the one the key belongs to — keeps `keys` for itself, as a control of its role uses them: a button acts on Enter, a slider on the arrows and nothing else. `false` with nothing focused. Reactive, like [`current`].
///
/// What decides whether a key may go past focus to whatever would take it otherwise, such as the dismiss stack's Enter.
pub fn focused_keeps(keys: ConsumedKeys) -> bool {
    let Some(id) = key_target() else {
        return false;
    };
    delegated(id, || focused_keeps(keys)) == Some(true)
        || with_focus_ref(|s| {
            s.entry(id)
                .is_some_and(|entry| entry.role.consumed_keys().contains(keys))
        })
}

/// Whether a focused text entry would take this press as text — the guard for a global shortcut handler.
///
/// Narrower than [`text_entry_focused`] on purpose: a field claims the letters and the caret keys, and nothing else. `⌘S` still saves while the caret sits in a field, and so do the function keys, because no editor here does anything with them. The list mirrors what [`crate::Input`] and [`crate::TextArea`] actually consume, and their own tests hold it to that.
pub fn text_entry_takes_key(key: &Key, modifiers: ModifiersState) -> bool {
    text_entry_focused() && edits_text(key, modifiers)
}

fn edits_text(key: &Key, modifiers: ModifiersState) -> bool {
    match key {
        // A chord is a command, not text: the editors ignore it too.
        Key::Char(_) if modifiers.is_ctrl || modifiers.is_meta => false,
        Key::Char(c) => !c.is_control(),
        Key::Named(named) => matches!(
            named,
            NamedKey::Space
                | NamedKey::Backspace
                | NamedKey::Delete
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::Home
                | NamedKey::End
                | NamedKey::Enter
                | NamedKey::Escape
                | NamedKey::Tab
        ),
    }
}

/// Moves focus to the next registered focusable in tab order (wrapping); with nothing focused, focuses the first. A no-op when nothing is registered.
pub fn focus_next() {
    step(1);
}

/// Like [`focus_next`] but backwards (Shift+Tab).
pub fn focus_prev() {
    step(-1);
}

/// A [`Scope`] as [`step`] reads it, once copied out from under the slot borrow: where it is, whether it is showing, and whether it holds focus inside itself.
type ScopeView = (NodeId, Rc<dyn Fn() -> bool>, bool);

/// Whether Tab should be able to land on a focusable at `node`, given the scopes registered right now.
///
/// Three ways to be out of reach, and they are genuinely different mechanisms rather than one seen from three angles — which is why a rule aimed at any single one of them leaves the others open:
/// - taking no input, by being out of layout flow, fully transparent or inert, itself or through an ancestor;
/// - inside a region kept mounted while not showing, which leaves the rect *and* the layout intact;
/// - outside the modal that is currently up, which is about nothing on the node itself.
fn reachable(node: Option<NodeId>, scopes: &[ScopeView]) -> bool {
    // A focus id standing for no widget has no way to be off screen.
    let Some(node) = node else { return true };
    if !crate::input_region::receives_input(node) {
        return false;
    }
    if scopes
        .iter()
        .any(|(scope, showing, _)| !showing() && crate::input_region::is_inside(node, *scope))
    {
        return false;
    }
    // The topmost trapping scope that is up holds focus inside itself.
    match scopes
        .iter()
        .rev()
        .find(|(_, showing, traps)| *traps && showing())
    {
        Some((scope, _, _)) => crate::input_region::is_inside(node, *scope),
        None => true,
    }
}

/// One place Tab stops: the focusable, the box it is, and what it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabStop {
    pub id: FocusId,
    /// `None` for a focus id that belongs to no widget.
    pub node: Option<NodeId>,
    pub role: Role,
}

/// The stops Tab walks right now, in the order it walks them.
///
/// The document's order, not the order the frame draws in: a layer fixed over the page or an overlay is drawn after everything under it and read where it was declared. Only what Tab can reach is in it — nothing inside a closed region or a disabled one, and only the open modal's own stops while one holds focus.
pub fn tab_order() -> Vec<TabStop> {
    let (order, scopes) = with_focus_ref(|s| {
        let order: Vec<TabStop> = s
            .order
            .iter()
            .filter(|e| e.tabbable)
            .map(|e| TabStop {
                id: e.id,
                node: e.node,
                role: e.role,
            })
            .collect();
        let scopes: Vec<ScopeView> = s
            .scopes
            .iter()
            .map(|sc| (sc.node, sc.showing.clone(), sc.traps))
            .collect();
        (order, scopes)
    });
    // Filtered once the borrow drops: `showing` is the author's closure, and the reachability queries borrow the layout runtime.
    let idle = idle_scrollers(&scopes);
    let order = order
        .into_iter()
        .filter(|stop| !idle.contains(&stop.id))
        .collect();
    in_document_order(order)
        .into_iter()
        .filter(|stop| reachable(stop.node, &scopes))
        .collect()
}

/// The stops in the order the document reads them, which is not the order they registered in: a list built before the field drawn above it registers first, and a branch rebuilt registers last. Content placed from elsewhere — a scroll's, an overlay's — reads where it is declared; a focus id with no node comes last.
fn in_document_order(mut order: Vec<TabStop>) -> Vec<TabStop> {
    // Roots rank by the first stop registered under each: separate roots have no order of their own to read.
    let mut roots: Vec<NodeId> = Vec::new();
    order.sort_by_cached_key(|stop| {
        stop.node.map_or((usize::MAX, Vec::new()), |node| {
            let (root, path) = crate::input_region::document_position(node);
            let rank = roots
                .iter()
                .position(|&seen| seen == root)
                .unwrap_or_else(|| {
                    roots.push(root);
                    roots.len() - 1
                });
            (rank, path)
        })
    });
    order
}

/// The scroll areas that are no stop right now: their content fits, or something reachable inside them takes focus. See [`register_scroller`].
fn idle_scrollers(scopes: &[ScopeView]) -> Vec<FocusId> {
    type Scroller = (FocusId, NodeId, Rc<dyn Fn() -> bool>);
    let (scrollers, holders) = with_focus_ref(|s| {
        let scrollers: Vec<Scroller> = s
            .order
            .iter()
            .filter_map(|e| Some((e.id, e.node?, e.scrolls.clone()?)))
            .collect();
        if scrollers.is_empty() {
            return (scrollers, Vec::new());
        }
        let holders: Vec<NodeId> = s
            .order
            .iter()
            .filter(|e| e.scrolls.is_none())
            .filter_map(|e| e.node)
            .collect();
        (scrollers, holders)
    });
    scrollers
        .into_iter()
        .filter(|(_, node, scrolls)| {
            !scrolls()
                || holders.iter().any(|&held| {
                    crate::input_region::is_inside(held, *node) && reachable(Some(held), scopes)
                })
        })
        .map(|(id, _, _)| id)
        .collect()
}

/// Declares that `id` carries an on/off state, and how to read it now. What the state is — checked, pressed, selected, expanded — is its role's [`Role::toggle_kind`].
///
/// Separate from registering the control because the two are known at different moments: a box declares what it *is* as it is built, and what it is *bound to* when the caller hands it a signal.
pub fn set_toggled(id: FocusId, state: impl Fn() -> bool + 'static) {
    let state: Rc<dyn Fn() -> bool> = Rc::new(state);
    with_focus(|s| {
        if let Some(entry) = s.entry_mut(id) {
            entry.toggled = Some(state);
        }
    });
}

/// Declares that `id` carries a number, and how to read it now. The counterpart of [`set_toggled`] for the roles whose state is a value rather than a flag.
pub fn set_value(id: FocusId, read: impl Fn() -> NumericValue + 'static) {
    let read: Rc<dyn Fn() -> NumericValue> = Rc::new(read);
    with_focus(|s| {
        if let Some(entry) = s.entry_mut(id) {
            entry.value = Some(read);
        }
    });
}

/// Declares the axis `id` runs along: the bar of a splitter, the track of a slider.
pub fn set_orientation(id: FocusId, orientation: Orientation) {
    with_focus(|s| {
        if let Some(entry) = s.entry_mut(id) {
            entry.orientation = Some(orientation);
        }
    });
}

/// What a row of a hierarchy says beyond its on/off state: whether it is open, and where it sits.
#[derive(Clone, Default)]
struct Outline {
    expanded: Option<Rc<dyn Fn() -> bool>>,
    position: Option<SetPosition>,
}

/// Declares that `id` has rows under it, and how to read whether it shows them now.
pub fn set_expanded(id: FocusId, state: impl Fn() -> bool + 'static) {
    let state: Rc<dyn Fn() -> bool> = Rc::new(state);
    with_focus(|s| {
        if let Some(entry) = s.entry_mut(id) {
            entry.outline.expanded = Some(state);
        }
    });
}

/// Declares where `id` sits in its hierarchy and among its siblings.
pub fn set_position(id: FocusId, position: SetPosition) {
    with_focus(|s| {
        if let Some(entry) = s.entry_mut(id) {
            entry.outline.position = Some(position);
        }
    });
}

/// Declares that `container` keeps focus while a cursor walks the controls inside it, and how to read which one the cursor is on now: the row of a tree, the item of a toolbar. `None` while there is no cursor, or its control is not built.
///
/// What lets a reader announce the item the arrows moved to, which it otherwise never hears of: focus itself never moves.
pub fn set_active_descendant(container: FocusId, read: impl Fn() -> Option<FocusId> + 'static) {
    let read: Rc<dyn Fn() -> Option<FocusId>> = Rc::new(read);
    with_focus(|s| {
        if let Some(entry) = s.entry_mut(container) {
            entry.active_descendant = Some(read);
        }
    });
}

/// One focusable as the accessibility layer sees it: where it is, what it is, and whether it is available.
pub struct Exposed {
    pub id: FocusId,
    pub node: NodeId,
    pub role: Role,
    /// Available to be activated. `false` is *announced*, not hidden — see [`ScopeReason`].
    pub enabled: bool,
    /// Its on/off state, for the controls that carry one: checked, pressed, selected or expanded as its role says.
    pub toggled: Option<bool>,
    /// Its numeric reading, for the controls that carry one.
    pub value: Option<NumericValue>,
    pub orientation: Option<Orientation>,
    /// Whether a tree row with rows under it shows them; `None` for a leaf and for every other control.
    pub expanded: Option<bool>,
    pub position: Option<SetPosition>,
}

/// The focusables a screen reader should be told about, in the order they registered; a scroll area only while it is a stop of its own.
///
/// Deliberately the same `reachable` the keyboard walks, so the two can never disagree about what is on screen — with one distinction Tab has no use for: a control kept out of reach by being *disabled* is reported as present and unavailable, where one inside a closed dialog is not reported at all.
pub fn exposed() -> Vec<Exposed> {
    let (order, scopes) = with_focus_ref(|s| {
        // Called after the borrow drops: reading a state closure can read a signal, and that can flush effects back through this very slot.
        type Row = (
            FocusId,
            Option<NodeId>,
            Role,
            Option<Rc<dyn Fn() -> bool>>,
            Option<Rc<dyn Fn() -> NumericValue>>,
            Option<Orientation>,
            Outline,
        );
        let order: Vec<Row> = s
            .order
            .iter()
            .map(|e| {
                (
                    e.id,
                    e.node,
                    e.role,
                    e.toggled.clone(),
                    e.value.clone(),
                    e.orientation,
                    e.outline.clone(),
                )
            })
            .collect();
        type ScopeRow = (NodeId, Rc<dyn Fn() -> bool>, bool, ScopeReason);
        let scopes: Vec<ScopeRow> = s
            .scopes
            .iter()
            .map(|sc| (sc.node, sc.showing.clone(), sc.traps, sc.reason))
            .collect();
        (order, scopes)
    });
    let hiding: Vec<ScopeView> = scopes
        .iter()
        .filter(|(_, _, _, reason)| *reason == ScopeReason::NotShowing)
        .map(|(node, showing, traps, _)| (*node, showing.clone(), *traps))
        .collect();
    let idle = idle_scrollers(&hiding);

    order
        .into_iter()
        .filter(|(id, ..)| !idle.contains(id))
        .filter_map(|(id, node, role, toggled, value, orientation, outline)| {
            let node = node?;
            reachable(Some(node), &hiding).then(|| Exposed {
                id,
                node,
                role,
                enabled: !scopes.iter().any(|(scope, showing, _, reason)| {
                    *reason == ScopeReason::Disabled
                        && !showing()
                        && crate::input_region::is_inside(node, *scope)
                }),
                toggled: toggled.as_ref().map(|read| read()),
                value: value.as_ref().map(|read| read()),
                orientation,
                expanded: outline.expanded.as_ref().map(|read| read()),
                position: outline.position,
            })
        })
        .collect()
}

/// Moves focus to the first reachable focusable inside `node`, reporting whether it found one.
///
/// What a dialog needs on open: the keyboard has to arrive somewhere inside it, or the user is left tabbing from wherever they were — which, now that a modal traps focus, means tabbing nowhere at all.
pub fn focus_first_in(node: NodeId) -> bool {
    let found = tab_order().into_iter().find(|stop| {
        stop.node
            .is_some_and(|widget| crate::input_region::is_inside(widget, node))
    });
    match found {
        Some(stop) => {
            request(stop.id);
            true
        }
        None => false,
    }
}

/// How the keyboard reaches `id` right now: whether Tab stops there, and which keys it keeps.
///
/// `declared` is the widget's own set for the state it is in; `None` takes `role`'s. A control out of reach keeps nothing, and one inside a modal that holds focus keeps Tab as well, because stepping inside the trap is Telar's and a host walking its own order would leave it. Reactive, like [`current`]: the box re-emits when a scope it sits in opens or closes.
pub fn focusable_of(id: FocusId, role: Role, declared: Option<ConsumedKeys>) -> Focusable {
    let (entry, scopes) = with_focus_ref(|s| {
        let entry = s.entry(id).map(|e| (e.node, e.tabbable));
        let scopes: Vec<ScopeView> = s
            .scopes
            .iter()
            .map(|sc| (sc.node, sc.showing.clone(), sc.traps))
            .collect();
        (entry, scopes)
    });
    let Some((node, tabbable)) = entry else {
        return Focusable::default();
    };
    if !reachable(node, &scopes) {
        return Focusable::default();
    }
    let consumes = declared.unwrap_or_else(|| role.consumed_keys());
    let trapped = scopes.iter().any(|(_, showing, traps)| *traps && showing());
    Focusable {
        tab_stop: tabbable,
        consumes: if trapped {
            consumes | ConsumedKeys::TAB
        } else {
            consumes
        },
    }
}

/// Follows a focus move the surface made on its own to the box `box_id` names (see [`Event::BoxFocused`](platform_core::Event::BoxFocused)), reporting whether Telar's focus changed.
///
/// Taken as keyboard focus, so the ring shows. A box already focused is left alone: the move that reports it may be the echo of a tap that focused it first, and re-taking it would turn that tap's ring on.
pub fn follow_box(box_id: u64) -> bool {
    // A presented control never holds focus: the document focusing a tapped row must not take focus from the tree that drives it.
    let found = with_focus_ref(|s| {
        s.order
            .iter()
            .find(|e| e.tabbable && e.node.is_some_and(|node| u64::from(node) == box_id))
            .map(|e| e.id)
    });
    let Some(id) = found else {
        return false;
    };
    if focused_signal().peek() == Some(id) {
        return false;
    }
    request(id);
    true
}

/// Whether `id` is still registered, so a caller restoring remembered focus does not aim at a widget that has since been dropped.
pub fn is_registered(id: FocusId) -> bool {
    with_focus_ref(|s| s.order.iter().any(|e| e.id == id))
}

/// Wraps at either end, except in a [hosted](host) surface with no modal up, where stepping past an end clears focus and leaves the way it went for the frame to carry on outside.
fn step(dir: isize) {
    let order: Vec<FocusId> = tab_order().into_iter().map(|stop| stop.id).collect();
    let travel = if dir > 0 {
        Travel::Forward
    } else {
        Travel::Backward
    };
    let n = order.len() as isize;
    let at = match current().and_then(|c| order.iter().position(|&x| x == c)) {
        Some(i) => i as isize + dir,
        None if dir > 0 => 0,
        None => n - 1,
    };
    if !(0..n).contains(&at) && with_focus_ref(|s| s.hosted) && !trapped() {
        with_focus(|s| s.departed = Some(travel));
        clear();
        return;
    }
    if n == 0 {
        return;
    }
    take_arriving(order[at.rem_euclid(n) as usize], false, Some(travel));
}

/// Whether a modal is up and holding focus inside itself.
fn trapped() -> bool {
    let scopes: Vec<(Rc<dyn Fn() -> bool>, bool)> = with_focus_ref(|s| {
        s.scopes
            .iter()
            .map(|scope| (scope.showing.clone(), scope.traps))
            .collect()
    });
    scopes.iter().any(|(showing, traps)| *traps && showing())
}

/// Marks the active surface as one shown inside another tree's frame, whose keyboard carries on outside it: Tab past the last focusable, or Shift+Tab past the first, clears focus here instead of wrapping and leaves the way it went for [`take_departure`]. A modal up inside still holds Tab within itself.
pub(crate) fn host() {
    with_focus(|s| s.hosted = true);
}

/// Which way the keyboard last stepped out of the active [hosted](host) surface, forgetting it.
pub(crate) fn take_departure() -> Option<Travel> {
    with_focus(|s| s.departed.take())
}

/// Which way the step that gave `id` focus went, when the latest request for focus was such a step for `id`: what tells Shift+Tab arriving at a composite from Tab arriving at it.
pub(crate) fn arrived_by(id: FocusId) -> Option<Travel> {
    with_focus_ref(|s| {
        s.arrival
            .filter(|(at, _)| *at == id)
            .map(|(_, travel)| travel)
    })
}

#[cfg(test)]
#[path = "focus_test.rs"]
mod tests;

#[cfg(test)]
#[path = "focus_order_test.rs"]
mod order_tests;

/// The checked state `id` declared, read now.
///
/// `None` both for a control that carries no such state and for one nothing has registered. Reading it here rather than through [`exposed`] is what lets a widget put its own state on the box it draws: the reading happens inside `view()`, so the box re-emits when the state changes, which a snapshot taken afterwards could never do.
pub fn toggled_state(id: FocusId) -> Option<bool> {
    let read = with_focus(|s| s.entry(id).and_then(|e| e.toggled.clone()))?;
    Some(read())
}

/// The number `id` declared, read now. Read inside `view()` for the same reason as [`toggled_state`].
pub fn value_state(id: FocusId) -> Option<NumericValue> {
    let read = with_focus(|s| s.entry(id).and_then(|e| e.value.clone()))?;
    Some(read())
}

/// The axis `id` declared, if any.
pub fn orientation_state(id: FocusId) -> Option<Orientation> {
    with_focus(|s| s.entry(id).and_then(|e| e.orientation))
}

/// Whether `id` shows the rows under it, read now; `None` for a leaf and for anything that does not expand. Read inside `view()` for the same reason as [`toggled_state`].
pub fn expanded_state(id: FocusId) -> Option<bool> {
    let read = with_focus(|s| s.entry(id).and_then(|e| e.outline.expanded.clone()))?;
    Some(read())
}

/// Where `id` said it sits, if it said.
pub fn position_state(id: FocusId) -> Option<SetPosition> {
    with_focus(|s| s.entry(id).and_then(|e| e.outline.position))
}

/// The control the cursor of `container` rests on, read now; `None` when it declared no cursor, or names a control no longer registered. Read inside `view()` for the same reason as [`toggled_state`].
pub fn active_descendant_state(container: FocusId) -> Option<FocusId> {
    let read = with_focus(|s| s.entry(container).and_then(|e| e.active_descendant.clone()))?;
    read().filter(|item| is_registered(*item))
}
