//! Keyboard focus: which widget receives key events. A base primitive with no styling of its own — a
//! focusable widget (e.g. [`crate::Input`]) requests focus on tap and consults it in `on_event`/`view`.
//!
//! Key events are broadcast to every widget (see `dispatch_container_event`), so focus is *self-filtering*:
//! a widget handles a key only when [`is_focused`] holds for its id — there is no central router. Focus is
//! a reactive signal, so a widget that reads [`current`]/[`is_focused`] inside its `view()` re-renders when
//! focus moves (e.g. to show or hide its caret). State is per-surface (each surface owns its own focus via
//! [`FocusContext`], activated by the runner), so focus never crosses windows; preserving focus across a
//! hot-reload dylib swap is out of scope.

use std::rc::Rc;

use layout_core::NodeId;
use platform_core::{Key, ModifiersState, NamedKey};
use reactive_core::{RwSignal, signal};
use rustc_hash::FxHashSet;

/// An opaque focus identity, one per focusable widget. Allocate with [`next_id`].
pub type FocusId = u64;

/// What kind of widget a focusable is, as far as the keyboard is concerned.
///
/// The distinction exists for one question: whether the keys arriving now are *text*. Key events are
/// broadcast, so an app-level shortcut handler and a focused field see the same press, and without this the
/// `3` typed into a dimension field also fires the app's `3` shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusKind {
    /// Takes keys as commands: a button, a tab, a slider.
    Widget,
    /// Takes keys as text: a field, an editor.
    TextEntry,
}

/// What a focusable *is*, for the reader that has to say it out loud — a separate question from [`FocusKind`],
/// which asks what the widget does with a key.
///
/// Defined in `platform-core` because it is the vocabulary the UI and the platform share, the same way
/// [`Key`] is. Re-exported here because this is where it is *authored*: a widget declares its role at the
/// moment it declares itself focusable, and the two are one call.
pub use platform_core::Role;

/// A cheap, `Copy` handle to a focusable widget's identity, so a caller that has moved the widget into a
/// container (and no longer holds a reference to it) can still drive its focus — e.g. autofocus a hosted
/// editor when its tab activates. Obtain one from the widget (see [`crate::TextArea::focus_handle`]).
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

/// Identifies one registered [`Scope`], so a closing overlay can withdraw exactly its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScopeId(u64);

/// A region of the tree whose focusables are only reachable while it is showing.
///
/// Declared by whatever can hide content without taking it out of the tree — an [`Overlay`](crate::Overlay)
/// kept mounted across a close, today. It names a *node*, not a set of ids, and that is the whole trick: an
/// overlay's children are built before the overlay that will host them, so it never learns which focusables
/// are its own. Ancestry answers instead.
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
/// Tab treats the two the same — neither is a stop — but they are opposite things to say out loud. A control
/// inside a closed dialog is *not there*; a disabled one is there and unavailable, and a reader that omitted
/// it would leave the user wondering where the button went.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScopeReason {
    NotShowing,
    Disabled,
}

/// One entry in the tab order.
struct Entry {
    id: FocusId,
    /// The widget's layout node, which is what makes reachability answerable. `None` for a focus id that
    /// belongs to no widget (the dismiss stack takes one as a token).
    node: Option<NodeId>,
    role: Role,
    /// Whether Tab stops here. `false` for a control that is driven some other way and still has to be
    /// announced: the rows of a menu answer to arrow keys, and putting each one in the tab order would make
    /// Tab walk a list the user opened precisely so as not to.
    tabbable: bool,
    /// A checked state, for the controls that have one. A closure and not a flag, for the same reason
    /// "reachable" is one: a checkbox toggles without being rebuilt, and a reader asking a moment later has to
    /// get the answer that is true then.
    toggled: Option<Rc<dyn Fn() -> bool>>,
}

/// Per-surface keyboard-focus state: the id allocator, the focused-widget signal, and the tab order.
struct FocusState {
    next_id: FocusId,
    next_scope: u64,
    focused: RwSignal<Option<FocusId>>,
    // Whether the focus held now was taken by a pointer. A signal, not a plain flag: Tab onto the widget you just clicked moves it without moving `focused`, and a ring that missed that would be stale exactly when the keyboard took over.
    pointer_focus: RwSignal<bool>,
    // Registered focusables in tab order (registration order ≈ document order). Drives Tab/Shift-Tab.
    order: Vec<Entry>,
    // Regions that can hide their contents without unregistering them; consulted when stepping.
    scopes: Vec<Scope>,
    // The subset of `order` that takes keys as text. A set rather than a field on each entry because it is
    // the minority and the only kind anyone asks about.
    text_entries: FxHashSet<FocusId>,
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
        }
    }
}

