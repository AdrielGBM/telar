//! A Telar UI that composes into a texture the application owns, at a resolution the application picks.
//!
//! The mirror of [`gpu::image`](crate::gpu::image). There, an application fills a texture and Telar places
//! it in its frame; here, Telar draws inside a picture the application is assembling. Both directions leave
//! Telar not knowing what the rest of the picture is of, which is the whole of the arrangement: it needs a
//! *where* and a *how big*, never a *what*.
//!
//! Three independent axes fall out of it, and they are deliberately not packaged together:
//!
//! - **Where Telar composes.** Into a window, or into somebody else's texture. A shell treating its UI as a
//!   layer it composites itself, a video editor putting its UI in the frame it encodes, a thumbnail with no
//!   window and no trip through the CPU.
//! - **At what resolution.** A [`TextureUi`] lays out against its own target, so a UI at 320×180 and the
//!   window chrome around it are two trees at two resolutions in one window. A viewport that renders at half
//!   resolution while it is being dragged is the same axis from the other end.
//! - **On which grid the text lands.** That one is a `TextStyle` axis
//!   ([`GlyphRaster`](crate::GlyphRaster)), not a property of this type, so it is available to a window tree
//!   as much as to this one.
//!
//! The tree here is a real Telar tree: layout, hit-testing, scroll, overlays, motion and i18n all work. It
//! owns its own [`Surface`], so its layout world, overlay registry and focus are its own — that is what lets
//! it live beside a window tree on the same thread without the two reaching into each other.
//!
//! The application drives it. There is no event loop and no window: [`render`](TextureUi::render) composes
//! one frame when asked, and [`on_event`](TextureUi::on_event) takes events the application forwards, mapped
//! back through [`place_in`](TextureUi::place_in) into the texture's own coordinates.

use std::rc::Rc;

use geometry_core::{Rect, Transform};
use layout_core::{AvailableSpace, LayoutError, LayoutStyle, SizeDimension};
use platform_core::Event;
use renderer_core::{Color, RenderBackend, RendererError};
use renderer_hardware::HardwareRenderer;
use renderer_hardware::gpu::wgpu;
use ui_core::{
    Component, ComponentList, EventResult, LayoutItem, NodeId, RenderNode, Surface, compute_layout,
    mark_dirty, new_container,
};

/// Why a [`TextureUi`] could not be built.
#[derive(Debug)]
pub enum TextureUiError {
    /// The content's layout tree could not be assembled.
    Layout(LayoutError),
    /// No GPU renderer could be built against the shared device.
    Renderer(RendererError),
}

impl std::fmt::Display for TextureUiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Layout(e) => write!(f, "texture UI layout failed: {e}"),
            Self::Renderer(e) => write!(f, "texture UI renderer failed: {e:?}"),
        }
    }
}

impl std::error::Error for TextureUiError {}

impl From<LayoutError> for TextureUiError {
    fn from(e: LayoutError) -> Self {
        Self::Layout(e)
    }
}

impl From<RendererError> for TextureUiError {
    fn from(e: RendererError) -> Self {
        Self::Renderer(e)
    }
}

/// The content plus the box it fills. The box exists because "this UI is 320×180" has to be true whatever
/// the content's own style says: a percent-sized parent turns the target's pixel size into a definite box
/// the content stretches into, exactly as a window root does for a windowed tree.
struct Root {
    content: Box<dyn LayoutItem>,
}

impl Component for Root {
    fn view(&self) -> RenderNode {
        self.content.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.content.on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        "TextureUiRoot"
    }
}

/// A Telar UI composed into an application-owned texture. See the [module docs](self).
pub struct TextureUi {
    renderer: HardwareRenderer<platform_headless::HeadlessWindow>,
    tree: ComponentList,
    root: NodeId,
    width: u32,
    height: u32,
    scale: f32,
    clear_color: Color,
    // Maps a point in the texture to a point in the window, so `on_event` can send the pointer the other way. Identity until the application says where it is showing the texture.
    placement: Transform,
    // Declared last so it drops last: the tree and its content release their state while this surface's layout, overlay and focus worlds still exist.
    surface: Rc<Surface>,
}

