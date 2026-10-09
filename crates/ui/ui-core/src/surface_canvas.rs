//! [`SurfaceCanvas`]: a tree with a [`Surface`] of its own, driven by whatever holds it rather than by a runner.
//!
//! Its overlays, dismiss stack, focus, size, safe area and breakpoints are its own, and so are the writing direction, locale, control size, theme mode and high contrast wherever it is given one, so it can sit inside another tree — through a [`SurfaceFrame`](crate::SurfaceFrame) — or inside an application's texture, or stand as a window-sized layer over an app, without either side reaching into the other.

use std::cell::{Cell, Ref};
use std::rc::Rc;

use geometry_core::{Insets, Rect, Size, Transform};
use layout_core::{AvailableSpace, Direction, LayoutError, LayoutStyle, NodeId, SizeDimension};
use platform_core::{AccessNode, Event, WindowCommand};
use reactive_core::{RwSignal, in_surface_world, root_scope, signal};
use renderer_core::{BorderRadius, DrawCommand};
use theme_core::ControlSize;
use ui_tree::{Component, ComponentList, EventResult, RenderNode, SegmentNodeInfo};

use crate::context::{compute_layout, mark_dirty, new_container};
use crate::layout_item::LayoutItem;
use crate::surface_context::{Surface, SurfaceGuard};

/// A tree with a surface of its own. See the module documentation.
///
/// Everything it does happens with its surface entered, so the tree reads its own size, overlays and focus whichever surface the caller had active.
pub struct SurfaceCanvas {
    tree: ComponentList,
    root: NodeId,
    size: RwSignal<Size>,
    safe_area: RwSignal<Insets>,
    scale: RwSignal<f32>,
    placement: Cell<Transform>,
    // Declared last so it drops last: the tree and its content release their state while this surface's worlds still exist.
    surface: Rc<Surface>,
}

/// The environment a [`SurfaceCanvas`] is built in: each value it names is the surface's own, as the canvas's setters give it, and each `None` follows the thread's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceEnv {
    /// A mode registered with [`register_mode`](theme_core::register_mode), by id.
    pub mode: Option<String>,
    /// A BCP 47 language tag.
    pub locale: Option<String>,
    pub direction: Option<Direction>,
    pub control_size: Option<ControlSize>,
    /// `Some(true)` for more contrast, `Some(false)` for the regular palette, whatever the application and the system say.
    pub high_contrast: Option<bool>,
    /// How far in from each edge the system keeps the surface for itself, as a device's notch and home indicator do. Zero keeps nothing.
    pub safe_area: Insets,
}

impl SurfaceEnv {
    fn apply(&self) {
        theme_core::set_surface_mode(self.mode.as_deref());
        i18n_core::set_surface_locale(self.locale.as_deref());
        layout_reactive::set_surface_direction(self.direction);
        theme_core::set_surface_control_size(self.control_size);
        preferences_core::set_surface_high_contrast(self.high_contrast);
    }
}

/// The content plus the box it fills, so "this surface is 390×844" holds whatever the content's own style says: a percent-sized parent turns the surface size into a definite box the content stretches into, as a window root does for a windowed tree.
struct SurfaceRoot {
    content: Box<dyn LayoutItem>,
}

impl Component for SurfaceRoot {
    fn view(&self) -> RenderNode {
        self.content.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.content.on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        "SurfaceRoot"
    }
}

impl SurfaceCanvas {
    /// Builds `build`'s content on a new surface of `size` logical units, filling it.
    ///
    /// `build` runs with the surface entered and under a root scope of its own, so what it creates lives exactly as long as this canvas and reads no context from the scope it was called in. Nothing is laid out until [`resize`](Self::resize) or [`lay_out`](Self::lay_out).
    pub fn new(
        size: Size,
        build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
    ) -> Result<Self, LayoutError> {
        Self::new_in(size, &SurfaceEnv::default(), build)
    }

    /// [`new`](Self::new) on a surface already in `env`: what `build` reads of the mode, locale, direction, control size, high contrast and safe area — once, or reactively — is `env`'s from the first build on. Setting them afterwards re-runs only the reactive readers, so a canvas shown in an environment of its own is built in it.
    pub fn new_in(
        size: Size,
        env: &SurfaceEnv,
        build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
    ) -> Result<Self, LayoutError> {
        Self::with_root_in(size, env, || {
            let content = build()?;
            let root = new_container(
                LayoutStyle::new()
                    .width(SizeDimension::Percent(1.0))
                    .height(SizeDimension::Percent(1.0)),
                &[content.layout_node()],
            )?;
            Ok((SurfaceRoot { content }, root))
        })
    }

