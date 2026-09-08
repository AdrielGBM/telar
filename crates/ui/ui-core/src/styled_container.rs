//! [`StyledContainer`]: the painted box every interactive widget is built on — state styles, gestures, focus and transforms.

use geometry_core::{Rect, Transform};
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::{
    Cursor, Event, Key, NamedKey, NumericValue, PointerButton, PointerSource, WindowCommand,
};
use reactive_core::{Effect, Reactive, RwSignal, effect, signal};
use renderer_core::{Border, Declared, RectStyle};
use theme_core::use_theme_tokens;
use ui_tree::{Component, EventResult, RenderNode};

use crate::child_host::{ChildSlot, DynHost};
use crate::context::{new_container, track_layout};
use crate::drag::DragGesture;
use crate::focus::{self, FocusId};
use crate::layout_item::{LayoutItem, TrackedChildren, register_container};
use crate::pointer::dispatch_container_event;
use crate::press::PressGesture;

/// The bool is [`KeyAnswer::took`], resolved at the builder so dispatch has one shape to call.
type KeyTable = Box<dyn Fn(&Key) -> bool>;

/// What a key handler answers, which is either nothing at all or whether it took the key.
///
/// Two shapes for one hook because a shortcut table and a key binding are different things. A table — «these are the application's keys» — answers `()`: it acts on the ones it knows and lets every key through, which is what a broadcast handler wants. A binding on a key the runtime *also* uses has to be able to end it, and `Tab` is that key: with nothing focused it enters the focus order, so a table that answered only `()` got its own action **and** the focus move, on every press.
///
/// `()` is the answer a closure written before this existed already gives, so nothing had to be rewritten to keep working.
pub trait KeyAnswer {
    /// Whether the handler took the key, leaving nothing for anyone else.
    fn took(self) -> bool;
}

impl KeyAnswer for () {
    fn took(self) -> bool {
        false
    }
}

impl KeyAnswer for bool {
    fn took(self) -> bool {
        self
    }
}

/// Re-resolves `node`'s layout style whenever the reactive state `style` reads changes, and once now.
///
/// The general form of [`StyledContainer::styled_by`], for a widget that is not a container — a text leaf sized off the theme's `font_size`, a slider thumb sized off its `spacing`. The returned [`Effect`] must be held for as long as the node lives, which for a leaf means its owning container `keeping` it.
pub fn style_follows(node: NodeId, style: impl Fn() -> LayoutStyle + 'static) -> Effect {
    effect(move || {
        let _ = crate::context::set_layout_style(node, style());
    })
}

/// The states a box paints differently in, **in precedence order**: the first one engaged wins.
///
/// A list rather than a chain of `else if`, so a new state is added by putting it at the right place here instead of at the right place inside `view()` — where the order was a comment and nothing held the code to it.
const PAINT_STATES: [PaintState; 3] = [PaintState::Disabled, PaintState::Active, PaintState::Hover];

/// One of the box's paint states. See [`PAINT_STATES`] for the order they resolve in.
#[derive(Clone, Copy)]
enum PaintState {
    /// A control that cannot be used must not also look pressable, so this is ahead of every other state.
    Disabled,
    /// A primary pointer held down inside the box — CSS `:active`.
    Active,
    /// Mouse only, like the hover callback.
    Hover,
}

/// The paint a box swaps in per state, and the state itself.
///
/// One value so "does this box repaint on a pointer transition" is a question with an owner, instead of a term someone has to remember to add to a disjunction spelled out at the top of `on_event`.
struct StateStyle {
    // Mouse only, like the hover callback.
    hover: Option<Box<dyn Fn(Rect) -> RectStyle>>,
    is_hovered: RwSignal<bool>,
    // The pressed / CSS `:active` state, taking precedence over `hover`. Mouse and touch; cleared on release, leave, or drag-off.
    active: Option<Box<dyn Fn(Rect) -> RectStyle>>,
    is_active: RwSignal<bool>,
    // Ahead of every other state: a control that cannot be used must not also look pressable.
    disabled: Option<Box<dyn Fn(Rect) -> RectStyle>>,
    // Laid *over* whichever state won, not instead of it.
    focus: Option<Box<dyn Fn(Rect) -> RectStyle>>,
}

impl Default for StateStyle {
    fn default() -> Self {
        Self {
            hover: None,
            is_hovered: signal(false),
            active: None,
            is_active: signal(false),
            disabled: None,
            focus: None,
        }
    }
}

impl StateStyle {
    /// Whether a pointer transition changes how the box looks, and so has to be tracked at all.
    fn repaints_on_pointer(&self) -> bool {
        self.hover.is_some() || self.active.is_some()
    }
}

/// What the box wants told about the pointer, beyond press and drag.
#[derive(Default)]
struct PointerHooks {
    hover: Option<Box<dyn Fn(bool)>>,
    // The continuous half of `hover`, which reports only the crossings.
    moved: Option<Box<dyn Fn(f32, f32)>>,
    scroll: Option<Box<dyn Fn(f32, f32)>>,
    // Restored to the default on leave.
    cursor: Option<Reactive<Cursor>>,
}

impl PointerHooks {
    /// Whether anything here needs the pointer's moves. `cursor` counts: a box whose only claim is a shape still has to see the crossings that set and clear it.
    fn is_set(&self) -> bool {
        self.hover.is_some()
            || self.moved.is_some()
            || self.scroll.is_some()
            || self.cursor.is_some()
    }
}

/// The box's place in the focus order, when it has one.
#[derive(Default)]
struct Focusable {
    // When set, the box joins the tab order, takes focus on tap, and handles Tab while focused.
    id: Option<FocusId>,
    // Dropping it with the box tears the subscription down.
    _effect: Option<Effect>,
    // Whether Enter and Space fire the press the way a tap does. Set by `control`, because a control that answers a mouse and not a keyboard is the failure this path exists to make unspellable.
    activates: bool,
    // Registered when the box is given a `disabled` source; withdrawn on drop.
    scope: Option<focus::ScopeId>,
}

