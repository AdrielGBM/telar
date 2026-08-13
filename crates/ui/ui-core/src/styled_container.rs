use geometry_core::{Rect, Transform};
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::{
    Cursor, Event, Key, NamedKey, PointerButton, PointerSource, ScrollDelta, WindowCommand,
};
use reactive_core::{Effect, RwSignal, effect, signal};
use renderer_core::RectStyle;
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

pub struct StyledContainer {
    node: NodeId,
    rect: RwSignal<Rect>,
    style: Box<dyn Fn(Rect) -> RectStyle>,
    // Swapped in while the pointer is over the box (mouse only), mirroring `Button`'s rect/rect_hover.
    hover_style: Option<Box<dyn Fn(Rect) -> RectStyle>>,
    is_hovered: RwSignal<bool>,
    // Swapped in while a primary pointer is held down inside the box (the pressed / CSS `:active` state),
    // taking precedence over `hover_style`. Mouse and touch; cleared on release, leave, or drag-off.
    active_style: Option<Box<dyn Fn(Rect) -> RectStyle>>,
    is_active: RwSignal<bool>,
    // A closure (not a plain f32) so `view()` re-reads it every run: a reactive opacity or a `transition:opacity` animation resolves to its current value on each re-render.
    opacity: Box<dyn Fn() -> f32>,
    // Resolved per `view()` (like `opacity`) so a `$signal`-driven transform re-reads its current value. Takes the laid-out `Rect` so rotate/scale can pivot on the box centre; `None` means identity (no wrapping node).
    transform: Box<dyn Fn(Rect) -> Option<[f32; 6]>>,
    children: TrackedChildren,
    // Set when the box holds a reactive fragment: static + dynamic children route through the host so
    // they interleave in this node (see `child_host`). `children` is empty in that case.
    dyn_host: Option<DynHost>,
    // Optional tap gesture so a styled box can itself be pressable (a clickable card); children still hit-test first.
    press: PressGesture,
    // Effects whose life is this widget's. Dropping an `Effect` deregisters it, so one that belongs to a widget must be owned by that widget: parked somewhere longer-lived it keeps firing against a node that is gone, and dropped on the floor it runs once and stops. Held here rather than in a wrapper so owning one costs no layout node — a row owning five effects is still one box.
    kept_effects: Vec<Effect>,
    // Optional drag gesture (slider/reorder/resize): reports the pointer position on press and each move.
    drag: DragGesture,
    // Fires with `true`/`false` as the mouse enters/leaves the box (mouse only, like the hover style).
    on_hover: Option<Box<dyn Fn(bool)>>,
    // Fires with the pointer position, local to the box, on every move over it. The continuous half of `on_hover`, which only reports the crossings.
    on_pointer_move: Option<Box<dyn Fn(f32, f32)>>,
    // Fires with the wheel delta while the pointer is over the box.
    on_scroll: Option<Box<dyn Fn(f32, f32)>>,
    // Fires on every key press. Key events carry no pointer position, so they are broadcast to every widget
    // — this is a GLOBAL shortcut handler (there is no per-widget focus), not focused text input.
    on_key: Option<Box<dyn Fn(&Key)>>,
    // When set, the box is focusable: it joins the tab order, takes focus on tap, and handles Tab while
    // focused. `on_focus` observes the transitions.
    focus_id: Option<FocusId>,
    // Watches focus transitions for `on_focus`; dropping it (with the box) tears the subscription down.
    _focus_effect: Option<Effect>,
    // Pointer shape while the box is hovered; restored to the default on leave. Set from `cursor:` in the DSL.
    cursor: Option<Cursor>,
    // Whether the box declines to shadow what it is drawn over (`pointer-events: none`). Set from
    // `click_through` in the DSL; see `LayoutItem::pointer_opaque`.
    click_through: bool,
}