reactive_core::surface_local! {
    /// Per-surface focus state. The runner activates each surface's [`FocusContext`] around its
    /// build/event/frame, so focus never crosses windows.
    slot FOCUS: FocusState = FocusState::new();
    access with_focus, with_focus_ref;
    context FocusContext, FocusGuard;
}

/// The active surface's focused-widget signal, cloned out of the slot so callers never hold the slot borrow
/// across a `.set()` — its flush re-enters the slot when an effect reads [`current`].
fn focused_signal() -> RwSignal<Option<FocusId>> {
    with_focus_ref(|s| s.focused.clone())
}

/// Allocates a fresh focus id for a focusable widget.
pub fn next_id() -> FocusId {
    with_focus(|s| {
        let id = s.next_id;
        s.next_id += 1;
        id
    })
}

/// The currently focused widget, or `None`. Reactive: reading this inside a `view()` re-renders the
/// caller when focus changes.
pub fn current() -> Option<FocusId> {
    focused_signal().get()
}

/// Whether `id` currently holds focus.
pub fn is_focused(id: FocusId) -> bool {
    current() == Some(id)
}

// The three commands below `peek` the signal they write, and it matters: a command is a thing an *effect* may
// well issue ("while this row is the selected one, focus its field"), and a reactive read there would
// subscribe that effect to the focus it sets — so the next focus change anywhere would re-run it and it would
// take the focus straight back. Same rule, and the same bug, as `ScrollViewport::reveal`.

/// Gives focus to `id` (a no-op if it already holds it).
pub fn request(id: FocusId) {
    set_pointer_focus(false);
    let focused = focused_signal();
    if focused.peek() != Some(id) {
        focused.set(Some(id));
    }
}

/// [`request`] for focus a *tap* is giving, which is the one case that should not draw a focus ring.
///
/// The distinction CSS spent years arriving at as `:focus-visible`. A ring on every click is noise — the user
/// already knows where they clicked — and the ring drawn anyway is why so many stylesheets used to turn
/// outlines off altogether, taking the keyboard's only cue with them. Focus taken any other way (Tab, or an
/// application focusing something itself) shows it.
pub fn request_from_pointer(id: FocusId) {
    set_pointer_focus(true);
    let focused = focused_signal();
    if focused.peek() != Some(id) {
        focused.set(Some(id));
    }
}

/// Whether `id` holds focus *and* should show it. Reactive, like [`current`].
pub fn is_focus_visible(id: FocusId) -> bool {
    is_focused(id) && !pointer_focus_signal().get()
}

fn pointer_focus_signal() -> RwSignal<bool> {
    with_focus_ref(|s| s.pointer_focus.clone())
}

fn set_pointer_focus(from_pointer: bool) {
    let flag = pointer_focus_signal();
    if flag.peek() != from_pointer {
        flag.set(from_pointer);
    }
}

/// Removes focus from `id`, but only if it currently holds it — so a widget blurring itself never steals
/// focus away from another.
pub fn release(id: FocusId) {
    let focused = focused_signal();
    if focused.peek() == Some(id) {
        focused.set(None);
    }
}

/// Clears focus entirely, whoever holds it.
pub fn clear() {
    let focused = focused_signal();
    if focused.peek().is_some() {
        focused.set(None);
    }
}

/// Adds `id` to the tab order (at the end), if not already present, as a widget that says what it does with
/// the keyboard — a text field registers as [`FocusKind::TextEntry`], which is what makes
/// [`text_entry_focused`] answerable. A focusable widget calls this on creation; registration order is the
/// traversal order.
pub fn register_as(id: FocusId, kind: FocusKind) {
    register_node(id, kind, None, default_role(kind), true);
}

/// [`register_as`] for a widget that can say which layout node it is, which is what lets Tab skip it while it
/// is not on screen. Every focusable widget should use this; the node-less forms remain for a focus id that
/// stands for something other than a widget.
pub fn register_at(id: FocusId, kind: FocusKind, node: NodeId) {
    register_node(id, kind, Some(node), default_role(kind), true);
}