/// The painted box every interactive widget is built on: state styles, gestures, focus and transforms.
pub struct StyledContainer {
    node: NodeId,
    rect: RwSignal<Rect>,
    style: Box<dyn Fn(Rect) -> RectStyle>,
    state: StateStyle,
    // A closure, so `view()` and the pointer path both re-read it. `None` skips the call on pointer moves.
    disabled_source: Option<Box<dyn Fn() -> bool>>,
    // A closure, so `view()` re-reads it and a `transition:opacity` resolves per render. `None` is opaque.
    opacity: Option<Box<dyn Fn() -> f32>>,
    // Takes the laid-out `Rect` so rotate/scale can pivot on the box centre; `None` means identity.
    transform: Option<Box<dyn Fn(Rect) -> Option<[f32; 6]>>>,
    children: TrackedChildren,
    // Set when the box holds a reactive fragment: static and dynamic children route through the host so they interleave in this node. `children` is empty in that case.
    dyn_host: Option<DynHost>,
    press: PressGesture,
    drag: DragGesture,
    pointer: PointerHooks,
    // A GLOBAL shortcut handler, not focused text input: key events carry no pointer position, so they are broadcast to every widget.
    on_key: Option<KeyTable>,
    focusable: Focusable,
    // Whether the box declines to shadow what it is drawn over (`pointer-events: none`).
    click_through: bool,
    // Whether a stroke that starts here is this box's and goes no further out.
    holds_stroke: bool,
    // What the box is, where it is more than a box. `None` reads it from what the box does.
    role: Option<renderer_core::Role>,
}

impl StyledContainer {
    pub fn new(
        layout_style: LayoutStyle,
        style: impl Fn(Rect) -> RectStyle + 'static,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        let (node, rect, children) = register_container(layout_style, children)?;
        Ok(Self::assemble(node, rect, Box::new(style), children, None))
    }

    /// Everything a fresh box holds before any builder touches it; the two constructors differ only in where their children live.
    fn assemble(
        node: NodeId,
        rect: RwSignal<Rect>,
        style: Box<dyn Fn(Rect) -> RectStyle>,
        children: TrackedChildren,
        dyn_host: Option<DynHost>,
    ) -> Self {
        Self {
            node,
            rect,
            style,
            state: StateStyle::default(),
            disabled_source: None,
            opacity: None,
            transform: None,
            children,
            dyn_host,
            press: PressGesture::default(),
            drag: DragGesture::default(),
            pointer: PointerHooks::default(),
            on_key: None,
            focusable: Focusable::default(),
            click_through: false,
            holds_stroke: false,
            role: None,
        }
    }

    /// A styled box whose children are a mix of static widgets and reactive fragments (`ChildSlot`s), reconciled into this box's own node so they inherit its flex direction/gap — the transparent `box`-with-a-`for` path (see [`Container::from_slots`](crate::Container::from_slots)).
    pub fn from_slots(
        layout_style: LayoutStyle,
        style: impl Fn(Rect) -> RectStyle + 'static,
        slots: Vec<ChildSlot>,
    ) -> Result<Self, LayoutError> {
        let node = new_container(layout_style, &[])?;
        let rect = track_layout(node).expect("new_container always registers a signal");
        let dyn_host = DynHost::build(node, slots)?;
        Ok(Self::assemble(
            node,
            rect,
            Box::new(style),
            Vec::new(),
            Some(dyn_host),
        ))
    }

    /// This box, for a backend whose output is a document: what it is, and what it asked layout for.
    ///
    /// The role is derived rather than declared — a box that answers a press *is* a button whether or not anybody said so, and one that declines pointer events is one the document should let events through.
    fn element(&self) -> std::sync::Arc<renderer_core::Element> {
        let mut semantics =
            renderer_core::Semantics::of(crate::element::role_of(self.role, self.press.is_set()));
        semantics.click_through = self.click_through;
        // Read inside `view()`, so what a reader is told and what is drawn are the same frame.
        if let Some(id) = self.focusable.id {
            semantics.focused = focus::is_focused(id);
            semantics.toggled = focus::toggled_state(id);
        }
        semantics.disabled = self.is_disabled();
        crate::element::with_semantics(self.node, semantics)
    }

    /// Whether the box is currently refusing input. `None` — the common case — answers without a dyn call on the pointer-move broadcast path, which every box in the tree pays.
    fn is_disabled(&self) -> bool {
        self.disabled_source.as_ref().is_some_and(|f| f())
    }

    /// The paint `state` swaps in, or `None` when the box has no style for it or is not in it.
    ///
    /// **The engagement test runs only once a style exists.** Each test reads a signal, and reading one subscribes this `view()` to it, so testing a state the box has no paint for would make a plain box re-render on every pointer move. That is why the states are a list of thunks rather than a table of booleans: a table would have to evaluate all four to build itself.
    fn state_style(&self, state: PaintState) -> Option<&dyn Fn(Rect) -> RectStyle> {
        let (style, is_engaged): (_, fn(&Self) -> bool) = match state {
            PaintState::Disabled => (&self.state.disabled, Self::is_disabled),
            PaintState::Active => (&self.state.active, |box_| box_.state.is_active.get()),
            PaintState::Hover => (&self.state.hover, |box_| box_.state.is_hovered.get()),
        };
        style.as_deref().filter(|_| is_engaged(self))
    }