impl StyledContainer {
    pub fn new(
        layout_style: LayoutStyle,
        style: impl Fn(Rect) -> RectStyle + 'static,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        let (node, rect, children) = register_container(layout_style, children)?;
        Ok(Self {
            node,
            rect,
            style: Box::new(style),
            hover_style: None,
            is_hovered: signal(false),
            active_style: None,
            is_active: signal(false),
            opacity: Box::new(|| 1.0),
            transform: Box::new(|_| None),
            children,
            dyn_host: None,
            press: PressGesture::default(),
            kept_effects: Vec::new(),
            drag: DragGesture::default(),
            on_hover: None,
            on_pointer_move: None,
            on_scroll: None,
            on_key: None,
            focus_id: None,
            _focus_effect: None,
            cursor: None,
            click_through: false,
        })
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
        Ok(Self {
            node,
            rect,
            style: Box::new(style),
            hover_style: None,
            is_hovered: signal(false),
            active_style: None,
            is_active: signal(false),
            opacity: Box::new(|| 1.0),
            transform: Box::new(|_| None),
            children: Vec::new(),
            dyn_host: Some(dyn_host),
            press: PressGesture::default(),
            kept_effects: Vec::new(),
            drag: DragGesture::default(),
            on_hover: None,
            on_pointer_move: None,
            on_scroll: None,
            on_key: None,
            focus_id: None,
            _focus_effect: None,
            cursor: None,
            click_through: false,
        })
    }

    fn dispatch_children(&mut self, event: &Event) -> EventResult {
        match &self.dyn_host {
            Some(host) => host.dispatch(event),
            None => dispatch_container_event(&mut self.children, event),
        }
    }

    pub fn with_opacity(mut self, opacity: impl Fn() -> f32 + 'static) -> Self {
        self.opacity = Box::new(opacity);
        self
    }

    /// Apply an affine transform (rotate/scale/translate) to the whole box each `view()`. The closure
    /// takes the laid-out rect and returns the 2×3 matrix, or `None` for identity.
    pub fn with_transform(
        mut self,
        transform: impl Fn(Rect) -> Option<[f32; 6]> + 'static,
    ) -> Self {
        self.transform = Box::new(transform);
        self
    }

    /// Paint the box with `f` while the mouse hovers it (a declarative style swap, like `Button`).
    /// Hover is mouse-only; touch never sets it, so a tap leaves no stuck hover state.
    pub fn on_hover_style(mut self, f: impl Fn(Rect) -> RectStyle + 'static) -> Self {
        self.hover_style = Some(Box::new(f));
        self
    }

    /// Paint the box with `f` while a primary pointer is held down inside it — the pressed / CSS `:active`
    /// state, which takes precedence over `on_hover_style`. Unlike hover it tracks touch as well as mouse,
    /// and it clears on release, on leaving the box, or once the press drags off, so it never sticks.
    pub fn on_active_style(mut self, f: impl Fn(Rect) -> RectStyle + 'static) -> Self {
        self.active_style = Some(Box::new(f));
        self
    }

    /// Whether the box is currently pressed (a primary pointer is held down inside it). Set only when an
    /// `active_style` is present; drives its paint swap and clears on release/leave/drag-off.
    fn set_active(&self, active: bool) {
        if self.active_style.is_some() && self.is_active.get() != active {
            self.is_active.set(active);
        }
    }

    /// Shows `cursor` while the pointer is over this box, and restores the default when it leaves.
    ///
    /// The shape is the app's statement of what the next press will do — orbit, resize a panel, place a
    /// point — so it belongs to the widget that would handle that press, not to a mode the app tracks.
    pub fn cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = Some(cursor);
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