impl TextureUi {
    /// Builds a UI that composes into `target`.
    ///
    /// `build` runs with this UI's surface active, so the content's layout nodes, overlays and effects are
    /// allocated in *its* world rather than in whatever surface happened to be active.
    ///
    /// `scale` is the target's device-pixel ratio: the tree is laid out at `target size / scale` logical
    /// pixels and drawn at the target's pixel size. Pass `1.0` for a target whose pixels *are* its layout
    /// units — a 320×180 sprite, say.
    ///
    /// Requirements on `target`: it must come from the device Telar lends
    /// ([`gpu::shared`](crate::gpu::shared)) and carry `RENDER_ATTACHMENT` usage; its format decides the
    /// format this renderer's pipelines are built against, so it must be renderable and blendable. The
    /// application keeps ownership in every sense that matters — it decides the size, the format and when
    /// the contents change; Telar only draws into it when asked.
    pub fn new(
        target: wgpu::Texture,
        scale: f32,
        build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
    ) -> Result<Self, TextureUiError> {
        Self::with_fonts(target, scale, Vec::new(), Vec::new(), build)
    }

    /// [`new`](Self::new) carrying faces of its own — `font_paths` on disk and `font_data` embedded — in the
    /// same shape [`AppConfig`](crate::AppConfig) takes them. The reason to reach for it is a face drawn on
    /// a pixel grid, to pair with [`GlyphRaster::Pixel`](crate::GlyphRaster::Pixel).
    ///
    /// **Loading a face is not choosing it.** These go into the font database; which one the text actually
    /// shapes with is the process-wide default family, so a caller that wants its own face has to name it
    /// with [`set_default_font_family`](crate::set_default_font_family) *before* building this — otherwise
    /// the face is loaded and the platform's own is drawn, which looks like the file failed to load and did
    /// not.
    ///
    /// Two things follow from how much of this is process-wide. These faces join the one font database every
    /// shaper is built from, so they stay loaded for the rest of the process — but a window already drawing
    /// keeps the shaper it built before they arrived, so a face meant for both still belongs in the app
    /// config. And there is one family per process: a pixel face here and a different one in the window
    /// around it is not expressible today, because `TextStyle` has no family axis.
    pub fn with_fonts(
        target: wgpu::Texture,
        scale: f32,
        font_paths: Vec<std::path::PathBuf>,
        font_data: Vec<Vec<u8>>,
        build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
    ) -> Result<Self, TextureUiError> {
        // The tree built below measures its text, and a texture UI can be the only Telar in the process — there may be no runner that installed a measurer.
        crate::install_default_text_metrics();
        let (width, height) = (target.width(), target.height());
        let renderer = HardwareRenderer::for_texture(
            target,
            None,
            false,
            crate::runner::offscreen_hardware_font_config(font_paths, font_data),
        )?;

        let surface = Surface::new();
        let (tree, root) = {
            let _g = surface.enter();
            let content = build()?;
            let root = new_container(
                LayoutStyle::new()
                    .width(SizeDimension::Percent(1.0))
                    .height(SizeDimension::Percent(1.0)),
                &[content.layout_node()],
            )?;
            (ComponentList::new(Root { content }), root)
        };

        let mut ui = Self {
            renderer,
            tree,
            root,
            width,
            height,
            scale: scale.max(f32::MIN_POSITIVE),
            // Telar's own target is cleared to transparency so the composed frame carries only what the tree drew; the application's texture then keeps everything the frame left uncovered. A `LoadOp::Load` here instead would read a multisample target the resolve already discarded.
            clear_color: Color::TRANSPARENT,
            placement: Transform::IDENTITY,
            surface,
        };
        ui.lay_out();
        Ok(ui)
    }

    /// The colour Telar's own frame starts from before the tree draws, blended into the application's
    /// texture afterwards. Transparent by default, which is what composing *into* a picture means; an
    /// opaque colour makes the UI own every pixel of the target instead.
    ///
    /// A colour and not an `Option`: "keep what was there" is what the blend already does, and asking the
    /// frame's own target to load instead would read a multisample surface the previous resolve discarded.
    pub fn set_clear_color(&mut self, clear_color: Color) {
        self.clear_color = clear_color;
    }

    /// Points the UI at a new texture and re-lays the tree out — how an application resizes it.
    ///
    /// The new texture must match the format of the one this UI was built with; a different format needs a
    /// new [`TextureUi`], because the render pipelines bake it in.
    pub fn resize(&mut self, target: wgpu::Texture, scale: f32) {
        self.width = target.width();
        self.height = target.height();
        self.scale = scale.max(f32::MIN_POSITIVE);
        self.renderer.compose_into(target);
        self.lay_out();
    }

    /// Where the texture is shown, as the rectangle it occupies in the window — the other half of
    /// [`resize`](Self::resize), and what makes a pointer land where the user sees it.
    ///
    /// Only the application knows this: it is the one that decided where to draw the texture and how to fit
    /// it. Without it the pointer arrives in window coordinates and hit-testing drifts from the picture by
    /// exactly the offset and zoom of the placement.
    pub fn place_in(&mut self, dest: Rect) {
        let (w, h) = self.logical_size();
        self.placement = Transform {
            a: dest.width / w.max(f32::MIN_POSITIVE),
            b: 0.0,
            c: 0.0,
            d: dest.height / h.max(f32::MIN_POSITIVE),
            e: dest.x,
            f: dest.y,
        };
    }