    /// A move: the hover state, the cursor, and whatever gesture is already running.
    ///
    /// Broadcast to all children for their hover, and feeding our own scroll-vs-tap tracking. Hover is mouse-only: touch has no "pointer left", so a tap would leave the box stuck in its hover style.
    fn on_pointer_moved(
        &mut self,
        event: &Event,
        rect: Rect,
        x: f64,
        y: f64,
        source: &PointerSource,
    ) -> EventResult {
        self.press.track_move(event);
        let dragged = self.drag.moved(event, rect) == EventResult::Handled;
        // A stroke past its drag threshold has committed to being a drag, so it is no longer a tap. Only for a box that set one: without a threshold both have always fired, and a slider taking a press keeps that.
        if self.drag.has_threshold() && self.drag.has_started() {
            self.press.cancel();
        }
        let child = self.dispatch_children(event);
        // A move is broadcast for gestures already running, but only the topmost box under the pointer is hovered.
        let inside = rect.contains(x as f32, y as f32) && !crate::pointer::pointer_occluded();
        // Pressed clears once the pointer drags off the box, so it never sticks.
        if !inside {
            self.set_active(false);
        }
        if inside && let Some(cb) = &self.pointer.moved {
            cb(x as f32 - rect.x, y as f32 - rect.y);
        }
        let tracks_hover = self.state.hover.is_some()
            || self.pointer.hover.is_some()
            || self.pointer.cursor.is_some();
        if tracks_hover
            && matches!(source, PointerSource::Mouse)
            && inside != self.state.is_hovered.get()
        {
            self.state.is_hovered.set(inside);
            if let Some(cursor) = &self.pointer.cursor {
                platform_core::push_window_command(WindowCommand::SetCursor(if inside {
                    cursor.get()
                } else {
                    Cursor::Default
                }));
            }
            if let Some(cb) = &self.pointer.hover {
                cb(inside);
            }
            return EventResult::Handled;
        }
        if dragged { EventResult::Handled } else { child }
    }

    /// A press: who claims the stroke, who takes the tap, and who takes focus.
    fn on_pointer_pressed(
        &mut self,
        event: &Event,
        rect: Rect,
        x: f64,
        y: f64,
        button: &PointerButton,
    ) -> EventResult {
        // Pressed state and focus are primary-only. Other buttons route to the press or drag gesture of a box that asked for them, and fall through untouched otherwise.
        let primary = *button == PointerButton::Primary;
        if !primary && !self.press.wants_alt() && !self.drag.arms(button) {
            return self.dispatch_children(event);
        }
        // A child takes the tap; the innermost drag takes the stroke. Standing this drag down because a child took the press made a strip draggable only where nothing pressable sat in it; arming it regardless moved the band and reordered the tab on one press. So the children are asked who claimed the stroke.
        let (below, claimed) = crate::drag::claimed(|| self.dispatch_children(event));
        // Said after the children have had the press and before this returns, so it reaches whatever contains this box.
        if self.holds_stroke && rect.contains(x as f32, y as f32) {
            crate::drag::claim();
        }
        if below == EventResult::Handled {
            self.press.cancel();
            if self.drag.is_set() && !claimed {
                self.drag.press(event, rect);
            }
            return EventResult::Handled;
        }
        if primary && rect.contains(x as f32, y as f32) {
            self.set_active(true);
        }
        let focused = match self.focusable.id {
            Some(id) if primary && rect.contains(x as f32, y as f32) => {
                focus::request_from_pointer(id);
                true
            }
            _ => false,
        };
        let tapped = self.press.is_set() && self.press.arm(event, rect) == EventResult::Handled;
        let dragged =
            !claimed && self.drag.is_set() && self.drag.press(event, rect) == EventResult::Handled;
        if tapped || dragged || focused {
            EventResult::Handled
        } else {
            EventResult::Ignored
        }
    }

    /// A release: the end of the tap, of the drag, or of neither.
    fn on_pointer_released(
        &mut self,
        event: &Event,
        rect: Rect,
        button: &PointerButton,
    ) -> EventResult {
        let primary = *button == PointerButton::Primary;
        if !primary && !self.press.wants_alt() && !self.drag.arms(button) {
            return self.dispatch_children(event);
        }
        if primary {
            self.set_active(false);
        }
        if self.dispatch_children(event) == EventResult::Handled {
            self.press.cancel();
            self.drag.end(None);
            return EventResult::Handled;
        }
        // The release carries the position the gesture actually finished at; a drag can end past the last move the compositor delivered.
        let released_at = match event {
            Event::PointerReleased { x, y, .. } => Some((*x as f32 - rect.x, *y as f32 - rect.y)),
            _ => None,
        };
        let dragged = self.drag.arms(button) && self.drag.end(released_at);
        let tapped = self.press.is_set() && self.press.release(event, rect) == EventResult::Handled;
        if tapped || dragged {
            EventResult::Handled
        } else {
            EventResult::Ignored
        }
    }

    /// Whether the box wants nothing from an event and can route it straight to its children, exactly as a plain container would.
    ///
    /// One question per group rather than one term per field: this predicate was a ten-term disjunction amended in ten commits, two of them fixing the omission the shape invites — a box whose only claim was a cursor, and one whose only claim was `on_key`, each silently lost its events.
    fn is_inert(&self) -> bool {
        !self.press.is_set()
            && !self.drag.is_set()
            && !self.state.repaints_on_pointer()
            && !self.pointer.is_set()
            && self.on_key.is_none()
            && self.focusable.id.is_none()
            // Holding the stroke is something a box does with a press, though it answers none.
            && !self.holds_stroke
    }

    fn dispatch_children(&mut self, event: &Event) -> EventResult {
        match &self.dyn_host {
            Some(host) => host.dispatch(event),
            None => dispatch_container_event(&mut self.children, event),
        }
    }

    pub fn with_opacity(mut self, opacity: impl Fn() -> f32 + 'static) -> Self {
        self.opacity = Some(Box::new(opacity));
        self
    }

    /// Apply an affine transform (rotate/scale/translate) to the whole box each `view()`. The closure takes the laid-out rect and returns the 2×3 matrix, or `None` for identity.
    pub fn with_transform(
        mut self,
        transform: impl Fn(Rect) -> Option<[f32; 6]> + 'static,
    ) -> Self {
        self.transform = Some(Box::new(transform));
        self
    }

    /// Paint the box with `f` while the mouse hovers it (a declarative style swap, like `Button`). Hover is mouse-only; touch never sets it, so a tap leaves no stuck hover state.
    pub fn hover_style(mut self, f: impl Fn(Rect) -> RectStyle + 'static) -> Self {
        self.state.hover = Some(Box::new(f));
        self
    }