/// [`register_at`] for a widget that is not simply "a thing you activate" — a checkbox, a tab, a slider. The
/// role is what a screen reader says this is; see [`Role`].
pub fn register_with_role(id: FocusId, kind: FocusKind, node: NodeId, role: Role) {
    register_node(id, kind, Some(node), role, true);
}

/// Registers a control that is announced but is not a Tab stop, because something else drives it.
///
/// The rows of a menu are the case: they answer to arrow keys and type-ahead, and a reader that could not see
/// them would be handed an open menu it could not describe — while a Tab order containing every row would
/// walk the user through a list they opened in order to *avoid* walking it.
pub fn register_presented(id: FocusId, node: NodeId, role: Role) {
    register_node(id, FocusKind::Widget, Some(node), role, false);
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
        match s.order.iter_mut().find(|e| e.id == id) {
            // Re-registering only ever adds knowledge: a widget that learns its node later keeps its place.
            Some(existing) => {
                existing.node = existing.node.or(node);
                if role != Role::default() {
                    existing.role = role;
                }
                existing.tabbable &= tabbable;
            }
            None => s.order.push(Entry {
                id,
                node,
                role,
                tabbable,
                toggled: None,
            }),
        }
        if kind == FocusKind::TextEntry {
            s.text_entries.insert(id);
        }
    });
}

/// Declares a region whose focusables are only reachable while `showing` reads true, and — when `traps` — that
/// holds focus inside itself while it is up.
///
/// The counterpart of the pointer barrier an overlay already puts up. Without it the tab order is a list built
/// when widgets were *constructed*, which says nothing about what is on screen now: a dialog kept mounted
/// across a close leaves its fields as Tab stops, and one that is open does not stop Tab walking out behind it.
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

/// Removes `id` from the tab order and drops its focus if it held it. A focusable widget calls this on
/// drop, so a destroyed widget never lingers in traversal or as the focused id.
pub fn unregister(id: FocusId) {
    with_focus(|s| {
        s.order.retain(|e| e.id != id);
        s.text_entries.remove(&id);
    });
    release(id);
}

/// Whether the focused widget takes keys as text. Reactive, like [`current`].
///
/// The guard an app-level shortcut table needs: without it, typing into a field also runs the shortcuts
/// that share its letters. Prefer [`text_entry_takes_key`], which lets through the presses no editor wants.
pub fn text_entry_focused() -> bool {
    match current() {
        Some(id) => with_focus_ref(|s| s.text_entries.contains(&id)),
        None => false,
    }
}

/// Whether a focused text entry would take this press as text — the guard for a global shortcut handler.
///
/// Narrower than [`text_entry_focused`] on purpose: a field claims the letters and the caret keys, and
/// nothing else. `⌘S` still saves while the caret sits in a field, and so do the function keys, because no
/// editor here does anything with them. The list mirrors what [`crate::Input`] and [`crate::TextArea`]
/// actually consume, and their own tests hold it to that.
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

/// Moves focus to the next registered focusable in tab order (wrapping); with nothing focused, focuses
/// the first. A no-op when nothing is registered.
pub fn focus_next() {
    step(1);
}

/// Like [`focus_next`] but backwards (Shift+Tab).
pub fn focus_prev() {
    step(-1);
}

/// A [`Scope`] as [`step`] reads it, once copied out from under the slot borrow: where it is, whether it is
/// showing, and whether it holds focus inside itself.
type ScopeView = (NodeId, Rc<dyn Fn() -> bool>, bool);

/// Whether Tab should be able to land on a focusable at `node`, given the scopes registered right now.
///
/// Three ways to be out of reach, and they are genuinely different mechanisms rather than one seen from three
/// angles — which is why a rule aimed at any single one of them leaves the others open:
/// - out of layout flow, by its own `display:none` or an ancestor's, which leaves the rect it last had;
/// - inside a region kept mounted while not showing, which leaves the rect *and* the layout intact;
/// - outside the modal that is currently up, which is about nothing on the node itself.
fn reachable(node: Option<NodeId>, scopes: &[ScopeView]) -> bool {
    // A focus id that stands for no widget has no way to be off screen.
    let Some(node) = node else { return true };
    if layout_reactive::is_hidden(node) {
        return false;
    }
    if scopes
        .iter()
        .any(|(scope, showing, _)| !showing() && layout_reactive::is_descendant_of(node, *scope))
    {
        return false;
    }
    // The topmost trapping scope that is up holds focus inside itself.
    match scopes
        .iter()
        .rev()
        .find(|(_, showing, traps)| *traps && showing())
    {
        Some((scope, _, _)) => layout_reactive::is_descendant_of(node, *scope),
        None => true,
    }
}

