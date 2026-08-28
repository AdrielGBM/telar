use geometry_core::{Rect, Transform};
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::{
    Cursor, Event, Key, NamedKey, NumericValue, PointerButton, PointerSource, WindowCommand,
};
use reactive_core::{Effect, RwSignal, effect, signal};
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

/// Re-resolves `node`'s layout style whenever the reactive state `style` reads changes, and once now.
///
/// The general form of [`StyledContainer::styled_by`], for a widget that is not a container — a text leaf sized
/// off the theme's `font_size`, a slider thumb sized off its `spacing`. The returned [`Effect`] must be held for
/// as long as the node lives, which for a leaf means its owning container `keeping` it.
pub fn style_follows(node: NodeId, style: impl Fn() -> LayoutStyle + 'static) -> Effect {
    effect(move || {
        let _ = crate::context::set_layout_style(node, style());
    })
}

/// The paint a box swaps in per state, and the state itself.
///
/// One value so "does this box repaint on a pointer transition" is a question with an owner, instead of a
/// term someone has to remember to add to a disjunction spelled out at the top of `on_event`.
struct StateStyle {
    // Swapped in while the pointer is over the box (mouse only), mirroring `Button`'s rect/rect_hover.
    hover: Option<Box<dyn Fn(Rect) -> RectStyle>>,
    is_hovered: RwSignal<bool>,
    // Swapped in while a primary pointer is held down inside the box (the pressed / CSS `:active` state),
    // taking precedence over `hover`. Mouse and touch; cleared on release, leave, or drag-off.
    active: Option<Box<dyn Fn(Rect) -> RectStyle>>,
    is_active: RwSignal<bool>,
    // Swapped in ahead of every other state: a control that cannot be used must not also look pressable.
    disabled: Option<Box<dyn Fn(Rect) -> RectStyle>>,
    // Laid *over* whichever state won, not instead of it — see `focus_style`.
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
    // Fires with `true`/`false` as the mouse enters/leaves the box (mouse only, like the hover style).
    hover: Option<Box<dyn Fn(bool)>>,
    // Fires with the pointer position, local to the box, on every move over it. The continuous half of `hover`, which only reports the crossings.
    moved: Option<Box<dyn Fn(f32, f32)>>,
    // Fires with the wheel delta while the pointer is over the box.
    scroll: Option<Box<dyn Fn(f32, f32)>>,
    // Pointer shape while the box is hovered; restored to the default on leave. Set from `cursor:` in the DSL.
    cursor: Option<Cursor>,
}

impl PointerHooks {
    /// Whether anything here needs the pointer's moves. `cursor` counts: a box whose only claim is a shape
    /// still has to see the crossings that set and clear it.
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
    // When set, the box is focusable: it joins the tab order, takes focus on tap, and handles Tab while
    // focused. `on_focus` observes the transitions.
    id: Option<FocusId>,
    // Watches focus transitions for `on_focus`; dropping it (with the box) tears the subscription down.
    _effect: Option<Effect>,
    // Whether Enter and Space fire the press the way a tap does. Set by `control`, because a control that
    // answers a mouse and not a keyboard is the failure this whole path exists to make unspellable.
    activates: bool,
    // Registered when the box is given a `disabled` source; withdrawn on drop.
    scope: Option<focus::ScopeId>,
}

pub struct StyledContainer {
    node: NodeId,
    rect: RwSignal<Rect>,
    style: Box<dyn Fn(Rect) -> RectStyle>,
    state: StateStyle,
    // A closure (like `opacity`) so `view()` and the pointer path both re-read it: whether a control is usable is state that moves. `None` is the common case and skips the call on the pointer-move path.
    disabled_source: Option<Box<dyn Fn() -> bool>>,
    // A closure (not a plain f32) so `view()` re-reads it every run: a reactive opacity or a `transition:opacity` animation resolves to its current value on each re-render. `None` means fully opaque.
    opacity: Option<Box<dyn Fn() -> f32>>,
    // Resolved per `view()` (like `opacity`) so a `$signal`-driven transform re-reads its current value. Takes the laid-out `Rect` so rotate/scale can pivot on the box centre; `None` means identity (no wrapping node).
    transform: Option<Box<dyn Fn(Rect) -> Option<[f32; 6]>>>,
    children: TrackedChildren,
    // Set when the box holds a reactive fragment: static + dynamic children route through the host so
    // they interleave in this node (see `child_host`). `children` is empty in that case.
    dyn_host: Option<DynHost>,
    // Optional tap gesture so a styled box can itself be pressable (a clickable card); children still hit-test first.
    press: PressGesture,
    // Effects whose life is this widget's. Dropping an `Effect` deregisters it, so one that belongs to a widget must be owned by that widget: parked somewhere longer-lived it keeps firing against a node that is gone, and dropped on the floor it runs once and stops. Held here rather than in a wrapper so owning one costs no layout node — a row owning five effects is still one box.
    // Optional drag gesture (slider/reorder/resize): reports the pointer position on press and each move.
    drag: DragGesture,
    pointer: PointerHooks,
    // Fires on every key press. Key events carry no pointer position, so they are broadcast to every widget
    // — this is a GLOBAL shortcut handler (there is no per-widget focus), not focused text input.
    on_key: Option<Box<dyn Fn(&Key)>>,
    focusable: Focusable,
    // Whether the box declines to shadow what it is drawn over (`pointer-events: none`). Set from
    // `click_through` in the DSL; see `LayoutItem::pointer_opaque`.
    click_through: bool,
    // Whether a stroke that starts here is this box's and goes no further out. See `holds_stroke`.
    holds_stroke: bool,
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