    /// Paint the box with `f` while a primary pointer is held down inside it — the pressed / CSS `:active` state, which takes precedence over `hover_style`. Unlike hover it tracks touch as well as mouse, and it clears on release, on leaving the box, or once the press drags off, so it never sticks.
    pub fn active_style(mut self, f: impl Fn(Rect) -> RectStyle + 'static) -> Self {
        self.state.active = Some(Box::new(f));
        self
    }

    /// Marks the box unusable while `f` reads true: it stops taking the pointer, stops tracking hover and the pressed state, stops showing its [`cursor`](Self::cursor), and paints its [`disabled_style`](Self::disabled_style) ahead of every other state.
    ///
    /// Closed here rather than left to each widget because the web never asks anyone to write it: `disabled` is platform semantics that a selector and the hit-tester read for free, and a catalogue that made every component re-implement it would get a different subset right in each one. The failure it prevents is small and immediate — a control the application has already disabled still lighting up under the pointer and still showing a hand cursor, which says "press me" about something that will do nothing.
    pub fn disabled(mut self, f: impl Fn() -> bool + 'static) -> Self {
        let f = std::rc::Rc::new(f);
        self.disabled_source = Some({
            let f = f.clone();
            Box::new(move || f())
        });
        // The same mechanism a hidden overlay uses, so a disabled wrapper reads as a `fieldset` to the keyboard too rather than shielding the mouse and leaving Tab a way in.
        self.focusable.scope = Some(focus::register_scope_because(
            self.node,
            move || !f(),
            false,
            focus::ScopeReason::Disabled,
        ));
        self
    }

    /// The paint for the disabled state, which wins over the pressed and hover ones.
    pub fn disabled_style(mut self, f: impl Fn(Rect) -> RectStyle + 'static) -> Self {
        self.state.disabled = Some(Box::new(f));
        self
    }

    /// The focus ring: drawn over whichever state won, while the box holds focus *and* should show it.
    ///
    /// Composed rather than swapped, unlike the other three, and the difference is the point. Hover, pressed and disabled are answers to "what is this box doing", so one of them replaces the rest. A ring answers a different question — where the keyboard is going — and a hovered box that lost its ring would hide that answer at the exact moment the user reached for the mouse. Which is why CSS gives focus its own property (`outline`) rather than another background.
    ///
    /// Only the properties the ring names are applied; `radius` always comes from the box, since a ring sits on a shape it does not get to reshape. Shown on [`focus::is_focus_visible`](crate::focus), so a tap takes focus without drawing one. Declares this box a control, and is the way to build one.
    ///
    /// One call because the three halves are one fact, and each alone is a control that does not work: a box that takes a tap but never a key, a ring on something Tab cannot reach, a thing announced to a screen reader that cannot say what it is. Splitting them across three optional builders is how nine catalogue components shipped answering the mouse and nothing else — every one of them compiled, and looked right.
    ///
    /// It joins the tab order at this node, answers Enter and Space the way it answers a tap, draws the theme's focus ring while the keyboard is what reached it (see [`focus::is_focus_visible`](crate::focus::is_focus_visible)), and reports `role` outwards. A caller that wants a ring of its own still says so with [`focus_style`](Self::focus_style); this only supplies one when nothing else has.
    ///
    /// Deliberately not implied by [`on_press`](Self::on_press): a scrim, a click-away backdrop and a drag surface all take presses and none of them is a place the keyboard should stop.
    pub fn control(mut self, role: focus::Role) -> Self {
        self.role = Some(role);
        let id = *self.focusable.id.get_or_insert_with(focus::next_id);
        focus::register_with_role(id, focus::FocusKind::Widget, self.node, role);
        self.focusable.activates = true;
        if self.state.focus.is_none() {
            self.state.focus = Some(Box::new(|_r| default_focus_ring()));
        }
        self.mark_interactive();
        self
    }

    /// What this box *is*, beyond a box: a region of the screen, a list, an article.
    ///
    /// Description only — it does not join the tab order, because a region is not a place the keyboard stops. [`control`](Self::control) is the one that declares a role *and* makes it focusable.
    pub fn role(mut self, role: renderer_core::Role) -> Self {
        self.role = Some(role);
        self
    }

    /// Declares that this control carries a checked state, and how to read it.
    ///
    /// Only meaningful after [`control`](Self::control), and only for the roles that have one. Without it a reader announces "checkbox" and stops — and a default of "unticked" would be worse, since it would be confidently wrong for half of them.
    pub fn toggled(self, state: impl Fn() -> bool + 'static) -> Self {
        if let Some(id) = self.focusable.id {
            focus::set_toggled(id, state);
        }
        self
    }

    /// Declares the number this box carries, so a reader says where a slider stands and not only that it is one. The counterpart of [`toggled`](Self::toggled) for a control whose state is a value.
    pub fn valued(self, read: impl Fn() -> NumericValue + 'static) -> Self {
        if let Some(id) = self.focusable.id {
            focus::set_value(id, read);
        }
        self
    }

    pub fn focus_style(mut self, f: impl Fn(Rect) -> RectStyle + 'static) -> Self {
        // Declaring a ring declares the box focusable, or it would join no tab order and nothing could satisfy the style.
        let id = *self.focusable.id.get_or_insert_with(focus::next_id);
        focus::register_at(id, focus::FocusKind::Widget, self.node);
        self.state.focus = Some(Box::new(f));
        self
    }

    /// Whether the box is currently pressed (a primary pointer is held down inside it). Set only when an `active_style` is present; drives its paint swap and clears on release/leave/drag-off.
    fn set_active(&self, active: bool) {
        if self.state.active.is_some() && self.state.is_active.get() != active {
            self.state.is_active.set(active);
        }
    }