    /// [`place_in`](Self::place_in) for a placement the application composed itself — a rotation, a flip, a
    /// transform it already holds. Maps a point in the UI's logical space to a point in the window.
    pub fn set_placement(&mut self, placement: Transform) {
        self.placement = placement;
    }

    /// Activates this UI's world — its layout tree, overlay registry, focus and services — for as long as
    /// the guard lives.
    ///
    /// Anything that touches those has to happen inside it: they are this UI's, and outside it the ambient
    /// ones a window tree uses are what answer. Building a widget to hand to this tree, reading a node's
    /// rect, opening one of its overlays.
    #[must_use = "the UI's world is only active while this guard is alive"]
    pub fn enter(&self) -> ui_core::SurfaceGuard {
        self.surface.enter()
    }

    /// The UI's logical size: the target's pixels divided by its scale, which is the box the tree lays out
    /// against and the space [`on_event`](Self::on_event) delivers pointers in.
    pub fn logical_size(&self) -> (f32, f32) {
        (
            self.width as f32 / self.scale,
            self.height as f32 / self.scale,
        )
    }

    /// Whether the composition changed since the last [`render`](Self::render) — the gate an application
    /// uses to skip re-composing a frame that would come out identical.
    pub fn is_dirty(&self) -> bool {
        self.tree.is_dirty()
    }

    /// Routes an event into the UI, mapping pointer coordinates back through the placement.
    ///
    /// `true` means the UI consumed it and the application should not act on it as well. Non-pointer events
    /// (keys, focus) pass through untransformed; the application decides which of them this UI should see at
    /// all, which is the only honest answer when several trees share one window's keyboard.
    pub fn on_event(&mut self, event: &Event) -> bool {
        let _g = self.surface.enter();
        let mapped = ui_core::transform_pointer(event, self.placement.to_array());
        let event = mapped.as_ref().unwrap_or(event);
        // Overlays first and in their own batch, exactly as the runner does: a modal has to refuse the event to the content behind it, and its handlers' signal writes must flush after the walk.
        let consumed =
            reactive_core::batch(|| ui_core::dispatch_overlays(event) == EventResult::Handled);
        consumed || self.tree.on_event(event) == EventResult::Handled
    }

    /// Composes one frame and blends it into the application's texture.
    ///
    /// Blends, rather than replaces: whatever the application drew there survives wherever the UI did not
    /// paint. Call it after the application has filled the texture for this frame, and again whenever it
    /// refills it — Telar draws when asked and never on its own.
    ///
    /// Animations and background work are not advanced here. The motion engine and the task registry are
    /// per-thread, and a windowed application's runner already drives them for every tree on that thread;
    /// a windowless one drives them itself, with [`motion::tick`](crate::motion) and
    /// [`drain_tasks`](crate::drain_tasks), exactly as it drives this.
    pub fn render(&mut self) -> Result<(), RendererError> {
        let _g = self.surface.enter();
        // A reactive change (a list gaining an item, a panel opening) mutates the layout tree without recomputing it; the runner does the same before composing a window frame.
        ui_core::relayout_if_dirty();
        let generation = self.tree.generation();
        self.renderer
            .begin_frame(self.width, self.height, self.scale, generation)?;
        let commands = self.tree.commands();
        self.renderer
            .render_frame(&commands, Some(self.clear_color))
    }

    fn lay_out(&mut self) {
        let _g = self.surface.enter();
        let (w, h) = (
            self.width as f32 / self.scale,
            self.height as f32 / self.scale,
        );
        let _ = mark_dirty(self.root);
        let _ = compute_layout(
            self.root,
            AvailableSpace::Definite(w),
            AvailableSpace::Definite(h),
        );
        // Content that lays itself out on resize — a scroll viewport, a shell that repositions its panels — learns its new box the same way a windowed tree does, because that is the idiom it was written to.
        self.tree.on_event(&Event::WindowResized {
            width: w.round().max(0.0) as u32,
            height: h.round().max(0.0) as u32,
        });
    }
}

impl Drop for TextureUi {
    fn drop(&mut self) {
        // Background work this UI started must not outlive it: its completion callbacks close over this surface's state. Scoped to this surface so a sibling tree's tasks are left running.
        reactive_core::cancel_tasks_for(self.surface.handle());
    }
}