    /// Give this widget ownership of an [`Effect`], so it runs for exactly as long as the widget exists.
    ///
    /// The reactive runtime scopes an effect to the *surface* it was registered on, which is the right span for
    /// a shell-wide subscription and far too coarse for one row of a list: the row goes, the effect stays, and
    /// it keeps firing at a node that is gone. Dropping the handle instead is the opposite failure — the effect
    /// deregisters, runs once, and stops, with nothing to say so. This is the third answer, and the one an
    /// effect that belongs to a widget wants.
    ///
    /// Chainable, so several effects can be kept without nesting anything.
    pub fn keeping(mut self, subscription: Effect) -> Self {
        self.kept_effects.push(subscription);
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
        self.keeping(style_follows(node, style))
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
    pub fn on_alt_press(mut self, f: impl Fn(PointerButton) + 'static) -> Self {
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

    /// Records this box as a pointer target in the per-surface interactive registry, so a surface that carves
    /// its input region from its content (a click-through overlay) receives input over it. See
    /// [`crate::interactive_rects`].
    fn mark_interactive(&self) {
        crate::input_region::register_interactive(self.node, self.rect.read_only());
    }

    /// Fire `f(true)` when the mouse enters the box and `f(false)` when it leaves (mouse only). Independent
    /// of `on_hover_style`: a box can observe hover without swapping its paint.
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
        self.on_hover = Some(Box::new(f));
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
        self.on_pointer_move = Some(Box::new(f));
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
        self.on_scroll = Some(Box::new(f));
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
    pub fn on_focus(mut self, f: impl Fn(bool) + 'static) -> Self {
        let id = *self.focus_id.get_or_insert_with(focus::next_id);
        focus::register(id);
        // An effect fires the callback only on an actual transition (its first run seeds `last`, no fire).
        let last = std::rc::Rc::new(std::cell::Cell::new(focus::is_focused(id)));
        self._focus_effect = Some(effect(move || {
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
        // Pressed wins over hover wins over base. Each `is_*` signal is only read when its style exists, so
        // a plain box's view() stays inert and subscribes to neither.
        let style = if let Some(active) = &self.active_style
            && self.is_active.get()
        {
            active
        } else if let Some(hover) = &self.hover_style
            && self.is_hovered.get()
        {
            hover
        } else {
            &self.style
        };
        let background = RenderNode::rect(
            Rect {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            },
            style(r),
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
        let opacity = (self.opacity)();
        let composed = if opacity < 1.0 {
            RenderNode::layer(opacity, 0.0, [content])
        } else {
            content
        };
        match (self.transform)(r) {
            Some(matrix) => RenderNode::transform_with(matrix, [composed]),
            None => composed,
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // No tap/drag handler, hover style, or event callbacks: behave exactly as a plain container (pure routing).
        if !self.press.is_set()
            && !self.drag.is_set()
            && self.hover_style.is_none()
            && self.active_style.is_none()
            && self.on_hover.is_none()
            && self.on_pointer_move.is_none()
            && self.on_scroll.is_none()
            && self.on_key.is_none()
            && self.focus_id.is_none()
        {
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
                let child = self.dispatch_children(event);
                // Inside the box AND nothing drawn in front of it there: a move is broadcast for the sake of
                // gestures already running, and only the topmost box under the pointer is *hovered*.
                let inside =
                    rect.contains(*x as f32, *y as f32) && !crate::pointer::pointer_occluded();
                // Pressed clears once the pointer drags off the box (mouse or touch) so it never sticks.
                if !inside {
                    self.set_active(false);
                }
                if inside && let Some(cb) = &self.on_pointer_move {
                    cb(*x as f32 - rect.x, *y as f32 - rect.y);
                }
                let tracks_hover =
                    self.hover_style.is_some() || self.on_hover.is_some() || self.cursor.is_some();
                if tracks_hover
                    && matches!(source, PointerSource::Mouse)
                    && inside != self.is_hovered.get()
                {
                    self.is_hovered.set(inside);
                    if let Some(cursor) = self.cursor {
                        platform_core::push_window_command(WindowCommand::SetCursor(if inside {
                            cursor
                        } else {
                            Cursor::Default
                        }));
                    }
                    if let Some(cb) = &self.on_hover {
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
                if self.dispatch_children(event) == EventResult::Handled {
                    self.press.cancel();
                    self.drag.end(None);
                    return EventResult::Handled;
                }
                // A primary press inside the box enters the pressed state (purely visual; independent of on_press).
                if primary && rect.contains(*x as f32, *y as f32) {
                    self.set_active(true);
                }
                // A tap inside a focusable box takes focus (and consumes the press so focus sticks).
                let focused = match self.focus_id {
                    Some(id) if primary && rect.contains(*x as f32, *y as f32) => {
                        focus::request(id);
                        true
                    }
                    _ => false,
                };
                let tapped =
                    self.press.is_set() && self.press.arm(event, rect) == EventResult::Handled;
                let dragged =
                    self.drag.is_set() && self.drag.press(event, rect) == EventResult::Handled;
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
            Event::CursorLeft => {
                self.press.cancel();
                self.drag.end(None);
                self.set_active(false);
                let tracks_hover = self.hover_style.is_some() || self.on_hover.is_some();
                if tracks_hover && self.is_hovered.get() {
                    self.is_hovered.set(false);
                    if let Some(cb) = &self.on_hover {
                        cb(false);
                    }
                }
                self.dispatch_children(event)
            }
            // Children (e.g. a nested scroll area) get first refusal; only then does an `on_scroll` box under the wheel consume it.
            Event::Scrolled { delta, x, y } => {
                if self.dispatch_children(event) == EventResult::Handled {
                    return EventResult::Handled;
                }
                let Some(cb) = &self.on_scroll else {
                    return EventResult::Ignored;
                };
                if !rect.contains(*x as f32, *y as f32) {
                    return EventResult::Ignored;
                }
                let (dx, dy) = match delta {
                    ScrollDelta::Lines { x, y } => (*x * 20.0, *y * 20.0),
                    ScrollDelta::Pixels { x, y } => (*x, *y),
                };
                cb(dx, dy);
                EventResult::Handled
            }
            // Broadcast (no pointer position): fire the global key handler, then keep routing to children.
            Event::KeyPressed { key, modifiers } => {
                // While this focusable box holds focus, Tab moves focus to the next/previous field.
                if let Some(id) = self.focus_id
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
        self._focus_effect.take();
        if let Some(id) = self.focus_id {
            focus::unregister(id);
        }
        crate::input_region::unregister_interactive(self.node);
    }
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
    use theme_core::{Theme, ThemeTokens, set_theme, use_theme};

    use super::*;

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
        focus::register(button);
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
        // Losing the pointer is where a reconstructed state goes wrong: the release never comes.
        crate::observe_pointer(&Event::CursorLeft);
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
    impl Theme for TestTheme {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }
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
    fn theme_button_click_force_tick_no_panic() {
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
            tree.bump_force_ticks();
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
        .on_hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)));
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
        .on_hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)));
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
        .on_active_style(|_r| RectStyle::default().with_fill(Color::rgba(0.5, 0.5, 0.5, 1.0)));
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
        .on_hover_style(move |_r| RectStyle::default().with_fill(hover))
        .on_active_style(move |_r| RectStyle::default().with_fill(active));
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
        .on_active_style(|_r| RectStyle::default().with_fill(Color::rgba(0.5, 0.5, 0.5, 1.0)));
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

    // A drag also ends when the pointer leaves the window, which carries no position. It must still end —
    // otherwise the gesture is stuck armed — reporting the last place it reached.
    #[test]
    fn on_drag_end_still_fires_when_the_cursor_leaves() {
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
        card.on_event(&Event::CursorLeft);
        assert_eq!(
            *ends.borrow(),
            vec![(60.0, 25.0)],
            "the last position the drag reached"
        );
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

    /// The two failures this exists to sit between: an effect dropped on the floor runs once and stops, and one
    /// parked somewhere longer-lived keeps firing at a widget that is gone. Kept on the widget it belongs to, it
    /// does neither.
    #[test]
    fn a_kept_effect_lives_exactly_as_long_as_its_widget() {
        crate::reset_layout_runtime();
        reactive_core::reset_runtime();
        let source = signal(0i32);
        let seen = std::rc::Rc::new(std::cell::Cell::new(0i32));

        let watched = source.clone();
        let sink = seen.clone();
        let boxed = StyledContainer::new(LayoutStyle::new(), |_r| RectStyle::default(), vec![])
            .unwrap()
            .keeping(effect(move || sink.set(watched.get())));

        source.set(7);
        assert_eq!(seen.get(), 7, "the effect runs while the widget is alive");

        drop(boxed);
        source.set(9);
        assert_eq!(
            seen.get(),
            7,
            "and stops when the widget goes, rather than firing at a node that is gone"
        );
    }
}