    /// Drops everything that meant *the pointer is inside this box*: hover, the pressed look, and a tap still waiting for a release within the bounds. Deliberately not the drag — that one is measured from the press and does not care where the pointer has wandered to.
    fn end_containment(&mut self) {
        self.press.cancel();
        self.set_active(false);
        let tracks_hover = self.state.hover.is_some()
            || self.pointer.hover.is_some()
            || self.pointer.cursor.is_some();
        if tracks_hover && self.state.is_hovered.get() {
            self.state.is_hovered.set(false);
            // Nothing else restores the shape while the pointer is still inside the window.
            if self.pointer.cursor.is_some() {
                platform_core::push_window_command(WindowCommand::SetCursor(Cursor::Default));
            }
            if let Some(cb) = &self.pointer.hover {
                cb(false);
            }
        }
    }

    /// Shows `cursor` while the pointer is over this box, and restores the default when it leaves.
    ///
    /// The shape is the app's statement of what the next press will do — orbit, resize a panel, place a point — so it belongs to the widget that would handle that press, not to a mode the app tracks.
    ///
    /// **A shape is as often worked out as it is written**: one strip resizes a column and the same component resizes a row. So it takes a [`Reactive<Cursor>`] — a `Cursor` converts into one, so a literal call site says exactly what it did before — and one that reads follows what it reads, including while the pointer is already inside: the shape is a fact about the box, not about the crossing.
    pub fn cursor(mut self, cursor: impl Into<Reactive<Cursor>>) -> Self {
        let cursor = cursor.into();
        if matches!(cursor, Reactive::Read(_)) {
            let (hovered, held) = (self.state.is_hovered, cursor.clone());
            effect(move || {
                let shape = held.get();
                if hovered.get() {
                    platform_core::push_window_command(WindowCommand::SetCursor(shape));
                }
            });
        }
        self.pointer.cursor = Some(cursor);
        self
    }

    /// Declares that this box does not stand between the pointer and whatever it is drawn over — CSS's `pointer-events: none`, and the second consumer of the hook [`Overlay`](crate::Overlay) opened.
    ///
    /// A box covers what is behind it: since the hit-test walks in paint order, the topmost child under the pointer takes the event whether or not it wants it. That is right for a panel and wrong for a *label* — a readout floating over a canvas, a badge over a photo, a drag ghost — which is drawn on top precisely so it can be read, and whose whole contract is that the thing underneath still works. A modeller's transform readout sits across the top of the viewport it reports on; without this, moving the pointer under it stops the operation it is describing.
    ///
    /// It is a property of *this* box only. Children still hit-test normally, so a click-through bar can hold a real button — the same split CSS makes with `pointer-events: auto` on a child.
    pub fn click_through(mut self, through: bool) -> Self {
        self.click_through = through;
        self
    }

    /// A stroke that starts on this box is this box's, and goes no further out.
    ///
    /// **The other half of «the innermost drag owns the stroke».** A band that moves the window is dragged by its empty space, and the controls sitting in it are not empty space — but a button claims nothing, so its press was the band.s and the compositor took the pointer away to move the window before the button could answer. The same shape holds for a menu panel over a dismissing backdrop, and for any control inside anything draggable.
    ///
    /// Not `on_drag` with an empty body, which is what this replaces: that says «I drag, and do nothing», and what is meant is «this one is mine». It claims for every button, because a stroke is a stroke whichever one started it.
    pub fn holds_stroke(mut self) -> Self {
        self.holds_stroke = true;
        self
    }

    /// Give this widget ownership of an [`Effect`], so it runs for exactly as long as the widget exists.
    ///
    /// The reactive runtime scopes an effect to the *surface* it was registered on, which is the right span for a shell-wide subscription and far too coarse for one row of a list: the row goes, the effect stays, and it keeps firing at a node that is gone. Dropping the handle instead is the opposite failure — the effect deregisters, runs once, and stops, with nothing to say so. This is the third answer, and the one an effect that belongs to a widget wants.
    ///
    /// Says what the text below this box looks like — see [`Container::declaring`](crate::Container::declaring).
    pub fn declaring(self, declared: impl Fn() -> Declared + 'static) -> Self {
        let node = self.node;
        effect(move || crate::inherit::declare(node, declared()));
        self
    }

    /// Keeps the box's *layout* style in step with the reactive state it was built from — the theme's metric tokens, today. `style` runs now, and again whenever a signal it read changes; the node is restyled in place, so a live theme switch re-spaces the box as well as re-colouring it.
    ///
    /// Paint needs nothing like this: a rect or text style is a closure the renderer re-runs every frame, so a token read inside one is already live. A layout style is a *value*, handed to the layout tree once when the node is made — which is why the reactive read has to be arranged here rather than coming for free.
    ///
    /// Give [`new`](Self::new) the same builder, so the node starts at the style it will settle on: `StyledContainer::new(shell(), paint, kids)?.styled_by(shell)`.
    pub fn styled_by(self, style: impl Fn() -> LayoutStyle + 'static) -> Self {
        let node = self.node;
        style_follows(node, style);
        self
    }

    /// Make the box itself pressable. The callback fires on a tap (release, not press) inside the box; a child widget that handles the press wins, and a scroll gesture started on the box does not fire it.
    pub fn on_press(self, f: impl Fn() + 'static) -> Self {
        self.maybe_on_press(Some(f))
    }

    /// [`on_press`](Self::on_press) for a handler the caller may not have supplied.
    ///
    /// What a wrapper component needs to forward an optional callback. A box whose press handler is a no-op still reports the tap `Handled`, so "no handler" would become "swallows the click" — a display-only chip eating a press instead of letting it through. `None` leaves the box exactly as it was; the `maybe_*` pairs below say the same for every other event whose absence the box can observe.
    pub fn maybe_on_press(mut self, f: Option<impl Fn() + 'static>) -> Self {
        let Some(f) = f else { return self };
        self.press.set(f);
        self.mark_interactive();
        self
    }