    /// Everything a fresh box holds before any builder touches it; the two constructors differ only in
    /// where their children live.
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
        }
    }

    /// A styled box whose children are a mix of static widgets and reactive fragments (`ChildSlot`s),
    /// reconciled into this box's own node so they inherit its flex direction/gap — the transparent
    /// `box`-with-a-`for` path (see [`Container::from_slots`](crate::Container::from_slots)).
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

    /// Whether the box is currently refusing input. `None` — the common case — answers without a dyn call on
    /// the pointer-move broadcast path, which every box in the tree pays.
    fn is_disabled(&self) -> bool {
        self.disabled_source.as_ref().is_some_and(|f| f())
    }

    /// Whether the box wants nothing from an event and can route it straight to its children, exactly as a
    /// plain container would.
    ///
    /// One question per group rather than one term per field: this predicate was a ten-term disjunction
    /// amended in ten commits, two of them fixing the omission the shape invites — a box whose only claim
    /// was a cursor, and one whose only claim was `on_key`, each silently lost its events.
    fn is_inert(&self) -> bool {
        !self.press.is_set()
            && !self.drag.is_set()
            && !self.state.repaints_on_pointer()
            && !self.pointer.is_set()
            && self.on_key.is_none()
            && self.focusable.id.is_none()
            // Holding the stroke is something a box *does* with a press, though it answers none — and the third time this shape has cost an omission, exactly as the note above says it would.
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

    /// Apply an affine transform (rotate/scale/translate) to the whole box each `view()`. The closure
    /// takes the laid-out rect and returns the 2×3 matrix, or `None` for identity.
    pub fn with_transform(
        mut self,
        transform: impl Fn(Rect) -> Option<[f32; 6]> + 'static,
    ) -> Self {
        self.transform = Some(Box::new(transform));
        self
    }

    /// Paint the box with `f` while the mouse hovers it (a declarative style swap, like `Button`).
    /// Hover is mouse-only; touch never sets it, so a tap leaves no stuck hover state.
    pub fn hover_style(mut self, f: impl Fn(Rect) -> RectStyle + 'static) -> Self {
        self.state.hover = Some(Box::new(f));
        self
    }

    /// Paint the box with `f` while a primary pointer is held down inside it — the pressed / CSS `:active`
    /// state, which takes precedence over `hover_style`. Unlike hover it tracks touch as well as mouse,
    /// and it clears on release, on leaving the box, or once the press drags off, so it never sticks.
    pub fn active_style(mut self, f: impl Fn(Rect) -> RectStyle + 'static) -> Self {
        self.state.active = Some(Box::new(f));
        self
    }

    /// Marks the box unusable while `f` reads true: it stops taking the pointer, stops tracking hover and
    /// the pressed state, stops showing its [`cursor`](Self::cursor), and paints its
    /// [`disabled_style`](Self::disabled_style) ahead of every other state.
    ///
    /// Closed here rather than left to each widget because the web never asks anyone to write it: `disabled`
    /// is platform semantics that a selector and the hit-tester read for free, and a catalogue that made every
    /// component re-implement it would get a different subset right in each one. The failure it prevents is
    /// small and immediate — a control the application has already disabled still lighting up under the
    /// pointer and still showing a hand cursor, which says "press me" about something that will do nothing.
    pub fn disabled(mut self, f: impl Fn() -> bool + 'static) -> Self {
        let f = std::rc::Rc::new(f);
        self.disabled_source = Some({
            let f = f.clone();
            Box::new(move || f())
        });
        // Tab is the question the pointer already answers here, so it goes through the same mechanism a hidden overlay uses — which gives a disabled *wrapper* the `fieldset` reading for the keyboard too, rather than shielding the mouse and leaving Tab a way in.
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
    /// Composed rather than swapped, unlike the other three, and the difference is the point. Hover, pressed
    /// and disabled are answers to "what is this box doing", so one of them replaces the rest. A ring answers
    /// a different question — where the keyboard is going — and a hovered box that lost its ring would hide
    /// that answer at the exact moment the user reached for the mouse. Which is why CSS gives focus its own
    /// property (`outline`) rather than another background.
    ///
    /// Only the properties the ring names are applied; `radius` always comes from the box, since a ring sits
    /// on a shape it does not get to reshape. Shown on [`focus::is_focus_visible`](crate::focus), so a tap
    /// takes focus without drawing one.
    /// Declares this box a control, and is the way to build one.
    ///
    /// One call because the three halves are one fact, and each alone is a control that does not work: a box
    /// that takes a tap but never a key, a ring on something Tab cannot reach, a thing announced to a screen
    /// reader that cannot say what it is. Splitting them across three optional builders is how nine catalogue
    /// components shipped answering the mouse and nothing else — every one of them compiled, and looked right.
    ///
    /// It joins the tab order at this node, answers Enter and Space the way it answers a tap, draws the
    /// theme's focus ring while the keyboard is what reached it (see
    /// [`focus::is_focus_visible`](crate::focus::is_focus_visible)), and reports `role` outwards. A caller
    /// that wants a ring of its own still says so with [`focus_style`](Self::focus_style); this only
    /// supplies one when nothing else has.
    ///
    /// Deliberately not implied by [`on_press`](Self::on_press): a scrim, a click-away backdrop and a drag
    /// surface all take presses and none of them is a place the keyboard should stop.
    pub fn control(mut self, role: focus::Role) -> Self {
        let id = *self.focusable.id.get_or_insert_with(focus::next_id);
        focus::register_with_role(id, focus::FocusKind::Widget, self.node, role);
        self.focusable.activates = true;
        if self.state.focus.is_none() {
            self.state.focus = Some(Box::new(|_r| default_focus_ring()));
        }
        self.mark_interactive();
        self
    }

    /// Declares that this control carries a checked state, and how to read it.
    ///
    /// Only meaningful after [`control`](Self::control), and only for the roles that have one. Without it a
    /// reader announces "checkbox" and stops — and a default of "unticked" would be worse, since it would be
    /// confidently wrong for half of them.
    pub fn toggled(self, state: impl Fn() -> bool + 'static) -> Self {
        if let Some(id) = self.focusable.id {
            focus::set_toggled(id, state);
        }
        self
    }

    /// Declares the number this box carries, so a reader says where a slider stands and not only that it is
    /// one. The counterpart of [`toggled`](Self::toggled) for a control whose state is a value.
    pub fn valued(self, read: impl Fn() -> NumericValue + 'static) -> Self {
        if let Some(id) = self.focusable.id {
            focus::set_value(id, read);
        }
        self
    }

    pub fn focus_style(mut self, f: impl Fn(Rect) -> RectStyle + 'static) -> Self {
        // Declaring a ring is declaring the box focusable, or it would join no tab order and the ring would be a style nothing could satisfy.
        let id = *self.focusable.id.get_or_insert_with(focus::next_id);
        focus::register_at(id, focus::FocusKind::Widget, self.node);
        self.state.focus = Some(Box::new(f));
        self
    }

    /// Whether the box is currently pressed (a primary pointer is held down inside it). Set only when an
    /// `active_style` is present; drives its paint swap and clears on release/leave/drag-off.
    fn set_active(&self, active: bool) {
        if self.state.active.is_some() && self.state.is_active.get() != active {
            self.state.is_active.set(active);
        }
    }

    /// Drops everything that meant *the pointer is inside this box*: hover, the pressed look, and a tap
    /// still waiting for a release within the bounds. Deliberately not the drag — that one is measured from
    /// the press and does not care where the pointer has wandered to.
    fn end_containment(&mut self) {
        self.press.cancel();
        self.set_active(false);
        let tracks_hover = self.state.hover.is_some()
            || self.pointer.hover.is_some()
            || self.pointer.cursor.is_some();
        if tracks_hover && self.state.is_hovered.get() {
            self.state.is_hovered.set(false);
            // The shape was this box's claim about what a press would do here, and nothing else restores it while the pointer is still inside the window.
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
    /// The shape is the app's statement of what the next press will do — orbit, resize a panel, place a
    /// point — so it belongs to the widget that would handle that press, not to a mode the app tracks.
    pub fn cursor(mut self, cursor: Cursor) -> Self {
        self.pointer.cursor = Some(cursor);
        self
    }

    /// Declares that this box does not stand between the pointer and whatever it is drawn over — CSS's
    /// `pointer-events: none`, and the second consumer of the hook [`Overlay`] opened.
    ///
    /// A box covers what is behind it: since the hit-test walks in paint order, the topmost child under the
    /// pointer takes the event whether or not it wants it. That is right for a panel and wrong for a *label*
    /// — a readout floating over a canvas, a badge over a photo, a drag ghost — which is drawn on top
    /// precisely so it can be read, and whose whole contract is that the thing underneath still works. A
    /// modeller's transform readout sits across the top of the viewport it reports on; without this, moving
    /// the pointer under it stops the operation it is describing.
    ///
    /// It is a property of *this* box only. Children still hit-test normally, so a click-through bar can
    /// hold a real button — the same split CSS makes with `pointer-events: auto` on a child.
    pub fn click_through(mut self, through: bool) -> Self {
        self.click_through = through;
        self
    }

    /// A stroke that starts on this box is this box's, and goes no further out.
    ///
    /// **The other half of «the innermost drag owns the stroke».** A band that moves the window is dragged by
    /// its empty space, and the controls sitting in it are not empty space — but a button claims nothing, so
    /// its press was the band.s and the compositor took the pointer away to move the window before the button
    /// could answer. The same shape holds for a menu panel over a dismissing backdrop, and for any control
    /// inside anything draggable.
    ///
    /// Not `on_drag` with an empty body, which is what this replaces: that says «I drag, and do nothing», and
    /// what is meant is «this one is mine». It claims for every button, because a stroke is a stroke whichever
    /// one started it.
    pub fn holds_stroke(mut self) -> Self {
        self.holds_stroke = true;
        self
    }

    /// Give this widget ownership of an [`Effect`], so it runs for exactly as long as the widget exists.
    ///
    /// The reactive runtime scopes an effect to the *surface* it was registered on, which is the right span for
    /// a shell-wide subscription and far too coarse for one row of a list: the row goes, the effect stays, and
    /// it keeps firing at a node that is gone. Dropping the handle instead is the opposite failure — the effect
    /// deregisters, runs once, and stops, with nothing to say so. This is the third answer, and the one an
    /// effect that belongs to a widget wants.
    ///
    /// Says what the text below this box looks like — see
    /// [`Container::declaring`](crate::Container::declaring).
    pub fn declaring(self, declared: impl Fn() -> Declared + 'static) -> Self {
        let node = self.node;
        effect(move || crate::inherit::declare(node, declared()));
        self
    }

    /// Keeps the box's *layout* style in step with the reactive state it was built from — the theme's metric
    /// tokens, today. `style` runs now, and again whenever a signal it read changes; the node is restyled in
    /// place, so a live theme switch re-spaces the box as well as re-colouring it.
    ///
    /// Paint needs nothing like this: a rect or text style is a closure the renderer re-runs every frame, so a
    /// token read inside one is already live. A layout style is a *value*, handed to the layout tree once when
    /// the node is made — which is why the reactive read has to be arranged here rather than coming for free.
    ///
    /// Give [`new`](Self::new) the same builder, so the node starts at the style it will settle on:
    /// `StyledContainer::new(shell(), paint, kids)?.styled_by(shell)`.
    pub fn styled_by(self, style: impl Fn() -> LayoutStyle + 'static) -> Self {
        let node = self.node;
        style_follows(node, style);
        self
    }

    /// Make the box itself pressable. The callback fires on a tap (release, not press) inside the box;
    /// a child widget that handles the press wins, and a scroll gesture started on the box does not fire it.
    pub fn on_press(self, f: impl Fn() + 'static) -> Self {
        self.maybe_on_press(Some(f))
    }

    /// [`on_press`](Self::on_press) for a handler the caller may not have supplied.
    ///
    /// What a wrapper component needs to forward an optional callback. A box whose press handler is a no-op
    /// still reports the tap `Handled`, so "no handler" would become "swallows the click" — a display-only chip
    /// eating a press instead of letting it through. `None` leaves the box exactly as it was; the `maybe_*`
    /// pairs below say the same for every other event whose absence the box can observe.
    pub fn maybe_on_press(mut self, f: Option<impl Fn() + 'static>) -> Self {
        let Some(f) = f else { return self };
        self.press.set(f);
        self.mark_interactive();
        self
    }

    /// Fire `f(button)` on a tap with a **non-primary** button — `Secondary` (right) or `Auxiliary` (middle).
    /// Same tap-on-release semantics as [`Self::on_press`]: a child that handles the press wins, and travel
    /// past the tap slop cancels it.
    ///
    /// Opt-in per box rather than folded into `on_press`, because a non-primary press otherwise falls through
    /// to whatever is behind it — silently swallowing right-clicks on every pressable box would break that.
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

    /// Fires once a press inside the box is held past ~500ms without moving past the tap slop, instead of
    /// `on_press`'s tap-on-release. There is no dedicated timer in the gesture pipeline, so the threshold is
    /// only checked on the next pointer event after the press (a move or the release) — it fires slightly
    /// late, never at exactly 500ms, and a release before that next check-in is a normal tap.
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

    /// Make the box draggable. The callback fires with the pointer position (layout space) on a press
    /// inside the box and on every move until release — even after the pointer leaves the box. Map the
    /// coordinate to a value (slider) or an offset (reorder/resize).
    /// Fires once when a drag started on this box ends, with the position it finished at (layout space, local
    /// to the box, same as [`on_drag`](Self::on_drag)).
    ///
    /// This is what makes a *threshold* gesture expressible: `on_drag` alone reports where the pointer is but
    /// never that it let go, so a swipe-to-dismiss or a drag-to-open can be tracked and never decided. A drag
    /// also ends when the pointer leaves the window or a child consumes the release; those carry no position,
    /// so the last one the drag reached is reported instead — the gesture always ends exactly once.
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
    /// A slider or a splitter wants exactly one button and gets it by default. A surface with more than one
    /// thing to drag needs the others: a modeller orbits with the primary button and pans with the secondary,
    /// which is what the OS and every 3D application call those gestures. The handler is the same one — read
    /// [`crate::pointer_buttons`] inside it to tell which button is doing the dragging.
    pub fn drag_button(mut self, button: platform_core::PointerButton) -> Self {
        self.drag.arm_with(&button);
        self
    }

    /// How far the pointer must travel before this box counts as being dragged.
    ///
    /// Without it a press *is* a drag from its first instant, which is right for a slider — pressing the track
    /// is how you set the value — and wrong for anything where a click and a drag mean different things on the
    /// same button. A viewport is the case: a click picks what is under it, a drag orbits, and telling them
    /// apart is the difference between selecting something and nudging the camera by a pixel.
    ///
    /// Set it and the two stop overlapping: a stroke that never travels this far fires only
    /// [`on_press`](Self::on_press), one that does fires only the drag handlers, and neither fires both.
    pub fn drag_threshold(mut self, px: f32) -> Self {
        self.drag.set_threshold(px);
        self
    }

    /// Holds the drag to one axis: the other coordinate is reported as it stood when the press landed.
    ///
    /// What a gesture with one meaning owes its reader. A strip reordered along its own axis, a slider, a
    /// splitter — each takes one number and throws the other away, and the ones that forget let what they are
    /// dragging wander off the line it lives on.
    pub fn drag_axis(mut self, axis: crate::drag::DragAxis) -> Self {
        self.drag.lock_to(axis);
        self
    }

    /// Keeps the reported point inside `bounds`, in this box's own coordinates.
    ///
    /// A drag goes on receiving moves after the pointer has left the widget — that is what keeps a slider
    /// tracking when the hand overshoots — and the same broadcast is what lets a pointer dragged out of the
    /// window report a place no layout could produce. This is where a caller says how far out the answer may
    /// go: once, rather than at every use. Read on each report, so a box that resizes takes its bounds with it.
    pub fn drag_within(mut self, bounds: impl Fn() -> Rect + 'static) -> Self {
        self.drag.keep_within(bounds);
        self
    }

    /// Records this box as a pointer target in the per-surface interactive registry, so a surface that carves
    /// its input region from its content (a click-through overlay) receives input over it. See
    /// [`crate::interactive_rects`].
    fn mark_interactive(&self) {
        crate::input_region::register_interactive(self.node, self.rect.read_only());
    }

    /// Fire `f(true)` when the mouse enters the box and `f(false)` when it leaves (mouse only). Independent
    /// of `hover_style`: a box can observe hover without swapping its paint.
    ///
    /// Registers the box as a pointer target, like [`on_scroll`](Self::on_scroll) does for the same reason: a
    /// surface that carves its input region from its content (a click-through overlay) never receives a move
    /// event over a box it left out of that region, so a hover it did not register is a hover it can't observe.
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

    /// Fire `f(x, y)` with the pointer position — local to the box, as [`on_drag`](Self::on_drag) reports it —
    /// on every move over it.
    ///
    /// The continuous half of [`on_hover`](Self::on_hover), which reports only the crossings. It is what a
    /// surface that answers to *where* the pointer is needs: highlighting the face under the cursor, previewing
    /// a snap, stretching a dimension line. Fires for touch as well as mouse, since a drag on a touchscreen
    /// asks the same question.
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

    /// Fire `f(dx, dy)` with the wheel delta while the pointer is over the box — scroll-to-adjust on a control
    /// (a volume or brightness chip, a stepper), or zoom on a viewport. Deltas are normalised to pixels,
    /// matching [`ScrollArea`](crate::ScrollArea): a line delta counts as 20px, so one wheel notch is roughly
    /// ±60.
    ///
    /// Targeted by hit-testing the wheel's own position, so it answers a wheel that arrives before the pointer
    /// has moved at all. A scrollable child (a scroll area inside the box) gets first refusal and keeps it.
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

    /// Fire `f(&key)` on every key press. This is a GLOBAL handler (key events reach every widget; there is
    /// no per-widget focus), so it suits app-level shortcuts, not focused text entry.
    ///
    /// It stands aside while a text entry holds focus and the press is text it would take
    /// ([`focus::text_entry_takes_key`]) — so a shortcut on `3` does not also fire when the user types `3`
    /// into a field, while `⌘S` still reaches it. Read the modifiers with [`crate::modifiers`]: key events
    /// carry them, but pointer events do not, so the state registry is the one answer that works everywhere.
    pub fn on_key(self, f: impl Fn(&Key) + 'static) -> Self {
        self.maybe_on_key(Some(f))
    }

    /// [`on_key`](Self::on_key) for a handler the caller may not have supplied.
    pub fn maybe_on_key(mut self, f: Option<impl Fn(&Key) + 'static>) -> Self {
        let Some(f) = f else { return self };
        self.on_key = Some(Box::new(f));
        self
    }

    /// Make the box focusable and fire `f(true)`/`f(false)` when it gains/loses keyboard focus. It joins
    /// the tab order (Tab/Shift-Tab reach it) and takes focus on tap. Use it to drive a focus ring or to
    /// build a custom focusable widget on top of a `box`.
    pub fn on_focus(self, f: impl Fn(bool) + 'static) -> Self {
        self.maybe_on_focus(Some(f))
    }

    /// [`on_focus`](Self::on_focus) for a handler the caller may not have supplied.
    pub fn maybe_on_focus(mut self, f: Option<impl Fn(bool) + 'static>) -> Self {
        let Some(f) = f else { return self };
        let id = *self.focusable.id.get_or_insert_with(focus::next_id);
        focus::register_at(id, focus::FocusKind::Widget, self.node);
        // An effect fires the callback only on an actual transition (its first run seeds `last`, no fire).
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
        // Disabled wins over pressed wins over hover wins over base. Each state is only read when its style exists, so a plain box's view() stays inert and subscribes to none of them.
        let style = if let Some(disabled) = &self.state.disabled
            && self.is_disabled()
        {
            disabled
        } else if let Some(active) = &self.state.active
            && self.state.is_active.get()
        {
            active
        } else if let Some(hover) = &self.state.hover
            && self.state.is_hovered.get()
        {
            hover
        } else {
            &self.style
        };
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
        match self.transform.as_ref().and_then(|t| t(r)) {
            Some(matrix) => RenderNode::transform_with(matrix, [composed]),
            None => composed,
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // `disabled` on a region means the region, as a `fieldset` does — hence ahead of the pure-routing bail below, which a wrapper with no handlers of its own would otherwise take.
        // The state it was showing goes with it, or a box disabled mid-hover keeps the highlight and the hand cursor it can no longer honour.
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
            // Moves are broadcast to all children (their hover) and also feed our own scroll-vs-tap and
            // hover tracking. Hover is mouse-only: touch has no "pointer left", so a tap would otherwise
            // leave the box stuck in its hover style.
            Event::PointerMoved { x, y, source } => {
                self.press.track_move(event);
                let dragged = self.drag.moved(event, rect) == EventResult::Handled;
                // A stroke that has cleared its drag threshold has committed to being a drag, so it is no
                // longer a tap. Only for a box that set one: without a threshold the two have always both
                // fired, and a slider that also takes a press is entitled to keep that.
                if self.drag.has_threshold() && self.drag.has_started() {
                    self.press.cancel();
                }
                let child = self.dispatch_children(event);
                // Inside the box AND nothing drawn in front of it there: a move is broadcast for the sake of
                // gestures already running, and only the topmost box under the pointer is *hovered*.
                let inside =
                    rect.contains(*x as f32, *y as f32) && !crate::pointer::pointer_occluded();
                // Pressed clears once the pointer drags off the box (mouse or touch) so it never sticks.
                if !inside {
                    self.set_active(false);
                }
                if inside && let Some(cb) = &self.pointer.moved {
                    cb(*x as f32 - rect.x, *y as f32 - rect.y);
                }
                let tracks_hover = self.state.hover.is_some()
                    || self.pointer.hover.is_some()
                    || self.pointer.cursor.is_some();
                if tracks_hover
                    && matches!(source, PointerSource::Mouse)
                    && inside != self.state.is_hovered.get()
                {
                    self.state.is_hovered.set(inside);
                    if let Some(cursor) = self.pointer.cursor {
                        platform_core::push_window_command(WindowCommand::SetCursor(if inside {
                            cursor
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
            // A child (e.g. an inner button) hit-tests first and wins; only a press on the bare box arms our tap/drag.
            Event::PointerPressed { x, y, button, .. } => {
                // Pressed state and focus are primary-only. A box that asked for other buttons gets them
                // routed to its press or drag gesture; every other box lets them fall through untouched.
                let primary = *button == PointerButton::Primary;
                if !primary && !self.press.wants_alt() && !self.drag.arms(button) {
                    return self.dispatch_children(event);
                }
                // **A child takes the tap; the innermost drag takes the stroke.** Standing this box's drag down because a child took the press made a strip draggable only where it had nothing pressable in it — tabs that can be clicked could never be dragged. Arming it regardless is the other half of the same mistake: the tab reorders and the band it sits in moves the window, on one press. So the children are asked whether one of them claimed the stroke.
                let (below, claimed) = crate::drag::claimed(|| self.dispatch_children(event));
                // Said once the children have had the press and before this returns, so it reaches whatever contains this box: a stroke that starts here is this box's, and nothing further out takes it.
                if self.holds_stroke && rect.contains(*x as f32, *y as f32) {
                    crate::drag::claim();
                }
                if below == EventResult::Handled {
                    self.press.cancel();
                    if self.drag.is_set() && !claimed {
                        self.drag.press(event, rect);
                    }
                    return EventResult::Handled;
                }
                // A primary press inside the box enters the pressed state (purely visual; independent of on_press).
                if primary && rect.contains(*x as f32, *y as f32) {
                    self.set_active(true);
                }
                // A tap inside a focusable box takes focus (and consumes the press so focus sticks).
                let focused = match self.focusable.id {
                    Some(id) if primary && rect.contains(*x as f32, *y as f32) => {
                        focus::request_from_pointer(id);
                        true
                    }
                    _ => false,
                };
                let tapped =
                    self.press.is_set() && self.press.arm(event, rect) == EventResult::Handled;
                let dragged = !claimed
                    && self.drag.is_set()
                    && self.drag.press(event, rect) == EventResult::Handled;
                if tapped || dragged || focused {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::PointerReleased { button, .. } => {
                let primary = *button == PointerButton::Primary;
                if !primary && !self.press.wants_alt() && !self.drag.arms(button) {
                    return self.dispatch_children(event);
                }
                // A release always ends the pressed state, wherever it lands.
                if primary {
                    self.set_active(false);
                }
                if self.dispatch_children(event) == EventResult::Handled {
                    self.press.cancel();
                    self.drag.end(None);
                    return EventResult::Handled;
                }
                // The release carries its own position, which is the one the gesture actually finished at —
                // a drag can end past the last move the compositor delivered.
                let released_at = match event {
                    Event::PointerReleased { x, y, .. } => {
                        Some((*x as f32 - rect.x, *y as f32 - rect.y))
                    }
                    _ => None,
                };
                let dragged = self.drag.arms(button) && self.drag.end(released_at);
                let tapped =
                    self.press.is_set() && self.press.release(event, rect) == EventResult::Handled;
                if tapped || dragged {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            // Crossing the window border says nothing about a gesture in flight: a drag is measured from the press, and ending it here cuts an orbit short at the edge of a full-window viewport. What leaving does invalidate is containment.
            Event::CursorLeft => {
                self.end_containment();
                self.dispatch_children(event)
            }
            // Where a live gesture really does have to end: a window that loses focus never sends the release for what was held, and Alt-Tab never crosses the border.
            Event::FocusChanged { is_focused: false } => {
                self.end_containment();
                self.drag.end(None);
                self.dispatch_children(event)
            }
            // Children (e.g. a nested scroll area) get first refusal; only then does an `on_scroll` box under the wheel consume it.
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
            // Broadcast (no pointer position): fire the global key handler, then keep routing to children.
            Event::KeyPressed { key, modifiers } => {
                // While this focusable box holds focus, Tab moves focus to the next/previous field.
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
                // The keyboard half of a tap. Consumed only when there was something to fire, so Space on a
                // focused box with no press handler still reaches whatever else wanted it — and skipped
                // entirely for a box that handles its own keys, which is not second-guessed: a dropdown
                // trigger answers Enter by confirming a highlighted row, not by re-opening itself.
                if self.focusable.activates
                    && self.on_key.is_none()
                    && let Some(id) = self.focusable.id
                    && focus::is_focused(id)
                    && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Space))
                    && self.press.activate()
                {
                    return EventResult::Handled;
                }
                // Not while a field has the caret: the press is the user typing, and this handler is the app's
                // shortcut table, which would otherwise fire on every letter of what they type.
                if let Some(cb) = &self.on_key
                    && !focus::text_entry_takes_key(key, *modifiers)
                {
                    cb(key);
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
        // Drop the focus watcher first so releasing focus below doesn't fire `on_focus` during teardown.
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
/// A ring and not a fill, because it answers a different question from hover or pressed — *where the keys are
/// going*, not what the box is doing — and has to survive being layered over whichever of those won. Its
/// radius is deliberately absent: the compositing path takes that from the box, since a ring sits on a shape
/// it does not get to reshape.
fn default_focus_ring() -> RectStyle {
    RectStyle::default().with_border(Border::uniform(use_theme_tokens().primary(), 2.0))
}

/// Builds the affine matrix for a box's declarative `rotate`/`scale`/`translate` attributes, pivoting
/// rotation and scale on the box centre. Returns `None` when every component is identity, so an untransformed
/// box skips the extra transform node entirely.
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
mod tests {
    use crate::context::reset_layout_runtime;
    use std::cell::Cell;
    use std::rc::Rc;

    use layout_core::AvailableSpace;
    use platform_core::{PointerButton, PointerSource};
    use renderer_core::{Color, ShapeStyle};
    use theme_core::{ThemeTokens, set_theme, use_theme};

    use super::*;
    use platform_core::ScrollDelta;

    #[test]
    fn box_transform_identity_is_none() {
        let r = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        assert!(box_transform(r, 0.0, 1.0, 1.0, 0.0, 0.0).is_none());
    }

    #[test]
    fn box_transform_scale_pivots_on_center() {
        let r = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        // scale_around(2, 2, 50, 50): pins the centre, so e = f = 50 - 2*50 = -50.
        assert_eq!(
            box_transform(r, 0.0, 2.0, 2.0, 0.0, 0.0).unwrap(),
            [2.0, 0.0, 0.0, 2.0, -50.0, -50.0]
        );
    }

    #[test]
    fn box_transform_translate_offsets_origin() {
        let r = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        assert_eq!(
            box_transform(r, 0.0, 1.0, 1.0, 8.0, -4.0).unwrap(),
            [1.0, 0.0, 0.0, 1.0, 8.0, -4.0]
        );
    }
    use crate::container::Container;
    use crate::context::{compute_layout, track_layout};

    /// The three halves a control is made of, asserted together because that is the whole reason they are one
    /// call: Tab reaches it, Enter fires it, and it says what it is. Nine catalogue components had none of the
    /// three while compiling and looking correct, which is what a split API buys you.
    #[test]
    fn a_control_joins_the_tab_order_answers_enter_and_says_what_it_is() {
        use std::cell::Cell;

        reset_layout_runtime();
        focus::clear();
        let fired: Rc<Cell<u32>> = Rc::new(Cell::new(0));
        let sink = fired.clone();
        let mut card = StyledContainer::new(
            LayoutStyle::new().width(80.0).height(30.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .control(focus::Role::CheckBox)
        .on_press(move || sink.set(sink.get() + 1));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(80.0),
            AvailableSpace::Definite(30.0),
        )
        .unwrap();

        focus::focus_next();
        assert!(focus::current().is_some(), "Tab reaches it");

        let key = |named| Event::KeyPressed {
            key: Key::Named(named),
            modifiers: platform_core::ModifiersState::default(),
        };
        assert_eq!(card.on_event(&key(NamedKey::Enter)), EventResult::Handled);
        assert_eq!(card.on_event(&key(NamedKey::Space)), EventResult::Handled);
        assert_eq!(fired.get(), 2, "Enter and Space each fire the press");

        let exposed = focus::exposed();
        assert_eq!(exposed.len(), 1);
        assert_eq!(exposed[0].role, focus::Role::CheckBox);
        assert!(exposed[0].enabled);
    }

    /// And what it must *not* do. A scrim, a click-away backdrop and a drag surface all take presses, and none
    /// of them is a place the keyboard should stop — so a press on its own still buys nothing.
    #[test]
    fn a_press_handler_alone_is_not_a_control() {
        reset_layout_runtime();
        focus::clear();
        let mut card = StyledContainer::new(
            LayoutStyle::new().width(80.0).height(30.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(|| {});
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(80.0),
            AvailableSpace::Definite(30.0),
        )
        .unwrap();

        focus::focus_next();
        assert!(focus::exposed().is_empty(), "it is not a tab stop");
        assert_eq!(
            card.on_event(&Event::KeyPressed {
                key: Key::Named(NamedKey::Enter),
                modifiers: platform_core::ModifiersState::default(),
            }),
            EventResult::Ignored,
            "and Enter is left for whoever else wanted it"
        );
    }

    fn press(x: f64, y: f64, source: PointerSource) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source,
        }
    }
    fn release(x: f64, y: f64, source: PointerSource) -> Event {
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source,
        }
    }

    /// **A strip is draggable even where the thing under the pointer is pressable.** A row of tabs that can be
    /// clicked *and* dragged into another order is the ordinary shape of a reorderable list, and the press was
    /// standing the parent's drag down the instant a child took it — so the threshold was never reached and
    /// the strip could only be dragged by its gaps.
    ///
    /// What the child takes is the tap. The stroke is still the box's, and the child's own tap is cancelled by
    /// its slop once the stroke has committed to being a drag.
    #[test]
    fn a_child_that_takes_the_press_does_not_take_the_drag_with_it() {
        reset_layout_runtime();
        let dragged: Rc<Cell<u32>> = Rc::new(Cell::new(0));
        let sink = dragged.clone();
        let tab = StyledContainer::new(
            LayoutStyle::new().width(100.0).height(30.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(|| {});
        let mut strip = StyledContainer::new(
            LayoutStyle::new().flex_row().width(100.0).height(30.0),
            |_r| RectStyle::default(),
            vec![Box::new(tab)],
        )
        .unwrap()
        .drag_threshold(4.0)
        .on_drag(move |_x, _y| sink.set(sink.get() + 1));
        compute_layout(
            strip.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(30.0),
        )
        .unwrap();

        strip.on_event(&press(10.0, 15.0, PointerSource::Mouse));
        strip.on_event(&Event::PointerMoved {
            x: 60.0,
            y: 15.0,
            source: PointerSource::Mouse,
        });

        assert!(
            dragged.get() > 0,
            "la pulsación del hijo se llevó el arrastre del padre"
        );
    }

    /// **And a box that holds the stroke stops it without pretending to drag.** The rule needs a way to be
    /// said by something that is not draggable at all — a button in a band that moves the window, a panel over
    /// a backdrop that dismisses — and saying it with an `on_drag` that does nothing is a lie about what the
    /// widget is.
    #[test]
    fn a_box_that_holds_the_stroke_keeps_it_from_whatever_contains_it() {
        reset_layout_runtime();
        let moved: Rc<Cell<u32>> = Rc::new(Cell::new(0));
        let sink = moved.clone();
        let control = StyledContainer::new(
            LayoutStyle::new().width(60.0).height(30.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(|| {})
        .holds_stroke();
        let mut band = StyledContainer::new(
            LayoutStyle::new().flex_row().width(200.0).height(30.0),
            |_r| RectStyle::default(),
            vec![Box::new(control)],
        )
        .unwrap()
        .on_drag(move |_x, _y| sink.set(sink.get() + 1));
        compute_layout(
            band.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(30.0),
        )
        .unwrap();

        band.on_event(&press(30.0, 15.0, PointerSource::Mouse));
        band.on_event(&Event::PointerMoved {
            x: 80.0,
            y: 15.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(moved.get(), 0, "la banda se llevó el trazo del control");

        band.on_event(&release(80.0, 15.0, PointerSource::Mouse));
        band.on_event(&press(150.0, 15.0, PointerSource::Mouse));
        assert!(moved.get() > 0, "y donde no hay control sigue siendo suya");
    }

    /// **And the innermost drag owns the stroke.** The other half of the rule above: a tab that reorders sits
    /// in a band that moves the window, so a press that armed both ran two gestures at once — the tab went
    /// nowhere because the window went with it.
    #[test]
    fn a_drag_inside_another_one_is_the_only_one_that_runs() {
        reset_layout_runtime();
        let (inner, outer) = (Rc::new(Cell::new(0u32)), Rc::new(Cell::new(0u32)));
        let (near, far) = (inner.clone(), outer.clone());
        let tab = StyledContainer::new(
            LayoutStyle::new().width(60.0).height(30.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .drag_threshold(4.0)
        .on_drag(move |_x, _y| near.set(near.get() + 1));
        let mut band = StyledContainer::new(
            LayoutStyle::new().flex_row().width(200.0).height(30.0),
            |_r| RectStyle::default(),
            vec![Box::new(tab)],
        )
        .unwrap()
        .drag_threshold(4.0)
        .on_drag(move |_x, _y| far.set(far.get() + 1));
        compute_layout(
            band.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(30.0),
        )
        .unwrap();

        // On the tab: the tab reorders and the band stays put.
        band.on_event(&press(30.0, 15.0, PointerSource::Mouse));
        band.on_event(&Event::PointerMoved {
            x: 80.0,
            y: 15.0,
            source: PointerSource::Mouse,
        });
        assert!(inner.get() > 0, "la pestaña no se arrastró");
        assert_eq!(outer.get(), 0, "y la banda se fue con ella");

        // Past the tabs, where the band is the only thing there: the band is what moves.
        band.on_event(&release(80.0, 15.0, PointerSource::Mouse));
        band.on_event(&press(150.0, 15.0, PointerSource::Mouse));
        band.on_event(&Event::PointerMoved {
            x: 190.0,
            y: 15.0,
            source: PointerSource::Mouse,
        });
        assert!(outer.get() > 0, "la banda ya no se puede arrastrar");
    }

    #[test]
    fn on_hover_fires_on_enter_and_leave() {
        let seen: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        reset_layout_runtime();
        let inner = Container::new(LayoutStyle::new().width(100.0).height(100.0), vec![]).unwrap();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![Box::new(inner)],
        )
        .unwrap()
        .on_hover(move |h| sink.set(Some(h)));
        let node = card.layout_node();
        compute_layout(
            node,
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        card.on_event(&Event::PointerMoved {
            x: 50.0,
            y: 50.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(seen.get(), Some(true), "entering fires on_hover(true)");
        card.on_event(&Event::CursorLeft);
        assert_eq!(seen.get(), Some(false), "leaving fires on_hover(false)");
    }

    /// The wheel is targeted by where it happened, not by a hover the box had to have seen first — so the
    /// very first wheel over a box lands, and one over a sibling never does.
    #[test]
    fn on_scroll_targets_by_position_and_normalises_lines() {
        let seen: Rc<Cell<(f32, f32)>> = Rc::new(Cell::new((0.0, 0.0)));
        let sink = seen.clone();
        reset_layout_runtime();
        let inner = Container::new(LayoutStyle::new().width(100.0).height(100.0), vec![]).unwrap();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![Box::new(inner)],
        )
        .unwrap()
        .on_scroll(move |dx, dy| sink.set((dx, dy)));
        let node = card.layout_node();
        compute_layout(
            node,
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        assert_eq!(
            card.on_event(&Event::Scrolled {
                delta: ScrollDelta::Pixels { x: 0.0, y: -30.0 },
                x: 300.0,
                y: 300.0,
            }),
            EventResult::Ignored,
            "a wheel event outside the box is ignored"
        );
        assert_eq!(seen.get(), (0.0, 0.0));

        assert_eq!(
            card.on_event(&Event::Scrolled {
                delta: ScrollDelta::Pixels { x: 0.0, y: -30.0 },
                x: 50.0,
                y: 50.0,
            }),
            EventResult::Handled,
            "a wheel over the box is ours, with no move having preceded it"
        );
        assert_eq!(seen.get(), (0.0, -30.0));

        // Line deltas are normalised to pixels the same way ScrollArea does it.
        card.on_event(&Event::Scrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: 3.0 },
            x: 50.0,
            y: 50.0,
        });
        assert_eq!(seen.get(), (0.0, 60.0));
    }

    #[test]
    fn on_key_fires_on_key_press() {
        let count = Rc::new(Cell::new(0u32));
        let sink = count.clone();
        reset_layout_runtime();
        let inner = Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column(),
            |_r| RectStyle::default(),
            vec![Box::new(inner)],
        )
        .unwrap()
        .on_key(move |_k| sink.set(sink.get() + 1));
        card.on_event(&Event::KeyPressed {
            key: Key::Char('a'),
            modifiers: platform_core::ModifiersState::default(),
        });
        assert_eq!(count.get(), 1, "a key press fires on_key");
    }

    /// The bug this closes: an app-level shortcut table sharing letters with what the user types. `3` selects
    /// a mode until a field has the caret, and `⌘S` saves either way because no editor here wants it.
    #[test]
    fn a_global_key_handler_stands_aside_while_a_field_has_the_caret() {
        let count = Rc::new(Cell::new(0u32));
        let sink = count.clone();
        reset_layout_runtime();
        focus::clear();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column(),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_key(move |_k| sink.set(sink.get() + 1));
        let press = |key, modifiers| Event::KeyPressed { key, modifiers };
        let plain = platform_core::ModifiersState::default();
        let meta = platform_core::ModifiersState {
            is_meta: true,
            ..Default::default()
        };

        card.on_event(&press(Key::Char('3'), plain));
        assert_eq!(count.get(), 1, "with nothing focused the shortcut fires");

        let field = focus::next_id();
        focus::register_as(field, focus::FocusKind::TextEntry);
        focus::request(field);
        card.on_event(&press(Key::Char('3'), plain));
        assert_eq!(count.get(), 1, "typing into a field is not a shortcut");
        card.on_event(&press(Key::Char('s'), meta));
        assert_eq!(count.get(), 2, "a chord is a command, not text");
        card.on_event(&press(Key::Named(NamedKey::F5), plain));
        assert_eq!(count.get(), 3, "no editor takes F5");

        focus::unregister(field);
        card.on_event(&press(Key::Char('3'), plain));
        assert_eq!(count.get(), 4, "the caret left, the shortcut is back");
    }

    /// A focusable that is not a text entry — a button, a tab — leaves the shortcut table alone.
    #[test]
    fn a_focused_button_does_not_swallow_shortcuts() {
        let count = Rc::new(Cell::new(0u32));
        let sink = count.clone();
        reset_layout_runtime();
        focus::clear();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column(),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_key(move |_k| sink.set(sink.get() + 1));
        let button = focus::next_id();
        focus::register_as(button, focus::FocusKind::Widget);
        focus::request(button);
        card.on_event(&Event::KeyPressed {
            key: Key::Char('3'),
            modifiers: platform_core::ModifiersState::default(),
        });
        assert_eq!(count.get(), 1);
        focus::unregister(button);
    }

    /// A box drags from the primary button and no other, until it says otherwise — a slider must not slide
    /// on a right-click. A surface with more than one thing to drag opts the others in, and tells them apart
    /// through the button registry rather than through a wider callback.
    #[test]
    fn a_drag_starts_only_from_the_buttons_the_box_asked_for() {
        let seen = Rc::new(Cell::new(0u32));
        let sink = seen.clone();
        reset_layout_runtime();
        let mut plain = StyledContainer::new(
            LayoutStyle::new().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_drag(move |_x, _y| sink.set(sink.get() + 1));
        compute_layout(
            plain.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let press = |button: PointerButton| Event::PointerPressed {
            x: 50.0,
            y: 50.0,
            button,
            source: PointerSource::Mouse,
        };
        plain.on_event(&press(PointerButton::Secondary));
        assert_eq!(seen.get(), 0, "a secondary press is not this box's drag");
        plain.on_event(&press(PointerButton::Primary));
        assert_eq!(seen.get(), 1, "the primary one always is");

        let count = Rc::new(Cell::new(0u32));
        let sink = count.clone();
        reset_layout_runtime();
        let mut viewport = StyledContainer::new(
            LayoutStyle::new().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_drag(move |_x, _y| sink.set(sink.get() + 1))
        .drag_button(PointerButton::Secondary);
        compute_layout(
            viewport.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        assert_eq!(
            viewport.on_event(&press(PointerButton::Secondary)),
            EventResult::Handled
        );
        assert_eq!(count.get(), 1, "the box asked for this button");
        // And the drag it started ends on that button's release, or it would stay armed for ever.
        assert_eq!(
            viewport.on_event(&Event::PointerReleased {
                x: 60.0,
                y: 60.0,
                button: PointerButton::Secondary,
                source: PointerSource::Mouse,
            }),
            EventResult::Handled
        );
    }

    /// The registry that tells the buttons apart, since the drag callback reports where the pointer is and
    /// not what pressed it.
    #[test]
    fn the_button_registry_holds_what_is_down() {
        crate::reset_pointer();
        assert!(!crate::pointer_buttons().any());
        crate::observe_pointer(&Event::PointerPressed {
            x: 0.0,
            y: 0.0,
            button: PointerButton::Secondary,
            source: PointerSource::Mouse,
        });
        assert!(crate::pointer_buttons().secondary);
        assert!(!crate::pointer_buttons().primary);
        // Losing the focus is where a reconstructed state goes wrong: the release never comes.
        crate::observe_pointer(&Event::FocusChanged { is_focused: false });
        assert!(!crate::pointer_buttons().any());
    }

    #[test]
    fn on_pointer_move_reports_the_position_local_to_the_box() {
        let seen: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        reset_layout_runtime();
        // A 20px spacer above the box, so its own origin is not the window's and a raw position would show.
        let spacer = Container::new(LayoutStyle::new().width(100.0).height(20.0), vec![]).unwrap();
        let card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_pointer_move(move |x, y| sink.set(Some((x, y))));
        let mut root = Container::new(
            LayoutStyle::new().flex_column().width(100.0).height(120.0),
            vec![Box::new(spacer), Box::new(card)],
        )
        .unwrap();
        compute_layout(
            root.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(120.0),
        )
        .unwrap();

        root.on_event(&Event::PointerMoved {
            x: 30.0,
            y: 50.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(
            seen.get(),
            Some((30.0, 30.0)),
            "the box starts 20px down, so the y arrives 20 less — as on_drag reports it"
        );

        seen.set(None);
        root.on_event(&Event::PointerMoved {
            x: 300.0,
            y: 300.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(seen.get(), None, "a move outside the box is not its move");
    }

    #[derive(Clone)]
    struct TestTheme(Color);
    impl ThemeTokens for TestTheme {
        fn primary(&self) -> Color {
            self.0
        }
        fn on_primary(&self) -> Color {
            Color::WHITE
        }
    }

    // Clicking a theme button (which sets the global THEME) while a themed StyledContainer ancestor is on the dispatch stack must not re-enter that ancestor's render segment mid borrow_mut.
    #[test]
    fn a_theme_button_click_that_repaints_the_tree_does_not_panic() {
        set_theme(TestTheme(Color::RED));

        reset_layout_runtime();
        // A pressable primitive stands in for the old high-level Button (now in ui-components).
        let btn = StyledContainer::new(
            LayoutStyle::new().width(50.0).height(30.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || set_theme(TestTheme(Color::GREEN)));
        let btn_node = btn.layout_node();
        let inner = Container::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            vec![Box::new(btn)],
        )
        .unwrap();
        let card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(use_theme::<TestTheme>().0),
            vec![Box::new(inner)],
        )
        .unwrap();
        let card_node = card.layout_node();
        compute_layout(
            card_node,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        let br = track_layout(btn_node).unwrap().get();

        let mut tree = crate::ComponentList::new(card);
        let _ = tree.commands();

        reactive_core::begin_batch();
        let handled = tree.on_event(&Event::PointerPressed {
            x: (br.x + br.width / 2.0) as f64,
            y: (br.y + br.height / 2.0) as f64,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        if handled == EventResult::Handled {
            reactive_core::end_batch();
            reactive_core::begin_batch();
        }
        let _ = tree.commands();
        reactive_core::end_batch();
    }

    // A pressable box fires its callback on release (a tap), never on press alone.
    #[test]
    fn on_press_fires_on_tap_not_press() {
        let flag = Rc::new(Cell::new(false));
        let f = flag.clone();
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || f.set(true));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        assert_eq!(
            card.on_event(&press(100.0, 50.0, PointerSource::Mouse)),
            EventResult::Handled
        );
        assert!(!flag.get(), "press alone must not fire on_press");
        assert_eq!(
            card.on_event(&release(100.0, 50.0, PointerSource::Mouse)),
            EventResult::Handled
        );
        assert!(flag.get(), "release inside the box fires on_press");
    }

    // Holding a press past the long-press threshold fires on_long_press on the next pointer event (there is
    // no dedicated timer) and suppresses the tap; a quick release stays a normal tap and never fires on_long_press.
    #[test]
    fn on_long_press_fires_after_threshold_not_on_quick_release() {
        let long_flag = Rc::new(Cell::new(false));
        let tap_flag = Rc::new(Cell::new(false));
        let lf = long_flag.clone();
        let tf = tap_flag.clone();
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || tf.set(true))
        .on_long_press(move || lf.set(true));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        // A quick release (well under the threshold) stays a normal tap.
        card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
        card.on_event(&release(100.0, 50.0, PointerSource::Mouse));
        assert!(tap_flag.get(), "a quick release still fires on_press");
        assert!(
            !long_flag.get(),
            "a quick release must not fire on_long_press"
        );

        // Holding past the threshold: the next event (the release here) fires on_long_press instead of on_press.
        tap_flag.set(false);
        card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
        std::thread::sleep(std::time::Duration::from_millis(550));
        card.on_event(&release(100.0, 50.0, PointerSource::Mouse));
        assert!(
            long_flag.get(),
            "a release after the threshold fires on_long_press"
        );
        assert!(!tap_flag.get(), "a long press must not also fire on_press");
    }

    fn press_with(x: f64, y: f64, button: PointerButton) -> Event {
        Event::PointerPressed {
            x,
            y,
            button,
            source: PointerSource::Mouse,
        }
    }
    fn release_with(x: f64, y: f64, button: PointerButton) -> Event {
        Event::PointerReleased {
            x,
            y,
            button,
            source: PointerSource::Mouse,
        }
    }

    fn laid_out_box() -> StyledContainer {
        reset_layout_runtime();
        StyledContainer::new(
            LayoutStyle::new().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
    }

    fn settle(card: &mut StyledContainer) {
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
    }

    #[test]
    fn on_alt_press_reports_which_non_primary_button_tapped() {
        let seen: Rc<Cell<Option<PointerButton>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let mut card = laid_out_box().on_alt_press(move |b| sink.set(Some(b)));
        settle(&mut card);

        card.on_event(&press_with(50.0, 50.0, PointerButton::Secondary));
        card.on_event(&release_with(50.0, 50.0, PointerButton::Secondary));
        assert_eq!(seen.take(), Some(PointerButton::Secondary));

        card.on_event(&press_with(50.0, 50.0, PointerButton::Auxiliary));
        card.on_event(&release_with(50.0, 50.0, PointerButton::Auxiliary));
        assert_eq!(seen.take(), Some(PointerButton::Auxiliary));
    }

    #[test]
    fn a_box_wanting_only_alt_presses_leaves_the_primary_one_alone() {
        let alt = Rc::new(Cell::new(false));
        let sink = alt.clone();
        let mut card = laid_out_box().on_alt_press(move |_| sink.set(true));
        settle(&mut card);

        assert_eq!(
            card.on_event(&press_with(50.0, 50.0, PointerButton::Primary)),
            EventResult::Ignored,
            "a primary press must still fall through to whatever is behind the box"
        );
        card.on_event(&release_with(50.0, 50.0, PointerButton::Primary));
        assert!(!alt.get(), "the primary button is not an alt press");
    }

    // What a wrapper forwarding an optional `on_alt_press` needs: `None` must leave the box transparent to a right-click, not turn it into one that reports the press handled and swallows the context menu behind it.
    #[test]
    fn maybe_on_alt_press_of_none_lets_a_secondary_press_fall_through() {
        let mut card = laid_out_box().maybe_on_alt_press(None::<fn(PointerButton)>);
        settle(&mut card);

        assert_eq!(
            card.on_event(&press_with(50.0, 50.0, PointerButton::Secondary)),
            EventResult::Ignored
        );
    }

    #[test]
    fn a_plain_pressable_box_still_ignores_non_primary_buttons() {
        let tapped = Rc::new(Cell::new(false));
        let sink = tapped.clone();
        let mut card = laid_out_box().on_press(move || sink.set(true));
        settle(&mut card);

        assert_eq!(
            card.on_event(&press_with(50.0, 50.0, PointerButton::Secondary)),
            EventResult::Ignored,
            "right-click keeps passing through a box that never asked for it"
        );
        card.on_event(&release_with(50.0, 50.0, PointerButton::Secondary));
        assert!(!tapped.get(), "on_press is a primary-button gesture");
    }

    #[test]
    fn releasing_a_different_button_than_armed_completes_nothing() {
        let seen: Rc<Cell<Option<PointerButton>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let tapped = Rc::new(Cell::new(false));
        let tap_sink = tapped.clone();
        let mut card = laid_out_box()
            .on_press(move || tap_sink.set(true))
            .on_alt_press(move |b| sink.set(Some(b)));
        settle(&mut card);

        card.on_event(&press_with(50.0, 50.0, PointerButton::Secondary));
        card.on_event(&release_with(50.0, 50.0, PointerButton::Primary));
        assert_eq!(
            seen.take(),
            None,
            "the right button armed it, the left cannot complete it"
        );
        assert!(!tapped.get());
    }

    #[test]
    fn dragging_off_the_box_cancels_an_alt_press() {
        let seen: Rc<Cell<Option<PointerButton>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let mut card = laid_out_box().on_alt_press(move |b| sink.set(Some(b)));
        settle(&mut card);

        card.on_event(&press_with(50.0, 50.0, PointerButton::Secondary));
        card.on_event(&Event::PointerMoved {
            x: 95.0,
            y: 95.0,
            source: PointerSource::Mouse,
        });
        card.on_event(&release_with(95.0, 95.0, PointerButton::Secondary));
        assert_eq!(
            seen.take(),
            None,
            "travel past the tap slop cancels an alt press just as it cancels a tap"
        );
    }

    // A child that handles the press (an inner button) wins; the box's own on_press must stay silent.
    #[test]
    fn inner_button_press_wins_over_box() {
        let card_flag = Rc::new(Cell::new(false));
        let btn_flag = Rc::new(Cell::new(false));
        let cf = card_flag.clone();
        let bf = btn_flag.clone();
        reset_layout_runtime();
        // A pressable primitive child stands in for the old high-level Button (now in ui-components).
        let btn = StyledContainer::new(
            LayoutStyle::new().width(50.0).height(30.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || bf.set(true));
        let btn_node = btn.layout_node();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default(),
            vec![Box::new(btn)],
        )
        .unwrap()
        .on_press(move || cf.set(true));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let br = track_layout(btn_node).unwrap().get();
        let (cx, cy) = (
            (br.x + br.width / 2.0) as f64,
            (br.y + br.height / 2.0) as f64,
        );
        card.on_event(&press(cx, cy, PointerSource::Mouse));
        card.on_event(&release(cx, cy, PointerSource::Mouse));
        assert!(btn_flag.get(), "the inner button should fire");
        assert!(
            !card_flag.get(),
            "the box on_press must not fire when a child handled the press"
        );
    }

    /// A ring is not another state but a different question — where the keyboard is going — so it composes
    /// with whichever state won instead of replacing it. A hovered box that lost its ring would hide that
    /// answer exactly when the user reached for the mouse.
    #[test]
    fn a_focus_ring_is_drawn_over_the_state_that_won_not_instead_of_it() {
        reset_layout_runtime();
        let hover_fill = Color::rgba(0.9, 0.9, 0.9, 1.0);
        let ring = Border::uniform(Color::rgba(0.0, 0.4, 1.0, 1.0), 2.0);
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
            vec![],
        )
        .unwrap()
        .hover_style(move |_r| RectStyle::default().with_fill(hover_fill))
        .focus_style(move |_r| RectStyle {
            border: Some(ring),
            ..RectStyle::default()
        });
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let id = card.focusable.id.expect("a ring makes the box focusable");
        focus::request(id);
        card.on_event(&Event::PointerMoved {
            x: 100.0,
            y: 50.0,
            source: PointerSource::Mouse,
        });

        let painted = rect_style(&card.view()).expect("the box paints a rect");
        assert_eq!(
            painted.fill,
            Some(renderer_core::Paint::Solid(hover_fill)),
            "the hover fill survives the ring"
        );
        assert_eq!(painted.border, Some(ring), "and the ring is drawn over it");
        focus::release(id);
    }

    /// `:focus-visible`, which CSS spent years arriving at: a ring on every click is noise, and the ring drawn
    /// anyway is why so many stylesheets turned outlines off altogether and took the keyboard's only cue with
    /// them. Focus taken by a tap shows none; focus taken any other way does.
    #[test]
    fn a_tap_takes_focus_without_drawing_a_ring() {
        reset_layout_runtime();
        let ring = Border::uniform(Color::rgba(0.0, 0.4, 1.0, 1.0), 2.0);
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .focus_style(move |_r| RectStyle {
            border: Some(ring),
            ..RectStyle::default()
        });
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        let id = card.focusable.id.expect("a ring makes the box focusable");

        card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
        assert!(focus::is_focused(id), "the tap did take focus");
        assert_eq!(
            rect_style(&card.view()).and_then(|s| s.border),
            None,
            "but drew no ring for it"
        );

        // Reached with the keyboard instead, and the ring is exactly what says so.
        focus::request(id);
        assert_eq!(rect_style(&card.view()).and_then(|s| s.border), Some(ring));
        focus::release(id);
    }

    /// The bug this exists to make unwritable, taken from a real port: a control the application had already
    /// disabled still lit up with the accent under the pointer and still showed a hand cursor, because the
    /// author remembered to guard the callback and the tint but not the hover and the cursor. Three places to
    /// remember is three places to get wrong, so the box reads one flag and closes all of them.
    #[test]
    fn a_disabled_box_neither_lights_up_nor_fires() {
        reset_layout_runtime();
        let presses = Rc::new(Cell::new(0u32));
        let sink = presses.clone();
        let enabled = signal(false);
        let flag = enabled;
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
            vec![],
        )
        .unwrap()
        .hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)))
        .on_press(move || sink.set(sink.get() + 1))
        .disabled(move || !flag.get());
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let base = fill_color(&card.view());
        card.on_event(&Event::PointerMoved {
            x: 100.0,
            y: 50.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(
            fill_color(&card.view()),
            base,
            "a disabled box does not take the hover paint"
        );
        card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
        card.on_event(&release(100.0, 50.0, PointerSource::Mouse));
        assert_eq!(presses.get(), 0, "and its press callback never fires");

        // Enabling it needs nothing else: the flag is re-read, not sampled at construction.
        enabled.set(true);
        card.on_event(&Event::PointerMoved {
            x: 100.0,
            y: 50.0,
            source: PointerSource::Mouse,
        });
        assert_ne!(fill_color(&card.view()), base, "now it hovers");
        card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
        card.on_event(&release(100.0, 50.0, PointerSource::Mouse));
        assert_eq!(presses.get(), 1);
    }

    /// The shape a `surface_local!` world was supposed to be unable to survive: a style closure reading the
    /// very rect the layout pass that runs it is about to write. `styled_by` makes it reachable from any
    /// widget, since `style()` is an arbitrary closure the author wrote.
    ///
    /// It settles instead of panicking, and the reason is worth pinning: `compute_layout` collects the
    /// `(signal, rect)` updates *while* holding the layout-runtime borrow and applies them only after
    /// releasing it, so the flush that re-runs this closure never re-enters a live borrow. The remaining
    /// failure mode of this shape is a re-layout cycle, which has its own named assert.
    #[test]
    fn a_style_effect_that_reads_the_rect_its_own_layout_pass_just_wrote_settles_instead_of_panicking()
     {
        reset_layout_runtime();
        let card = StyledContainer::new(
            LayoutStyle::new().width(200.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap();
        let node = card.layout_node();
        let seen = track_layout(node).expect("the container registers a rect signal");
        let settled = seen;
        let runs = Rc::new(Cell::new(0u32));
        let counted = runs.clone();
        // Half of its own laid-out width — the port's shape, and unlike deriving height from height it has a fixed point worth asserting.
        let card = card.styled_by(move || {
            counted.set(counted.get() + 1);
            let width = seen.get().width;
            LayoutStyle::new()
                .width(200.0)
                .height((width * 0.5).max(1.0))
        });

        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        assert!(runs.get() >= 1, "the style closure ran");
        assert_eq!(
            settled.peek().height,
            100.0,
            "and the rect it derives itself from came to rest instead of running away"
        );
    }

    /// The other half of the port's bug, and the one that used to be a wall rather than an oversight:
    /// `cursor:` compiles from a literal and never passed through the signal path, so `cursor:$enabled` was
    /// not expressible at all. It does not need to be — the box suppresses the shape while disabled, so the
    /// attribute stays a literal and the framework answers the question.
    #[test]
    fn a_disabled_box_does_not_claim_the_pointer_shape() {
        use platform_core::take_window_commands;

        reset_layout_runtime();
        let enabled = signal(false);
        let flag = enabled;
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .cursor(Cursor::Pointer)
        .disabled(move || !flag.get());
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let over = Event::PointerMoved {
            x: 100.0,
            y: 50.0,
            source: PointerSource::Mouse,
        };
        let _ = take_window_commands();
        card.on_event(&over);
        assert!(
            take_window_commands().is_empty(),
            "a disabled box asks for no cursor at all"
        );

        enabled.set(true);
        card.on_event(&over);
        assert!(
            take_window_commands()
                .iter()
                .any(|c| matches!(c, WindowCommand::SetCursor(Cursor::Pointer))),
            "and asks for it again once it can be used"
        );

        // Disabled again with the pointer still inside: nothing else hands the shape back while it never leaves.
        enabled.set(false);
        card.on_event(&over);
        assert!(
            take_window_commands()
                .iter()
                .any(|c| matches!(c, WindowCommand::SetCursor(Cursor::Default))),
            "the shape is given back when the box stops accepting the pointer"
        );
    }

    /// Disabling a box while the pointer is inside it has to take back what it was already showing — nothing
    /// else will, since the pointer never leaves and the box stops accepting the moves that would settle it.
    #[test]
    fn disabling_a_hovered_box_takes_the_hover_back() {
        reset_layout_runtime();
        let enabled = signal(true);
        let flag = enabled;
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
            vec![],
        )
        .unwrap()
        .hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)))
        .disabled(move || !flag.get());
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let base = fill_color(&card.view());
        let moved = Event::PointerMoved {
            x: 100.0,
            y: 50.0,
            source: PointerSource::Mouse,
        };
        card.on_event(&moved);
        assert_ne!(fill_color(&card.view()), base, "hovered while enabled");

        enabled.set(false);
        card.on_event(&moved);
        assert_eq!(
            fill_color(&card.view()),
            base,
            "the highlight goes with the ability to act on it"
        );
    }

    /// The disabled paint wins over the pressed one, which already won over hover — so a box cannot be shown
    /// mid-press and unusable at the same time.
    #[test]
    fn the_disabled_paint_wins_over_every_other_state() {
        reset_layout_runtime();
        let off = Color::rgba(0.5, 0.5, 0.5, 1.0);
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
            vec![],
        )
        .unwrap()
        .hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)))
        .active_style(|_r| RectStyle::default().with_fill(Color::rgba(0.7, 0.7, 0.7, 1.0)))
        .disabled_style(move |_r| RectStyle::default().with_fill(off))
        .disabled(|| true);
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        card.on_event(&Event::PointerMoved {
            x: 100.0,
            y: 50.0,
            source: PointerSource::Mouse,
        });
        card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
        assert_eq!(fill_color(&card.view()), off);
    }

    /// `disabled` on a region means the region, as an HTML `fieldset` does: a wrapper with no handlers of its
    /// own still has to stop the pointer reaching what is inside it, or a disabled panel is disabled only in
    /// the places nobody put a control.
    #[test]
    fn a_disabled_wrapper_shields_its_children() {
        reset_layout_runtime();
        let presses = Rc::new(Cell::new(0u32));
        let sink = presses.clone();
        let inner = StyledContainer::new(
            LayoutStyle::new().width(200.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || sink.set(sink.get() + 1));
        let mut wrapper = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default(),
            vec![Box::new(inner)],
        )
        .unwrap()
        .disabled(|| true);
        compute_layout(
            wrapper.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        wrapper.on_event(&press(100.0, 50.0, PointerSource::Mouse));
        wrapper.on_event(&release(100.0, 50.0, PointerSource::Mouse));
        assert_eq!(presses.get(), 0);
    }

    // A hover style swaps the box's fill while the mouse is over it (mouse only), and clears on leave.
    #[test]
    fn hover_style_swaps_on_mouse_move() {
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
            vec![],
        )
        .unwrap()
        .hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let normal = fill_color(&card.view());
        card.on_event(&Event::PointerMoved {
            x: 100.0,
            y: 50.0,
            source: PointerSource::Mouse,
        });
        let hovered = fill_color(&card.view());
        assert_ne!(normal, hovered, "hover should swap the fill");

        card.on_event(&Event::PointerMoved {
            x: 9999.0,
            y: 9999.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(
            fill_color(&card.view()),
            normal,
            "leaving the box restores the base fill"
        );
    }

    // Touch never sets hover (no "pointer left" on touch), so a tap leaves no stuck hover style.
    #[test]
    fn touch_move_does_not_set_hover() {
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
            vec![],
        )
        .unwrap()
        .hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let normal = fill_color(&card.view());
        card.on_event(&Event::PointerMoved {
            x: 100.0,
            y: 50.0,
            source: PointerSource::Touch { id: 1 },
        });
        assert_eq!(
            fill_color(&card.view()),
            normal,
            "a touch move must not trigger hover"
        );
    }

    // A press inside swaps to the active (pressed) fill; the release restores the base fill.
    #[test]
    fn active_style_swaps_on_press_and_clears_on_release() {
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
            vec![],
        )
        .unwrap()
        .active_style(|_r| RectStyle::default().with_fill(Color::rgba(0.5, 0.5, 0.5, 1.0)));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let normal = fill_color(&card.view());
        card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
        assert_ne!(
            normal,
            fill_color(&card.view()),
            "press swaps to the active fill"
        );
        card.on_event(&release(100.0, 50.0, PointerSource::Mouse));
        assert_eq!(
            fill_color(&card.view()),
            normal,
            "release restores the base fill"
        );
    }

    // Pressed wins over hover: pressing while hovering shows the active fill, and releasing (still inside)
    // falls back to the hover fill.
    #[test]
    fn active_style_takes_precedence_over_hover() {
        reset_layout_runtime();
        let hover = Color::rgba(0.9, 0.9, 0.9, 1.0);
        let active = Color::rgba(0.4, 0.4, 0.4, 1.0);
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
            vec![],
        )
        .unwrap()
        .hover_style(move |_r| RectStyle::default().with_fill(hover))
        .active_style(move |_r| RectStyle::default().with_fill(active));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        card.on_event(&Event::PointerMoved {
            x: 100.0,
            y: 50.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(
            fill_color(&card.view()),
            hover,
            "hovering shows the hover fill"
        );
        card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
        assert_eq!(
            fill_color(&card.view()),
            active,
            "pressing while hovered shows the active fill (precedence)"
        );
        card.on_event(&release(100.0, 50.0, PointerSource::Mouse));
        assert_eq!(
            fill_color(&card.view()),
            hover,
            "releasing inside falls back to the hover fill"
        );
    }

    // Dragging the press off the box clears the pressed state, so it never sticks.
    #[test]
    fn active_style_clears_when_press_drags_off() {
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
            vec![],
        )
        .unwrap()
        .active_style(|_r| RectStyle::default().with_fill(Color::rgba(0.5, 0.5, 0.5, 1.0)));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let normal = fill_color(&card.view());
        card.on_event(&press(100.0, 50.0, PointerSource::Mouse));
        assert_ne!(normal, fill_color(&card.view()), "press activates");
        card.on_event(&Event::PointerMoved {
            x: 9999.0,
            y: 9999.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(
            fill_color(&card.view()),
            normal,
            "dragging off the box clears the pressed state"
        );
    }

    // The background rect's whole style, for assertions about how the states composed rather than about one field.
    fn rect_style(view: &RenderNode) -> Option<RectStyle> {
        let RenderNode::Group { children, .. } = view else {
            return None;
        };
        match children.first() {
            Some(RenderNode::Primitive(renderer_core::DrawCommand::Rect { style, .. })) => {
                Some(**style)
            }
            _ => None,
        }
    }

    fn fill_color(view: &RenderNode) -> Color {
        let group = match view {
            RenderNode::Group { children, .. } => children,
            _ => panic!("expected Group"),
        };
        if let RenderNode::Primitive(renderer_core::DrawCommand::Rect { style, .. }) = &group[0] {
            if let Some(renderer_core::Paint::Solid(c)) = style.fill {
                return c;
            }
        }
        panic!("expected a solid-fill background rect");
    }

    // A scroll gesture that begins on the box (press then drag past the slop) must not press it.
    #[test]
    fn scroll_drag_does_not_press_box() {
        let flag = Rc::new(Cell::new(false));
        let f = flag.clone();
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(200.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || f.set(true));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        let touch = PointerSource::Touch { id: 1 };
        card.on_event(&press(50.0, 20.0, touch.clone()));
        card.on_event(&Event::PointerMoved {
            x: 50.0,
            y: 120.0, // > TAP_SLOP away
            source: touch.clone(),
        });
        card.on_event(&release(50.0, 120.0, touch));
        assert!(!flag.get(), "a scroll drag over the box must not press it");
    }

    // on_drag fires on a press inside, on every subsequent move (even once the pointer leaves the box),
    // then stops after release.
    #[test]
    fn on_drag_reports_press_then_moves_until_release() {
        use std::cell::RefCell;
        let seen: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(200.0).height(200.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_drag(move |x, y| sink.borrow_mut().push((x, y)));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        let moved = |x: f64, y: f64| Event::PointerMoved {
            x,
            y,
            source: PointerSource::Mouse,
        };
        card.on_event(&press(40.0, 40.0, PointerSource::Mouse));
        card.on_event(&moved(80.0, 90.0));
        card.on_event(&moved(400.0, 400.0)); // outside the box: drag still tracks
        card.on_event(&release(400.0, 400.0, PointerSource::Mouse));
        card.on_event(&moved(10.0, 10.0)); // after release: no longer dragging

        assert_eq!(
            *seen.borrow(),
            vec![(40.0, 40.0), (80.0, 90.0), (400.0, 400.0)],
            "drag reports the press point then each move until release"
        );
    }

    /// A click and a drag on the same button stop overlapping once a threshold is set: below it the stroke is
    /// only a press, above it only a drag. A viewport is the case — a click picks what is under it and a drag
    /// orbits — and without this a one-pixel wobble did both.
    #[test]
    fn a_threshold_splits_a_click_from_a_drag_on_the_same_button() {
        use std::cell::Cell;
        use std::cell::RefCell;

        let build = || {
            let clicks: Rc<Cell<u32>> = Rc::new(Cell::new(0));
            let drags: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
            reset_layout_runtime();
            let (c, d) = (clicks.clone(), drags.clone());
            let card = StyledContainer::new(
                LayoutStyle::new().flex_column().width(200.0).height(200.0),
                |_r| RectStyle::default(),
                vec![],
            )
            .unwrap()
            .drag_threshold(4.0)
            .on_press(move || c.set(c.get() + 1))
            .on_drag(move |x, y| d.borrow_mut().push((x, y)));
            compute_layout(
                card.layout_node(),
                AvailableSpace::Definite(200.0),
                AvailableSpace::Definite(200.0),
            )
            .unwrap();
            (card, clicks, drags)
        };
        let moved = |x: f64, y: f64| Event::PointerMoved {
            x,
            y,
            source: PointerSource::Mouse,
        };

        // A hand that shifts a pixel between press and release still meant to click.
        let (mut card, clicks, drags) = build();
        card.on_event(&press(40.0, 40.0, PointerSource::Mouse));
        card.on_event(&moved(41.0, 40.0));
        card.on_event(&release(41.0, 40.0, PointerSource::Mouse));
        assert_eq!(clicks.get(), 1, "the click survives the wobble");
        assert!(drags.borrow().is_empty(), "and nothing was dragged");

        // And one that travels is a drag, which is no longer also a click.
        let (mut card, clicks, drags) = build();
        card.on_event(&press(40.0, 40.0, PointerSource::Mouse));
        card.on_event(&moved(90.0, 40.0));
        card.on_event(&release(90.0, 40.0, PointerSource::Mouse));
        assert_eq!(clicks.get(), 0, "a drag is not also a click");
        assert_eq!(*drags.borrow(), vec![(90.0, 40.0)]);
    }

    // Regression: a drag released OUTSIDE the widget must still end. Dispatched through a parent (whose
    // release path position-filters presses) — the release must broadcast to the dragging child anyway,
    // else it stays stuck to the pointer (fires on_drag on later moves).
    #[test]
    fn drag_released_outside_bounds_ends_via_parent_dispatch() {
        use std::cell::RefCell;
        let seen: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        reset_layout_runtime();
        let child = StyledContainer::new(
            LayoutStyle::new().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_drag(move |x, y| sink.borrow_mut().push((x, y)));
        let mut parent = Container::new(
            LayoutStyle::new().flex_column().width(300.0).height(300.0),
            vec![Box::new(child)],
        )
        .unwrap();
        compute_layout(
            parent.layout_node(),
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(300.0),
        )
        .unwrap();

        let moved = |x: f64, y: f64| Event::PointerMoved {
            x,
            y,
            source: PointerSource::Mouse,
        };
        // child sits at (0,0) 100×100. Press inside, drag well outside, release outside.
        parent.on_event(&press(50.0, 50.0, PointerSource::Mouse));
        parent.on_event(&moved(250.0, 250.0));
        parent.on_event(&release(250.0, 250.0, PointerSource::Mouse));
        // After release the drag must be over: a later move fires nothing.
        parent.on_event(&moved(60.0, 60.0));
        assert_eq!(
            *seen.borrow(),
            vec![(50.0, 50.0), (250.0, 250.0)],
            "drag ended on the outside release; the post-release move must not fire"
        );
    }

    // `on_drag_end` is what makes a threshold gesture (swipe-to-dismiss, drag-to-open) expressible: it fires
    // exactly once per drag, with the position it finished at.
    #[test]
    fn on_drag_end_fires_once_with_the_release_position() {
        use std::cell::RefCell;
        let ends: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = ends.clone();
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_drag_end(move |x, y| sink.borrow_mut().push((x, y)));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let moved = |x: f64, y: f64| Event::PointerMoved {
            x,
            y,
            source: PointerSource::Mouse,
        };
        card.on_event(&moved(10.0, 10.0));
        assert!(ends.borrow().is_empty(), "a move with no drag ends nothing");

        card.on_event(&press(20.0, 20.0, PointerSource::Mouse));
        card.on_event(&moved(70.0, 30.0));
        assert!(ends.borrow().is_empty(), "still dragging");
        card.on_event(&release(90.0, 40.0, PointerSource::Mouse));
        assert_eq!(
            *ends.borrow(),
            vec![(90.0, 40.0)],
            "the release position, not the last move — a drag can end past it"
        );

        // Exactly once: a second release with no drag in flight fires nothing.
        card.on_event(&release(95.0, 45.0, PointerSource::Mouse));
        assert_eq!(ends.borrow().len(), 1);
    }

    /// The pointer reaching the edge of the window is not the end of the gesture, and treating it as one is
    /// what makes an orbit stop dead against the border of a viewport that fills its window. The drag was
    /// armed by a press this widget took; it ends when that press is released, or when the window loses the
    /// focus that would have carried the release ([`losing_window_focus_ends_a_live_drag`]).
    #[test]
    fn a_drag_survives_the_cursor_leaving_the_window() {
        use std::cell::RefCell;
        let moves: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
        let ends: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
        let move_sink = moves.clone();
        let end_sink = ends.clone();
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_drag(move |x, y| move_sink.borrow_mut().push((x, y)))
        .on_drag_end(move |x, y| end_sink.borrow_mut().push((x, y)));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        card.on_event(&press(20.0, 20.0, PointerSource::Mouse));
        card.on_event(&Event::PointerMoved {
            x: 60.0,
            y: 25.0,
            source: PointerSource::Mouse,
        });
        card.on_event(&Event::CursorLeft);
        assert!(
            ends.borrow().is_empty(),
            "leaving the window does not finish the drag"
        );

        // Past the border the coordinates go negative, which is what a local drag reports once the pointer is outside the bounds.
        card.on_event(&Event::PointerMoved {
            x: -15.0,
            y: 25.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(
            moves.borrow().last().copied(),
            Some((-15.0, 25.0)),
            "the drag is still reporting after the pointer left"
        );

        card.on_event(&release(-15.0, 25.0, PointerSource::Mouse));
        assert_eq!(
            *ends.borrow(),
            vec![(-15.0, 25.0)],
            "the release is what ends it, wherever it lands"
        );
    }

    /// The other half of [`a_drag_survives_the_cursor_leaving_the_window`], and the reason the two have to
    /// land together: a window that loses focus never sends the release for what was held, and Alt-Tab with a
    /// button down never crosses the border. Before `CursorLeft` stopped ending drags this was latent — it
    /// only looked safe because leaving was aggressive enough to usually coincide.
    #[test]
    fn losing_window_focus_ends_a_live_drag() {
        use std::cell::RefCell;
        let ends: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = ends.clone();
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_drag_end(move |x, y| sink.borrow_mut().push((x, y)));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        card.on_event(&press(20.0, 20.0, PointerSource::Mouse));
        card.on_event(&Event::PointerMoved {
            x: 60.0,
            y: 25.0,
            source: PointerSource::Mouse,
        });
        card.on_event(&Event::FocusChanged { is_focused: false });
        assert_eq!(
            *ends.borrow(),
            vec![(60.0, 25.0)],
            "the last position the drag reached, since the loss carries none of its own"
        );

        // And exactly once: the gesture is disarmed, so regaining focus and moving reports nothing more.
        card.on_event(&Event::FocusChanged { is_focused: true });
        card.on_event(&Event::PointerMoved {
            x: 70.0,
            y: 30.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(ends.borrow().len(), 1);
    }

    // `on_drag_end` alone is enough to make a box draggable: a gesture that only cares about the outcome
    // should not have to register a per-move callback it ignores.
    #[test]
    fn on_drag_end_works_without_an_on_drag() {
        use std::cell::RefCell;
        let ends: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = ends.clone();
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_drag_end(move |x, y| sink.borrow_mut().push((x, y)));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        card.on_event(&press(10.0, 10.0, PointerSource::Mouse));
        card.on_event(&release(80.0, 10.0, PointerSource::Mouse));
        assert_eq!(*ends.borrow(), vec![(80.0, 10.0)]);
    }

    // A focusable box fires on_focus(true) when tapped and on_focus(false) when focus is cleared.
    #[test]
    fn on_focus_fires_on_gain_and_loss() {
        use std::cell::RefCell;
        let seen: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_focus(move |f| sink.borrow_mut().push(f));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        card.on_event(&press(50.0, 50.0, PointerSource::Mouse)); // tap focuses → on_focus(true)
        crate::focus::clear(); // → on_focus(false)
        assert_eq!(
            *seen.borrow(),
            vec![true, false],
            "on_focus fires true on gain then false on loss"
        );
    }

    // What a wrapper forwarding an optional `on_focus` needs: `None` must not join the tab order.
    #[test]
    fn maybe_on_focus_of_none_does_not_join_the_tab_order() {
        reset_layout_runtime();
        focus::clear();
        let card = StyledContainer::new(
            LayoutStyle::new().width(80.0).height(30.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .maybe_on_focus(None::<fn(bool)>);
        assert!(card.focusable.id.is_none(), "no handler, no focus id");

        focus::focus_next();
        assert!(focus::exposed().is_empty(), "and it is not a tab stop");
    }

    #[test]
    fn maybe_on_focus_of_some_fires_like_on_focus() {
        use std::cell::RefCell;
        let seen: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        reset_layout_runtime();
        let mut card = StyledContainer::new(
            LayoutStyle::new().flex_column().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .maybe_on_focus(Some(move |f| sink.borrow_mut().push(f)));
        compute_layout(
            card.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        card.on_event(&press(50.0, 50.0, PointerSource::Mouse));
        crate::focus::clear();
        assert_eq!(*seen.borrow(), vec![true, false]);
    }

    // A pressable box publishes its laid-out rect to the interactive registry (so a carved-input-region surface
    // receives input over it), and withdraws it on drop.
    #[test]
    fn pressable_publishes_rect_to_interactive_registry_and_withdraws_on_drop() {
        use crate::interactive_rects;
        reset_layout_runtime();
        let baseline = interactive_rects().len();
        let card = StyledContainer::new(
            LayoutStyle::new().width(120.0).height(40.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(|| {});
        let node = card.layout_node();
        // Zero-sized before layout, so it contributes nothing yet.
        assert_eq!(
            interactive_rects().len(),
            baseline,
            "an unlaid-out pressable contributes no rect"
        );
        compute_layout(
            node,
            AvailableSpace::Definite(120.0),
            AvailableSpace::Definite(40.0),
        )
        .unwrap();
        let rects = interactive_rects();
        assert_eq!(rects.len(), baseline + 1);
        assert!(
            rects.iter().any(|r| r.width == 120.0 && r.height == 40.0),
            "a laid-out pressable reports its rect"
        );
        drop(card);
        assert_eq!(
            interactive_rects().len(),
            baseline,
            "dropping the pressable withdraws its rect"
        );
    }

    /// The span an effect belonging to a widget wants, and the two failures either side of it: dropped on the
    /// floor it runs once and stops, parked somewhere longer-lived it keeps firing at a node that is gone.
    ///
    /// The widget used to hold the handle, which is why `keeping` existed. The owner holds it now, so the
    /// span is the scope the widget was *built* in rather than the widget value's own Rust lifetime — which
    /// is the same span for every widget a view produces, and unlike the handle it does not need a field.
    #[test]
    fn an_effect_lives_exactly_as_long_as_the_scope_that_built_it() {
        crate::reset_layout_runtime();
        reactive_core::reset_runtime();
        let source = signal(0i32);
        let seen = std::rc::Rc::new(std::cell::Cell::new(0i32));

        let sink = seen.clone();
        let scope = reactive_core::owner_scope();
        let owner = scope.id();
        let _boxed =
            StyledContainer::new(LayoutStyle::new(), |_r| RectStyle::default(), vec![]).unwrap();
        effect(move || sink.set(source.get()));
        drop(scope);

        source.set(7);
        assert_eq!(seen.get(), 7, "the effect runs while the scope is alive");

        reactive_core::dispose_owner(owner);
        source.set(9);
        assert_eq!(seen.get(), 7, "and stops when the scope is disposed");
    }
}
