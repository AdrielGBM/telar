//! [`SurfaceFrame`]: a [`SurfaceCanvas`] placed in another tree, as one of its widgets.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use geometry_core::{Point, Rect, Size, Transform};
use layout_core::{LayoutError, LayoutStyle, NodeId};
use layout_reactive::RelayoutHook;
use platform_core::{ConsumedKeys, Event, PointerButton, TextInput, WindowCommand};
use reactive_core::{OwnerId, dispose_owner, effect, owner_scope};
use ui_tree::{Component, EventResult, RenderNode};

use crate::focus::{self, FocusId, Keyboard};
use crate::frame_focus::{FrameFocus, Inside, KeyRoute};
use crate::layout_item::LayoutItem;
use crate::layout_leaf::LayoutLeaf;
use crate::surface_canvas::{SurfaceCanvas, composite_surface};
use crate::surface_context::{HomeSurface, Surface};

/// A [`SurfaceCanvas`] drawn where the layout puts this widget, at the canvas's [`scale`](SurfaceCanvas::scale).
///
/// The canvas keeps its own world, so whatever opens inside it stays inside it: a dialog covers the frame and is clipped to it, Escape reaches the frame's dismiss stack and not the page's, and its size, safe area and breakpoints are the frame's.
///
/// - **Pointer.** A press inside the frame, the moves and wheel over it, and every move and release of a gesture that began inside it reach the canvas, mapped into its coordinates; the surface's overlays see them first. A pointer leaving the frame reaches it as [`Event::CursorLeft`].
/// - **Keyboard.** The frame is one stop in the outer tab order and holds the keys while it has focus: a press inside it takes focus, arriving by Tab hands focus to the first focusable inside and arriving by Shift+Tab to the last, and leaving it clears focus inside. Tab moves among the focusables inside and, past the last, on to the stop after the frame, as Shift+Tab past the first does to the stop before it, so the keyboard is never trapped in a canvas; a modal open inside holds Tab within itself until it closes. Whether a field inside has the caret is what the outer tree hears when it asks [`text_entry_focused`](crate::focus::text_entry_focused).
/// - **Window.** The canvas has no window: only the pointer shapes it asks for reach the real one.
pub struct SurfaceFrame {
    canvas: Rc<SurfaceCanvas>,
    home: HomeSurface,
    leaf: LayoutLeaf,
    focus: FrameFocus,
    hovered: Cell<bool>,
    captured: Cell<bool>,
    _relayout: RelayoutHook,
    // What the frame keeps in step with the outer tree, freed with the frame rather than with the scope it was built in.
    owner: OwnerId,
}

#[derive(Clone, Copy, PartialEq)]
enum Sizing {
    Filling,
    Sized,
}

impl SurfaceFrame {
    /// A frame whose box `style` decides; the canvas's logical size follows it, as the box divided by the canvas's scale — a responsive canvas.
    pub fn filling(canvas: Rc<SurfaceCanvas>, style: LayoutStyle) -> Result<Self, LayoutError> {
        Self::build(canvas, style, Sizing::Filling)
    }

    /// A frame whose box is the canvas's logical size times its scale, kept so as either changes — a device of a fixed size, zoomed. `style` places the box; its width and height are the canvas's to decide.
    pub fn sized(canvas: Rc<SurfaceCanvas>, style: LayoutStyle) -> Result<Self, LayoutError> {
        Self::build(canvas, style, Sizing::Sized)
    }

    pub fn canvas(&self) -> &Rc<SurfaceCanvas> {
        &self.canvas
    }

    fn build(
        canvas: Rc<SurfaceCanvas>,
        style: LayoutStyle,
        sizing: Sizing,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(match sizing {
            Sizing::Filling => style.clone(),
            Sizing::Sized => sized_style(style.clone(), canvas.size_now(), canvas.scale_now()),
        })?;
        let focus = FrameFocus::register(leaf.node, Rc::downgrade(&canvas));
        let id = focus.id();
        canvas.host_keyboard();
        let surface = Rc::downgrade(canvas.surface());

        let relayout = layout_reactive::on_relayout({
            let canvas = Rc::downgrade(&canvas);
            let rect = leaf.rect;
            let laid_out = Cell::new(None);
            move || {
                if let Some(canvas) = canvas.upgrade() {
                    keep_up(&canvas, rect.peek(), sizing, &laid_out);
                }
            }
        });

        let scope = owner_scope();
        follow_inner_focus(id, surface);
        if sizing == Sizing::Sized {
            follow_canvas_size(&canvas, leaf.node, style);
        }
        let owner = scope.id();
        drop(scope);

        Ok(Self {
            canvas,
            home: HomeSurface::here(),
            leaf,
            focus,
            hovered: Cell::new(false),
            captured: Cell::new(false),
            _relayout: relayout,
            owner,
        })
    }