    /// Fire `f(button)` on a tap with a **non-primary** button — `Secondary` (right) or `Auxiliary` (middle). Same tap-on-release semantics as [`Self::on_press`]: a child that handles the press wins, and travel past the tap slop cancels it.
    ///
    /// Opt-in per box rather than folded into `on_press`, because a non-primary press otherwise falls through to whatever is behind it — silently swallowing right-clicks on every pressable box would break that.
    pub fn on_alt_press(self, f: impl Fn(PointerButton) + 'static) -> Self {
        self.maybe_on_alt_press(Some(f))
    }

    /// [`on_alt_press`](Self::on_alt_press) for a handler the caller may not have supplied.
    pub fn maybe_on_alt_press(mut self, f: Option<impl Fn(PointerButton) + 'static>) -> Self {
        let Some(f) = f else { return self };
        self.press.set_alt_press(f);
        self.mark_interactive();
        self
    }

    /// Fires once a press inside the box is held past ~500ms without moving past the tap slop, instead of `on_press`'s tap-on-release. There is no dedicated timer in the gesture pipeline, so the threshold is only checked on the next pointer event after the press (a move or the release) — it fires slightly late, never at exactly 500ms, and a release before that next check-in is a normal tap.
    pub fn on_long_press(self, f: impl Fn() + 'static) -> Self {
        self.maybe_on_long_press(Some(f))
    }

    /// [`on_long_press`](Self::on_long_press) for a handler the caller may not have supplied.
    pub fn maybe_on_long_press(mut self, f: Option<impl Fn() + 'static>) -> Self {
        let Some(f) = f else { return self };
        self.press.set_long_press(f);
        self.mark_interactive();
        self
    }

    /// Make the box draggable. The callback fires with the pointer position (layout space) on a press inside the box and on every move until release — even after the pointer leaves the box. Map the coordinate to a value (slider) or an offset (reorder/resize). Fires once when a drag started on this box ends, with the position it finished at (layout space, local to the box, same as [`on_drag`](Self::on_drag)).
    ///
    /// This is what makes a *threshold* gesture expressible: `on_drag` alone reports where the pointer is but never that it let go, so a swipe-to-dismiss or a drag-to-open can be tracked and never decided. A drag also ends when the pointer leaves the window or a child consumes the release; those carry no position, so the last one the drag reached is reported instead — the gesture always ends exactly once.
    pub fn on_drag_end(self, f: impl Fn(f32, f32) + 'static) -> Self {
        self.maybe_on_drag_end(Some(f))
    }

    /// [`on_drag_end`](Self::on_drag_end) for a handler the caller may not have supplied.
    pub fn maybe_on_drag_end(mut self, f: Option<impl Fn(f32, f32) + 'static>) -> Self {
        let Some(f) = f else { return self };
        self.drag.set_end(f);
        self.mark_interactive();
        self
    }

    pub fn on_drag(self, f: impl Fn(f32, f32) + 'static) -> Self {
        self.maybe_on_drag(Some(f))
    }

    /// [`on_drag`](Self::on_drag) for a handler the caller may not have supplied.
    pub fn maybe_on_drag(mut self, f: Option<impl Fn(f32, f32) + 'static>) -> Self {
        let Some(f) = f else { return self };
        self.drag.set(f);
        self.mark_interactive();
        self
    }

    /// Let `button` start the drag too, on top of the primary one that always does.
    ///
    /// A slider or a splitter wants exactly one button and gets it by default. A surface with more than one thing to drag needs the others: a modeller orbits with the primary button and pans with the secondary, which is what the OS and every 3D application call those gestures. The handler is the same one — read [`crate::pointer_buttons`] inside it to tell which button is doing the dragging.
    pub fn drag_button(mut self, button: platform_core::PointerButton) -> Self {
        self.drag.arm_with(&button);
        self
    }

    /// How far the pointer must travel before this box counts as being dragged.
    ///
    /// Without it a press *is* a drag from its first instant, which is right for a slider — pressing the track is how you set the value — and wrong for anything where a click and a drag mean different things on the same button. A viewport is the case: a click picks what is under it, a drag orbits, and telling them apart is the difference between selecting something and nudging the camera by a pixel.
    ///
    /// Set it and the two stop overlapping: a stroke that never travels this far fires only [`on_press`](Self::on_press), one that does fires only the drag handlers, and neither fires both.
    pub fn drag_threshold(mut self, px: f32) -> Self {
        self.drag.set_threshold(px);
        self
    }

    /// Holds the drag to one axis: the other coordinate is reported as it stood when the press landed.
    ///
    /// What a gesture with one meaning owes its reader. A strip reordered along its own axis, a slider, a splitter — each takes one number and throws the other away, and the ones that forget let what they are dragging wander off the line it lives on.
    pub fn drag_axis(mut self, axis: crate::drag::DragAxis) -> Self {
        self.drag.lock_to(axis);
        self
    }

    /// Keeps the reported point inside `bounds`, in this box's own coordinates.
    ///
    /// A drag goes on receiving moves after the pointer has left the widget — that is what keeps a slider tracking when the hand overshoots — and the same broadcast is what lets a pointer dragged out of the window report a place no layout could produce. This is where a caller says how far out the answer may go: once, rather than at every use. Read on each report, so a box that resizes takes its bounds with it.
    pub fn drag_within(mut self, bounds: impl Fn() -> Rect + 'static) -> Self {
        self.drag.keep_within(bounds);
        self
    }

    /// Records this box as a pointer target in the per-surface interactive registry, so a surface that carves its input region from its content (a click-through overlay) receives input over it. See [`crate::interactive_rects`].
    fn mark_interactive(&self) {
        crate::input_region::register_interactive(self.node, self.rect.read_only());
    }

    /// Fire `f(true)` when the mouse enters the box and `f(false)` when it leaves (mouse only). Independent of `hover_style`: a box can observe hover without swapping its paint.
    ///
    /// Registers the box as a pointer target, like [`on_scroll`](Self::on_scroll) does for the same reason: a surface that carves its input region from its content (a click-through overlay) never receives a move event over a box it left out of that region, so a hover it did not register is a hover it can't observe.
    pub fn on_hover(self, f: impl Fn(bool) + 'static) -> Self {
        self.maybe_on_hover(Some(f))
    }