    /// [`new`](Self::new) for content that is a root already: `build` returns the component the tree mounts and the layout node sized to the surface.
    pub fn with_root<C: Component + 'static>(
        size: Size,
        build: impl FnOnce() -> Result<(C, NodeId), LayoutError>,
    ) -> Result<Self, LayoutError> {
        Self::with_root_in(size, &SurfaceEnv::default(), build)
    }

    /// [`with_root`](Self::with_root) on a surface already in `env`, as [`new_in`](Self::new_in) is.
    pub fn with_root_in<C: Component + 'static>(
        size: Size,
        env: &SurfaceEnv,
        build: impl FnOnce() -> Result<(C, NodeId), LayoutError>,
    ) -> Result<Self, LayoutError> {
        let surface = Surface::new();
        let (tree, root, signals) = {
            let _entered = surface.enter();
            let signals =
                in_surface_world(|| (signal(size), signal(env.safe_area), signal(1.0f32)));
            crate::set_surface_size(size);
            crate::set_safe_area_insets(env.safe_area);
            env.apply();
            let _scope = root_scope();
            let (component, root) = build()?;
            (ComponentList::new(component), root, signals)
        };
        let (size, safe_area, scale) = signals;
        Ok(Self {
            tree,
            root,
            size,
            safe_area,
            scale,
            placement: Cell::new(Transform::IDENTITY),
            surface,
        })
    }

    /// The surface the tree lives on.
    pub fn surface(&self) -> &Rc<Surface> {
        &self.surface
    }

    /// Activates this canvas's world — its layout tree, overlays, focus — for as long as the guard lives: what building a widget for this tree, reading one of its rects or opening one of its overlays from outside has to happen inside.
    #[must_use = "the canvas's world is only active while this guard is alive"]
    pub fn enter(&self) -> SurfaceGuard {
        self.surface.enter()
    }

    /// The surface's logical size. Reactive.
    pub fn size(&self) -> Size {
        self.size.get()
    }

    /// Lays the tree out at `size` logical units, which every `use_surface_size` inside then reads, and tells the content its box changed with the [`Event::WindowResized`] a windowed tree hears.
    pub fn resize(&self, size: Size) {
        self.lay_out(size);
        // Content that lays itself out on resize — a scroll viewport, a shell that repositions its panels — learns its new box the way a windowed tree does, because that is the idiom it was written to.
        self.dispatch_tree(&Event::WindowResized {
            width: size.width.round().max(0.0) as u32,
            height: size.height.round().max(0.0) as u32,
        });
    }

    /// [`resize`](Self::resize) without telling the content: for a driver whose content learns its box some other way.
    pub fn lay_out(&self, size: Size) {
        if self.size.peek() != size {
            self.size.set(size);
        }
        let _entered = self.enter();
        crate::set_surface_size(size);
        let _ = mark_dirty(self.root);
        let _ = compute_layout(
            self.root,
            AvailableSpace::Definite(size.width),
            AvailableSpace::Definite(size.height),
        );
    }

    /// Lays out again what the tree's own reactive changes dirtied since the last pass, at the size it has — the per-frame step a runner takes for a windowed tree.
    pub fn relayout_if_dirty(&self) {
        let _entered = self.enter();
        crate::relayout_if_dirty();
    }

    /// How far in from each edge the system keeps the surface for itself, as `use_safe_area_insets` reads it inside. Reactive.
    pub fn safe_area(&self) -> Insets {
        self.safe_area.get()
    }

    pub fn set_safe_area(&self, insets: Insets) {
        if self.safe_area.peek() != insets {
            self.safe_area.set(insets);
        }
        let _entered = self.enter();
        crate::set_safe_area_insets(insets);
    }

    /// The writing direction the surface lays out in, where it has one of its own; `None` where it follows [`set_direction`](crate::set_direction). Reactive.
    pub fn direction(&self) -> Option<Direction> {
        let _entered = self.enter();
        layout_reactive::use_surface_direction()
    }

    /// Lays the surface out in `direction` whatever the thread's is, or with `None` in the thread's again. Only this surface lays out anew, on its next pass.
    pub fn set_direction(&self, direction: Option<Direction>) {
        let _entered = self.enter();
        layout_reactive::set_surface_direction(direction);
    }

    /// The locale the surface's text is translated into, where it has one of its own; `None` where it follows [`set_locale`](i18n_core::set_locale). Reactive.
    pub fn locale(&self) -> Option<String> {
        let _entered = self.enter();
        i18n_core::use_surface_locale()
    }

    /// Translates the surface's text into `locale` whatever the thread's is, or with `None` into the thread's again. Re-renders only the text inside that reads it.
    pub fn set_locale(&self, locale: Option<&str>) {
        let _entered = self.enter();
        i18n_core::set_surface_locale(locale);
    }

    /// The size of the controls on the surface, where it has one of its own; `None` where it follows [`set_control_size`](theme_core::set_control_size). Reactive.
    pub fn control_size(&self) -> Option<ControlSize> {
        let _entered = self.enter();
        theme_core::use_surface_control_size()
    }

    /// Sizes the surface's controls at `size` whatever the thread's is, or with `None` at the thread's again. Re-runs only the readers inside.
    pub fn set_control_size(&self, size: Option<ControlSize>) {
        let _entered = self.enter();
        theme_core::set_surface_control_size(size);
    }

    /// The theme mode the surface is shown in, where it has one of its own; `None` where it follows [`set_mode`](theme_core::set_mode). Reactive.
    pub fn mode(&self) -> Option<String> {
        let _entered = self.enter();
        theme_core::use_surface_mode()
    }

    /// Shows the surface in mode `id` whatever the thread's is, or with `None` in the thread's again. Its mode-following tokens resolve as that mode, and where the mode was registered with [`register_mode_theme`](theme_core::register_mode_theme), whatever inside has no theme provided resolves that theme. Re-runs only the readers inside; the application's theme stays installed as it is.
    pub fn set_mode(&self, id: Option<&str>) {
        let _entered = self.enter();
        theme_core::set_surface_mode(id);
    }

    /// Whether the surface is shown with more contrast, where it makes that choice itself; `None` where it follows [`set_high_contrast_override`](preferences_core::set_high_contrast_override) and the system. Reactive.
    pub fn high_contrast(&self) -> Option<bool> {
        let _entered = self.enter();
        preferences_core::use_surface_high_contrast()
    }

    /// Shows the surface with more contrast (`Some(true)`) or without (`Some(false)`) whatever the application and the system say, or with `None` as they say again. Re-runs only the readers of `use_high_contrast` inside.
    pub fn set_high_contrast(&self, high: Option<bool>) {
        let _entered = self.enter();
        preferences_core::set_surface_high_contrast(high);
    }

    /// How many units of the tree around it one logical unit of this surface covers when a [`SurfaceFrame`](crate::SurfaceFrame) draws it: a zoom, independent of the surface's own size. Reactive.
    pub fn scale(&self) -> f32 {
        self.scale.get()
    }

    pub fn set_scale(&self, scale: f32) {
        let scale = scale.max(f32::MIN_POSITIVE);
        if self.scale.peek() != scale {
            self.scale.set(scale);
        }
    }

    pub(crate) fn scale_now(&self) -> f32 {
        self.scale.peek()
    }

    pub(crate) fn size_now(&self) -> Size {
        self.size.peek()
    }

    /// Where the surface is shown: maps a point in its logical space to a point in the space its events arrive in.
    pub fn placement(&self) -> Transform {
        self.placement.get()
    }

    /// Sets [`placement`](Self::placement). Whoever draws the surface knows this and nobody else does: without it, pointers arrive in the outer space and hit-testing drifts from the picture by exactly the offset and zoom it is shown at.
    pub fn set_placement(&self, placement: Transform) {
        self.placement.set(placement);
    }

    /// Routes an event that arrived in the outer space: pointers are mapped back through the [`placement`](Self::placement), the surface's overlays see it first, and the tree only if none of them took it — but for a move, which reaches the tree [`covered`](crate::covered).
    pub fn dispatch(&self, event: &Event) -> EventResult {
        let mapped = crate::transform_pointer(event, self.placement.get().to_array());
        let event = mapped.as_ref().unwrap_or(event);
        if self.dispatch_overlays(event) == EventResult::Handled {
            if matches!(event, Event::PointerMoved { .. }) {
                crate::covered(|| self.dispatch_tree(event));
            }
            return EventResult::Handled;
        }
        self.dispatch_tree(event)
    }

    /// Offers an event already in the surface's own coordinates to its overlay layer alone: a live drag, then the dismiss stack, then the overlays by priority. In its own batch, so an overlay handler's writes flush after the walk.
    pub fn dispatch_overlays(&self, event: &Event) -> EventResult {
        let _entered = self.enter();
        reactive_core::batch(|| crate::dispatch_overlays(event))
    }

    /// Hands an event already in the surface's own coordinates to the tree, past the overlay layer.
    pub fn dispatch_tree(&self, event: &Event) -> EventResult {
        let _entered = self.enter();
        self.tree.dispatch(event)
    }

    /// The tree's current frame in the surface's own coordinates, its overlays drawn last.
    pub fn frame_commands(&self) -> Ref<'_, Vec<DrawCommand>> {
        self.tree.commands()
    }

    /// Whether the frame changed since [`frame_commands`](Self::frame_commands) last composed it.
    pub fn is_dirty(&self) -> bool {
        self.tree.is_dirty()
    }

    /// Bumped whenever the frame is composed anew; two equal reads mean the same frame.
    pub fn generation(&self) -> u64 {
        self.tree.generation()
    }

    /// The tree as a child of another tree's `view()`: drawn as this surface's whole frame, overlays included, where the parent puts it. Place it with [`composite_surface`].
    pub fn boundary(&self) -> RenderNode {
        self.tree.boundary()
    }

    /// The mounted components in pre-order, as the inspector lists them.
    pub fn walk(&self, out: &mut Vec<SegmentNodeInfo>) {
        self.tree.walk_tree(out);
    }

    /// What a screen reader is told about this surface, in its own coordinates. See [`accessibility::snapshot`](crate::accessibility::snapshot).
    pub fn access_snapshot(&self) -> Vec<AccessNode> {
        let commands = self.frame_commands();
        let _entered = self.enter();
        crate::accessibility::snapshot(&commands)
    }

    /// Drains what the tree asked of the window — a pointer shape, a title bar's drag. The surface has no window of its own, so whoever holds it decides which of these reach the real one.
    pub fn take_window_commands(&self) -> Vec<WindowCommand> {
        let _entered = self.enter();
        platform_core::take_window_commands()
    }
}