    fn placement_now(&self) -> Transform {
        placement(self.leaf.rect.peek(), self.canvas.scale_now())
    }

    fn forward(&self, event: &Event) -> EventResult {
        let result = self.canvas.dispatch(event);
        forward_pointer_shape(&self.canvas);
        result
    }

    fn press(&self, event: &Event, x: f32, y: f32, button: PointerButton) -> EventResult {
        self.captured.set(true);
        self.hovered.set(true);
        if button == PointerButton::Primary {
            focus::request_from_pointer(self.focus.id());
            let local = to_local(self.placement_now(), x, y);
            self.canvas.with_entered(|| {
                focus::blur_from_pointer(local.x, local.y);
            })
        }
        self.forward(event)
    }

    fn moved(&self, event: &Event, inside: bool) -> EventResult {
        let was_hovered = self.hovered.replace(inside);
        if inside || self.captured.get() {
            return self.forward(event);
        }
        if was_hovered {
            self.forward(&Event::CursorLeft);
        }
        EventResult::Ignored
    }

    /// A box event or a screen reader's action for a box drawn inside this frame, or inside a frame nested in it. Focus either moved onto a box in there brings the frame the outer focus too, so the keys that follow reach the box.
    fn in_surface(&self, event: &Event) -> EventResult {
        let result = self.canvas.dispatch_in_surface(event);
        if result == EventResult::Handled {
            self.focus.follow_inside();
        }
        result
    }

    /// Tab inside the canvas, and once it steps past the canvas's last focusable or Shift+Tab past its first, on through the outer tree from the frame.
    fn tab(&self, event: &Event) -> EventResult {
        self.focus.tab(event);
        forward_pointer_shape(&self.canvas);
        EventResult::Handled
    }
}

/// The canvas a frame draws, as its focus stop drives it: gone with the canvas, it holds nothing and takes nothing.
impl Keyboard for Weak<SurfaceCanvas> {
    fn text_entry_focused(&self) -> Option<bool> {
        Some(self.upgrade()?.text_entry_focused())
    }

    fn keeps(&self, keys: ConsumedKeys) -> Option<bool> {
        Some(self.upgrade()?.keeps(keys))
    }
}

impl Inside for Weak<SurfaceCanvas> {
    fn arrive(&self, backwards: bool) {
        if let Some(canvas) = self.upgrade() {
            canvas.arrive(backwards);
        }
    }

    fn tab(&self, key: &Event) -> bool {
        self.upgrade().is_some_and(|canvas| canvas.tab(key))
    }

    fn blur(&self) {
        if let Some(canvas) = self.upgrade() {
            canvas.blur();
        }
    }

    fn holds_focus(&self) -> bool {
        self.upgrade().is_some_and(|canvas| canvas.holds_focus())
    }

    fn text_input(&self) -> Option<(TextInput, Transform)> {
        let canvas = self.upgrade()?;
        Some((canvas.text_input()?, zoom(canvas.scale_now())))
    }
}

impl LayoutItem for SurfaceFrame {
    fn layout_node(&self) -> NodeId {
        self.leaf.node
    }
}