/// The tab order and the scopes, copied out from under the slot borrow — see [`step`] for why that matters.
fn snapshot() -> (Vec<(FocusId, Option<NodeId>)>, Vec<ScopeView>) {
    with_focus_ref(|s| {
        let order: Vec<(FocusId, Option<NodeId>)> = s
            .order
            .iter()
            .filter(|e| e.tabbable)
            .map(|e| (e.id, e.node))
            .collect();
        let scopes: Vec<ScopeView> = s
            .scopes
            .iter()
            .map(|sc| (sc.node, sc.showing.clone(), sc.traps))
            .collect();
        (order, scopes)
    })
}

/// Declares that `id` carries a checked state, and how to read it now.
///
/// Separate from registering the control because the two are known at different moments: a box declares what
/// it *is* as it is built, and what it is *bound to* when the caller hands it a signal.
pub fn set_toggled(id: FocusId, state: impl Fn() -> bool + 'static) {
    let state: Rc<dyn Fn() -> bool> = Rc::new(state);
    with_focus(|s| {
        if let Some(entry) = s.order.iter_mut().find(|e| e.id == id) {
            entry.toggled = Some(state);
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
    /// Its checked state, for the controls that carry one.
    pub toggled: Option<bool>,
}

/// The focusables a screen reader should be told about, in tab order.
///
/// Deliberately the same [`reachable`] the keyboard walks, so the two can never disagree about what is on
/// screen — with one distinction Tab has no use for: a control kept out of reach by being *disabled* is
/// reported as present and unavailable, where one inside a closed dialog is not reported at all.
pub fn exposed() -> Vec<Exposed> {
    let (order, scopes) = with_focus_ref(|s| {
        // The state closures come out with everything else and are called after the borrow drops: reading one
        // can read a signal, and reading a signal can flush effects back through this very slot.
        let order: Vec<(FocusId, Option<NodeId>, Role, Option<Rc<dyn Fn() -> bool>>)> = s
            .order
            .iter()
            .map(|e| (e.id, e.node, e.role, e.toggled.clone()))
            .collect();
        let scopes: Vec<(NodeId, Rc<dyn Fn() -> bool>, bool, ScopeReason)> = s
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

    order
        .into_iter()
        .filter_map(|(id, node, role, toggled)| {
            let node = node?;
            reachable(Some(node), &hiding).then(|| Exposed {
                id,
                node,
                role,
                enabled: !scopes.iter().any(|(scope, showing, _, reason)| {
                    *reason == ScopeReason::Disabled
                        && !showing()
                        && layout_reactive::is_descendant_of(node, *scope)
                }),
                toggled: toggled.as_ref().map(|read| read()),
            })
        })
        .collect()
}

/// Moves focus to the first reachable focusable inside `node`, reporting whether it found one.
///
/// What a dialog needs on open: the keyboard has to arrive somewhere inside it, or the user is left tabbing
/// from wherever they were — which, now that a modal traps focus, means tabbing nowhere at all.
pub fn focus_first_in(node: NodeId) -> bool {
    let (order, scopes) = snapshot();
    let found = order.into_iter().find(|(_, widget)| {
        widget.is_some_and(|widget| layout_reactive::is_descendant_of(widget, node))
            && reachable(*widget, &scopes)
    });
    match found {
        Some((id, _)) => {
            request(id);
            true
        }
        None => false,
    }
}

/// Whether `id` is still registered, so a caller restoring remembered focus does not aim at a widget that has
/// since been dropped.
pub fn is_registered(id: FocusId) -> bool {
    with_focus_ref(|s| s.order.iter().any(|e| e.id == id))
}

fn step(dir: isize) {
    // Snapshot, then release the slot borrow: `showing` is the author's closure and the reachability queries borrow the layout runtime, and neither may run under this one — nor may `request`, which flushes.
    let (order, scopes) = snapshot();
    let order: Vec<FocusId> = order
        .into_iter()
        .filter(|(_, node)| reachable(*node, &scopes))
        .map(|(id, _)| id)
        .collect();
    if order.is_empty() {
        return;
    }
    let n = order.len() as isize;
    let next = match current().and_then(|c| order.iter().position(|&x| x == c)) {
        Some(i) => order[((i as isize + dir).rem_euclid(n)) as usize],
        None => {
            if dir > 0 {
                order[0]
            } else {
                order[order.len() - 1]
            }
        }
    };
    request(next);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_release_and_ids_are_unique() {
        clear();
        let a = next_id();
        let b = next_id();
        assert_ne!(a, b, "ids must be unique");

        assert!(!is_focused(a));
        request(a);
        assert!(is_focused(a) && current() == Some(a));

        // Requesting b moves focus off a.
        request(b);
        assert!(is_focused(b) && !is_focused(a));

        // Releasing a (which is not focused) leaves b focused.
        release(a);
        assert!(is_focused(b));

        // Releasing the focused one clears it.
        release(b);
        assert!(current().is_none());
    }

    /// A control the application has disabled is not a Tab stop — in HTML a `disabled` element is skipped
    /// outright, and a keyboard user made to walk through controls that do nothing is being told less about
    /// the interface than a mouse user, who at least sees them dimmed.
    ///
    /// Rides the same scope mechanism a hidden overlay uses rather than a second one, which is what gives a
    /// disabled *wrapper* the `fieldset` reading for the keyboard as well as for the pointer.
    #[test]
    fn tab_skips_a_disabled_box() {
        use crate::context::{compute_layout, reset_layout_runtime};
        use crate::{LayoutItem, StyledContainer};
        use layout_core::{AvailableSpace, LayoutStyle};

        reset_layout_runtime();
        let base = next_id();
        register_as(base, FocusKind::Widget);

        let below = next_id();
        let off = StyledContainer::new(
            LayoutStyle::new().width(50.0).height(20.0),
            |_r| renderer_core::RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_focus(|_| {})
        .disabled(|| true);
        let above = next_id();
        compute_layout(
            off.layout_node(),
            AvailableSpace::Definite(50.0),
            AvailableSpace::Definite(20.0),
        )
        .unwrap();

        request(base);
        focus_next();
        let landed = current().expect("something took focus");
        assert!(
            !(landed > below && landed < above),
            "Tab landed on a box the application had disabled"
        );
    }

    /// The case that showed an overlay-shaped fix would only ever be half of one: `display:none` hides content
    /// without any overlay involved, and left it in the tab order just the same. The two mechanisms leave
    /// opposite traces — a hidden overlay keeps its children's rects and stops painting, a `display:none`
    /// subtree collapses to zero and keeps its place in the walk — so neither a paint test nor a rect test
    /// catches both. Ancestry does.
    #[test]
    fn tab_skips_a_focusable_taken_out_of_layout_flow() {
        use crate::context::{compute_layout, reset_layout_runtime, set_display};
        use crate::{LayoutItem, StyledContainer};
        use layout_core::{AvailableSpace, LayoutStyle};

        reset_layout_runtime();
        let base = next_id();
        register_as(base, FocusKind::Widget);

        let below = next_id();
        let hidden = StyledContainer::new(
            LayoutStyle::new().width(50.0).height(20.0),
            |_r| renderer_core::RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_focus(|_| {});
        let node = hidden.layout_node();
        let root = StyledContainer::new(
            LayoutStyle::new().width(100.0).height(100.0),
            |_r| renderer_core::RectStyle::default(),
            vec![Box::new(hidden)],
        )
        .unwrap();
        let above = next_id();

        set_display(node, false);
        compute_layout(
            root.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        request(base);
        focus_next();
        let landed = current().expect("something took focus");
        assert!(
            !(landed > below && landed < above),
            "Tab landed on a focusable that is out of layout flow"
        );
    }

    #[test]
    fn tab_order_steps_forward_and_back() {
        // Register three contiguous ids at the end of the order and step within that block (robust to any
        // ids other tests registered earlier on this thread).
        let (a, b, c) = (next_id(), next_id(), next_id());
        register_as(a, FocusKind::Widget);
        register_as(b, FocusKind::Widget);
        register_as(c, FocusKind::Widget);

        request(a);
        focus_next();
        assert_eq!(current(), Some(b));
        focus_next();
        assert_eq!(current(), Some(c));
        focus_prev();
        assert_eq!(current(), Some(b));

        // Unregistering the focused one drops focus and removes it from traversal.
        unregister(b);
        assert!(current().is_none());
        unregister(a);
        unregister(c);
    }
}