    /// [`on_hover`](Self::on_hover) for a handler the caller may not have supplied.
    pub fn maybe_on_hover(mut self, f: Option<impl Fn(bool) + 'static>) -> Self {
        let Some(f) = f else { return self };
        self.pointer.hover = Some(Box::new(f));
        self.mark_interactive();
        self
    }

    /// Fire `f(x, y)` with the pointer position — local to the box, as [`on_drag`](Self::on_drag) reports it — on every move over it.
    ///
    /// The continuous half of [`on_hover`](Self::on_hover), which reports only the crossings. It is what a surface that answers to *where* the pointer is needs: highlighting the face under the cursor, previewing a snap, stretching a dimension line. Fires for touch as well as mouse, since a drag on a touchscreen asks the same question.
    pub fn on_pointer_move(self, f: impl Fn(f32, f32) + 'static) -> Self {
        self.maybe_on_pointer_move(Some(f))
    }

    /// [`on_pointer_move`](Self::on_pointer_move) for a handler the caller may not have supplied.
    pub fn maybe_on_pointer_move(mut self, f: Option<impl Fn(f32, f32) + 'static>) -> Self {
        let Some(f) = f else { return self };
        self.pointer.moved = Some(Box::new(f));
        self.mark_interactive();
        self
    }

    /// Fire `f(dx, dy)` with the wheel delta while the pointer is over the box — scroll-to-adjust on a control (a volume or brightness chip, a stepper), or zoom on a viewport. Deltas are normalised to pixels, matching [`LayoutScrollArea`](crate::LayoutScrollArea): a line delta counts as 20px, so one wheel notch is roughly ±60.
    ///
    /// Targeted by hit-testing the wheel's own position, so it answers a wheel that arrives before the pointer has moved at all. A scrollable child (a scroll area inside the box) gets first refusal and keeps it.
    pub fn on_scroll(self, f: impl Fn(f32, f32) + 'static) -> Self {
        self.maybe_on_scroll(Some(f))
    }

    /// [`on_scroll`](Self::on_scroll) for a handler the caller may not have supplied.
    pub fn maybe_on_scroll(mut self, f: Option<impl Fn(f32, f32) + 'static>) -> Self {
        let Some(f) = f else { return self };
        self.pointer.scroll = Some(Box::new(f));
        self.mark_interactive();
        self
    }

    /// Fire `f(&key)` on every key press. This is a GLOBAL handler (key events reach every widget; there is no per-widget focus), so it suits app-level shortcuts, not focused text entry.
    ///
    /// It stands aside while a text entry holds focus and the press is text it would take ([`focus::text_entry_takes_key`]) — so a shortcut on `3` does not also fire when the user types `3` into a field, while `⌘S` still reaches it. Read the modifiers with [`crate::modifiers`]: key events carry them, but pointer events do not, so the state registry is the one answer that works everywhere.
    ///
    /// Return `bool` from `f` rather than `()` to say whether the shortcut took the key — see [`KeyAnswer`]. That is what a table binding a key the runtime also uses needs: `Tab` enters the focus order when nothing claims it, so a handler that answers `()` gets both its own action and the focus move.
    pub fn on_key<A: KeyAnswer>(self, f: impl Fn(&Key) -> A + 'static) -> Self {
        self.maybe_on_key(Some(f))
    }

    /// [`on_key`](Self::on_key) for a handler the caller may not have supplied.
    pub fn maybe_on_key<A: KeyAnswer>(mut self, f: Option<impl Fn(&Key) -> A + 'static>) -> Self {
        let Some(f) = f else { return self };
        self.on_key = Some(Box::new(move |key| f(key).took()));
        self
    }

    /// Make the box focusable and fire `f(true)`/`f(false)` when it gains/loses keyboard focus. It joins the tab order (Tab/Shift-Tab reach it) and takes focus on tap. Use it to drive a focus ring or to build a custom focusable widget on top of a `box`.
    pub fn on_focus(self, f: impl Fn(bool) + 'static) -> Self {
        self.maybe_on_focus(Some(f))
    }

    /// [`on_focus`](Self::on_focus) for a handler the caller may not have supplied.
    pub fn maybe_on_focus(mut self, f: Option<impl Fn(bool) + 'static>) -> Self {
        let Some(f) = f else { return self };
        let id = *self.focusable.id.get_or_insert_with(focus::next_id);
        focus::register_at(id, focus::FocusKind::Widget, self.node);
        // The first run seeds `last` and does not fire, so the callback sees only real transitions.
        let last = std::rc::Rc::new(std::cell::Cell::new(focus::is_focused(id)));
        self.focusable._effect = Some(effect(move || {
            let now = focus::is_focused(id);
            if now != last.get() {
                last.set(now);
                f(now);
            }
        }));
        self
    }
}

impl LayoutItem for StyledContainer {
    fn layout_node(&self) -> NodeId {
        self.node
    }

    fn pointer_opaque(&self) -> bool {
        !self.click_through
    }
}