impl Component for SurfaceFrame {
    fn view(&self) -> RenderNode {
        self.leaf.at_layout_position(composite_surface(
            zoom(self.canvas.scale()),
            self.canvas.size(),
            [self.canvas.boundary()],
        ))
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let rect = self.leaf.rect.peek();
        self.canvas.set_placement(self.placement_now());
        let inside = |x: f64, y: f64| rect.contains(x as f32, y as f32);
        match self.focus.key_route(event) {
            Some(KeyRoute::Tab) => return self.tab(event),
            Some(KeyRoute::Inside) => return self.forward(event),
            Some(KeyRoute::Outside) => return EventResult::Ignored,
            None => {}
        }
        match event {
            Event::PointerPressed { x, y, button, .. } if inside(*x, *y) => {
                self.press(event, *x as f32, *y as f32, *button)
            }
            Event::PointerMoved { x, y, .. } => self.moved(event, inside(*x, *y)),
            Event::PointerReleased { .. } => {
                self.captured.set(false);
                self.forward(event)
            }
            Event::Scrolled { x, y, .. } | Event::ScrollEnded { x, y } if inside(*x, *y) => {
                self.forward(event)
            }
            Event::CursorLeft if self.hovered.replace(false) => {
                self.forward(event);
                EventResult::Ignored
            }
            Event::InSurface { .. } => self.in_surface(event),
            // What everything in the window hears, so the frame passes it on and never keeps it from the siblings after it.
            Event::ModifiersChanged { .. }
            | Event::FocusChanged { .. }
            | Event::CursorEntered
            | Event::ScaleFactorChanged { .. }
            | Event::SystemPreferencesChanged { .. } => {
                self.canvas.dispatch_tree(event);
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn debug_name(&self) -> &'static str {
        "SurfaceFrame"
    }
}

impl Drop for SurfaceFrame {
    fn drop(&mut self) {
        self.home.with_entered(|| dispose_owner(self.owner));
    }
}

fn zoom(scale: f32) -> Transform {
    Transform::scale_around(scale, scale, 0.0, 0.0)
}

fn placement(rect: Rect, scale: f32) -> Transform {
    zoom(scale).then(Transform::translate(rect.x, rect.y))
}

fn to_local(placement: Transform, x: f32, y: f32) -> Point {
    let point = Point::new(x, y);
    placement
        .invert()
        .map_or(point, |inverse| inverse.apply(point))
}

fn sized_style(style: LayoutStyle, size: Size, scale: f32) -> LayoutStyle {
    style.width(size.width * scale).height(size.height * scale)
}

/// Run after every layout pass of the tree around the frame, which is what gives the frame its box: places the canvas there, lays it out when its size changed and otherwise picks up what its own changes dirtied.
fn keep_up(canvas: &SurfaceCanvas, rect: Rect, sizing: Sizing, laid_out: &Cell<Option<Size>>) {
    let scale = canvas.scale_now();
    canvas.set_placement(placement(rect, scale));
    let size = match sizing {
        Sizing::Filling => Size::new(rect.width / scale, rect.height / scale),
        Sizing::Sized => canvas.size_now(),
    };
    if laid_out.replace(Some(size)) != Some(size) {
        canvas.resize(size);
    }
    canvas.relayout_if_dirty();
    forward_pointer_shape(canvas);
}

/// The pointer shape is the one thing a surface asks of the window that the window around it should honour; a title bar's drag or close inside a frame is the frame's content, not the window's.
fn forward_pointer_shape(canvas: &SurfaceCanvas) {
    for command in canvas.take_window_commands() {
        if let WindowCommand::SetCursor(_) = command {
            platform_core::push_window_command(command);
        }
    }
}

/// Brings the frame the outer focus whenever focus moves onto a box inside while it does not hold it, however it moved there — a play step, a field focusing itself — so the box that shows the focus is the one the keys reach.
fn follow_inner_focus(id: FocusId, surface: Weak<Surface>) {
    effect(move || {
        let Some(surface) = surface.upgrade() else {
            return;
        };
        let held_inside = surface.with_entered(|| focus::current().is_some());
        // `request` reads the outer focus without subscribing, so losing it out here never takes it back.
        if held_inside {
            focus::request(id);
        }
    });
}

fn follow_canvas_size(canvas: &Rc<SurfaceCanvas>, node: NodeId, style: LayoutStyle) {
    let canvas = Rc::downgrade(canvas);
    effect(move || {
        let Some(canvas) = canvas.upgrade() else {
            return;
        };
        let style = sized_style(style.clone(), canvas.size(), canvas.scale());
        let _ = layout_reactive::set_layout_style(node, style);
    });
}

#[cfg(test)]
#[path = "surface_frame_test.rs"]
mod tests;