impl Drop for SurfaceCanvas {
    fn drop(&mut self) {
        // Background work and timers this canvas started must not outlive it: their callbacks close over this surface's state. Scoped to this surface so a sibling tree's are left running.
        reactive_core::cancel_tasks_for(self.surface.handle());
        reactive_core::cancel_timers_for(self.surface.handle());
    }
}

/// Places a surface's content in an outer tree: drawn through `placement` and clipped to where its `size` lands, so nothing it draws — its own overlays included — spills over what is around it.
pub fn composite_surface(
    placement: Transform,
    size: Size,
    content: impl IntoIterator<Item = RenderNode>,
) -> RenderNode {
    RenderNode::clip(
        placed_bounds(placement, size),
        BorderRadius::zero(),
        [RenderNode::transform_with(placement.to_array(), content)],
    )
}

fn placed_bounds(placement: Transform, size: Size) -> Rect {
    if placement.b == 0.0 && placement.c == 0.0 {
        let (width, height) = (placement.a * size.width, placement.d * size.height);
        return Rect::new(
            placement.e + width.min(0.0),
            placement.f + height.min(0.0),
            width.abs(),
            height.abs(),
        );
    }
    let corners = [
        (0.0, 0.0),
        (size.width, 0.0),
        (0.0, size.height),
        (size.width, size.height),
    ]
    .map(|(x, y)| placement.apply(geometry_core::Point::new(x, y)));
    let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for corner in corners {
        min_x = min_x.min(corner.x);
        min_y = min_y.min(corner.y);
        max_x = max_x.max(corner.x);
        max_y = max_y.max(corner.y);
    }
    Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

#[cfg(test)]
#[path = "surface_canvas_test.rs"]
mod tests;