impl Component for StyledContainer {
    fn view(&self) -> RenderNode {
        let r = self.rect.get();
        let style = PAINT_STATES
            .iter()
            .find_map(|&state| self.state_style(state))
            .unwrap_or(&*self.style);
        let painted = match (&self.state.focus, self.focusable.id) {
            (Some(ring), Some(id)) if focus::is_focus_visible(id) => {
                let base = style(r);
                let ring = ring(r);
                RectStyle {
                    fill: ring.fill.or(base.fill),
                    border: ring.border.or(base.border),
                    shadow: ring.shadow.or(base.shadow),
                    // The ring sits on the box's shape rather than choosing one of its own.
                    radius: base.radius,
                }
            }
            _ => style(r),
        };
        let background = RenderNode::rect(
            Rect {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            },
            painted,
        );
        let content = match &self.dyn_host {
            Some(host) => {
                RenderNode::group(std::iter::once(background).chain(host.child_boundaries()))
            }
            None => RenderNode::group(
                std::iter::once(background)
                    .chain(self.children.iter().map(|c| c.segment.boundary())),
            ),
        };
        let opacity = self.opacity.as_ref().map_or(1.0, |o| o());
        let composed = if opacity < 1.0 {
            RenderNode::layer(opacity, 0.0, [content])
        } else {
            content
        };
        let placed = match self.transform.as_ref().and_then(|t| t(r)) {
            Some(matrix) => RenderNode::transform_with(matrix, [composed]),
            None => composed,
        };
        // The element wraps everything, so a document backend folds the transform and the layer into the box's own style rather than inventing a wrapper for each.
        if ui_tree::element_capture() {
            RenderNode::element(self.element(), [placed])
        } else {
            placed
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // Ahead of the pure-routing bail below, which a wrapper with no handlers would otherwise take. The state it was showing goes with it, or a box disabled mid-hover keeps a highlight it can no longer honour.
        if self.is_disabled() {
            return match event {
                Event::PointerMoved { .. }
                | Event::PointerPressed { .. }
                | Event::PointerReleased { .. }
                | Event::Scrolled { .. } => {
                    self.end_containment();
                    self.drag.end(None);
                    EventResult::Ignored
                }
                _ => self.dispatch_children(event),
            };
        }
        if self.is_inert() {
            return self.dispatch_children(event);
        }
        let rect = self.rect.get();
        match event {
            Event::PointerMoved { x, y, source } => {
                self.on_pointer_moved(event, rect, *x, *y, source)
            }
            Event::PointerPressed { x, y, button, .. } => {
                self.on_pointer_pressed(event, rect, *x, *y, button)
            }
            Event::PointerReleased { button, .. } => self.on_pointer_released(event, rect, button),
            // A drag is measured from the press, so ending it at the window border would cut an orbit short. What leaving does invalidate is containment.
            Event::CursorLeft => {
                self.end_containment();
                self.dispatch_children(event)
            }
            // Where a live gesture really must end: a window losing focus never sends the release for what was held.
            Event::FocusChanged { is_focused: false } => {
                self.end_containment();
                self.drag.end(None);
                self.dispatch_children(event)
            }
            // Children get first refusal; only then does an `on_scroll` box under the wheel consume it.
            Event::Scrolled { delta, x, y } => {
                if self.dispatch_children(event) == EventResult::Handled {
                    return EventResult::Handled;
                }
                let Some(cb) = &self.pointer.scroll else {
                    return EventResult::Ignored;
                };
                if !rect.contains(*x as f32, *y as f32) {
                    return EventResult::Ignored;
                }
                let (dx, dy) = delta.pixels();
                cb(dx, dy);
                EventResult::Handled
            }
            Event::KeyPressed { key, modifiers } => {
                if let Some(id) = self.focusable.id
                    && focus::is_focused(id)
                    && matches!(key, Key::Named(NamedKey::Tab))
                {
                    if modifiers.is_shift {
                        focus::focus_prev();
                    } else {
                        focus::focus_next();
                    }
                    return EventResult::Handled;
                }
                // Consumed only when there was something to fire, so Space on a box with no press handler still reaches whatever else wanted it. Skipped for a box that handles its own keys: a dropdown trigger answers Enter by confirming a highlighted row, not by re-opening itself.
                if self.focusable.activates
                    && self.on_key.is_none()
                    && let Some(id) = self.focusable.id
                    && focus::is_focused(id)
                    && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Space))
                    && self.press.activate()
                {
                    return EventResult::Handled;
                }
                // Not while a field has the caret: this is the app's shortcut table, which would otherwise fire on every letter typed.
                if let Some(cb) = &self.on_key
                    && !focus::text_entry_takes_key(key, *modifiers)
                    && cb(key)
                {
                    return EventResult::Handled;
                }
                self.dispatch_children(event)
            }
            _ => self.dispatch_children(event),
        }
    }

    fn debug_name(&self) -> &'static str {
        "StyledContainer"
    }
}

impl Drop for StyledContainer {
    fn drop(&mut self) {
        // Before releasing focus below, so `on_focus` does not fire during teardown.
        self.focusable._effect.take();
        if let Some(id) = self.focusable.id {
            focus::unregister(id);
        }
        if let Some(scope) = self.focusable.scope {
            focus::unregister_scope(scope);
        }
        crate::input_region::unregister_interactive(self.node);
    }
}

/// The ring a control wears when the keyboard is what reached it, unless it asked for one of its own.
///
/// A ring and not a fill, because it answers a different question from hover or pressed — *where the keys are going*, not what the box is doing — and has to survive being layered over whichever of those won. Its radius is deliberately absent: the compositing path takes that from the box, since a ring sits on a shape it does not get to reshape.
fn default_focus_ring() -> RectStyle {
    RectStyle::default().with_border(Border::uniform(use_theme_tokens().primary(), 2.0))
}

/// Builds the affine matrix for a box's declarative `rotate`/`scale`/`translate` attributes, pivoting rotation and scale on the box centre. Returns `None` when every component is identity, so an untransformed box skips the extra transform node entirely.
pub fn box_transform(
    rect: Rect,
    rotate_deg: f32,
    scale_x: f32,
    scale_y: f32,
    translate_x: f32,
    translate_y: f32,
) -> Option<[f32; 6]> {
    if rotate_deg == 0.0
        && scale_x == 1.0
        && scale_y == 1.0
        && translate_x == 0.0
        && translate_y == 0.0
    {
        return None;
    }
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let matrix = Transform::rotate_around(rotate_deg, cx, cy)
        .then(Transform::scale_around(scale_x, scale_y, cx, cy))
        .then(Transform::translate(translate_x, translate_y));
    Some(matrix.to_array())
}

#[cfg(test)]
#[path = "styled_container_test.rs"]
mod tests;

#[cfg(test)]
#[path = "styled_container_paint_state_test.rs"]
mod paint_state_tests;
