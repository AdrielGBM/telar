//! GPU path tessellation via Vello (Finding 4.3).
//!
//! When the `vello-paths` feature is enabled, `DrawCommand::Path` entries are
//! recorded into a `vello::Scene` and rasterized on the GPU through Vello's
//! compute pipeline (flatten -> binning -> coarse -> fine), bypassing the Lyon
//! CPU tessellator. The whole module is feature-gated so the dependency and its
//! code are absent from default builds.

#[cfg(feature = "vello-paths")]
pub(crate) use imp::VelloPathRenderer;

#[cfg(feature = "vello-paths")]
mod imp {
    use std::num::NonZeroUsize;

    use renderer_core::{Color, FillRule, LineCap, LineJoin, PathData, PathStyle, PathVerb};
    use vello::kurbo::{Affine, BezPath, Cap, Join, Point, Stroke as KurboStroke};
    use vello::peniko::{Brush, Color as VelloColor, Fill};
    use vello::{AaConfig, AaSupport, Renderer, RendererOptions, Scene};

    // Vello requires the target texture to use this exact format with STORAGE_BINDING usage.
    pub(crate) const VELLO_TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
    const VELLO_TARGET_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::STORAGE_BINDING
        .union(wgpu::TextureUsages::TEXTURE_BINDING)
        .union(wgpu::TextureUsages::COPY_SRC);

    // Area AA keeps shader-variant compilation (and thus startup cost) minimal; it is Vello's recommended default for general UI.
    const VELLO_AA: AaConfig = AaConfig::Area;

    /// Accumulates path draw commands into a `vello::Scene` and rasterizes them on
    /// the GPU into a pooled offscreen texture. The result is composited by the
    /// caller through the existing `CompositePipeline`.
    pub(crate) struct VelloPathRenderer {
        renderer: Renderer,
        scene: Scene,
        // Whether any path has been recorded into the scene since the last reset; lets the caller skip the GPU pass and compositing entirely on path-free frames.
        has_content: bool,
        // Pooled render target, reallocated only when the required dimensions grow.
        target: Option<VelloTarget>,
    }

    struct VelloTarget {
        view: wgpu::TextureView,
        width: u32,
        height: u32,
    }

    impl VelloPathRenderer {
        pub(crate) fn new(device: &wgpu::Device) -> Option<Self> {
            let renderer = Renderer::new(
                device,
                RendererOptions {
                    use_cpu: false,
                    antialiasing_support: AaSupport::area_only(),
                    num_init_threads: NonZeroUsize::new(1),
                    pipeline_cache: None,
                },
            );
            match renderer {
                Ok(renderer) => Some(Self {
                    renderer,
                    scene: Scene::new(),
                    has_content: false,
                    target: None,
                }),
                Err(e) => {
                    tracing::warn!(
                        "failed to initialize vello renderer, falling back to lyon: {e}"
                    );
                    None
                }
            }
        }

        /// Clears the accumulated scene before a new frame's paths are recorded.
        pub(crate) fn reset(&mut self) {
            self.scene.reset();
            self.has_content = false;
        }

        /// Records a single path (fill and/or stroke) into the scene. `transform`
        /// is the world-space affine (column-major `[a, b, c, d, e, f]`) already
        /// resolved by the renderer's draw state, so recorded coordinates are in
        /// the same target space as the rest of the frame.
        pub(crate) fn add_path(&mut self, data: &PathData, style: &PathStyle, transform: [f32; 6]) {
            let path = build_bez_path(data);
            if path.elements().is_empty() {
                return;
            }
            let affine = Affine::new([
                transform[0] as f64,
                transform[1] as f64,
                transform[2] as f64,
                transform[3] as f64,
                transform[4] as f64,
                transform[5] as f64,
            ]);

            if let Some(fill) = style.fill {
                let fill_rule = match style.fill_rule {
                    FillRule::EvenOdd => Fill::EvenOdd,
                    FillRule::Winding => Fill::NonZero,
                };
                let brush = Brush::Solid(to_vello_color(fill.solid_color()));
                self.scene.fill(fill_rule, affine, &brush, None, &path);
                self.has_content = true;
            }

            if let Some(stroke) = style.stroke {
                let kurbo_stroke = KurboStroke::new(stroke.width as f64)
                    .with_caps(to_kurbo_cap(stroke.cap))
                    .with_join(to_kurbo_join(stroke.join));
                let brush = Brush::Solid(to_vello_color(stroke.paint.solid_color()));
                self.scene
                    .stroke(&kurbo_stroke, affine, &brush, None, &path);
                self.has_content = true;
            }
        }

        /// Rasterizes the accumulated scene on the GPU into the pooled target and
        /// returns a view of it. Returns `None` when nothing was recorded or when
        /// the GPU pass fails (caller should fall back to leaving paths unrendered
        /// for this frame; Lyon is the path when the feature is off).
        pub(crate) fn render(
            &mut self,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            width: u32,
            height: u32,
        ) -> Option<&wgpu::TextureView> {
            if !self.has_content || width == 0 || height == 0 {
                return None;
            }

            self.ensure_target(device, width, height);
            let target = self.target.as_ref()?;

            let params = vello::RenderParams {
                base_color: VelloColor::TRANSPARENT,
                width,
                height,
                antialiasing_method: VELLO_AA,
            };

            if let Err(e) =
                self.renderer
                    .render_to_texture(device, queue, &self.scene, &target.view, &params)
            {
                tracing::warn!("vello render_to_texture failed: {e}");
                return None;
            }

            Some(&self.target.as_ref()?.view)
        }

        // Grows the pooled target to cover the requested size; never shrinks, to avoid per-frame reallocation churn when window size oscillates.
        fn ensure_target(&mut self, device: &wgpu::Device, width: u32, height: u32) {
            let needs_alloc = match &self.target {
                Some(t) => t.width < width || t.height < height,
                None => true,
            };
            if !needs_alloc {
                return;
            }
            let alloc_w = self.target.as_ref().map_or(width, |t| t.width.max(width));
            let alloc_h = self
                .target
                .as_ref()
                .map_or(height, |t| t.height.max(height));
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("rsx-vello-path-target"),
                size: wgpu::Extent3d {
                    width: alloc_w,
                    height: alloc_h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: VELLO_TARGET_FORMAT,
                usage: VELLO_TARGET_USAGE,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.target = Some(VelloTarget {
                view,
                width: alloc_w,
                height: alloc_h,
            });
        }
    }

    fn build_bez_path(data: &PathData) -> BezPath {
        let mut path = BezPath::new();
        let mut open = false;
        for verb in data.verbs() {
            match verb {
                PathVerb::MoveTo(p) => {
                    if open {
                        // BezPath does not auto-close; an unterminated subpath simply ends here.
                    }
                    path.move_to(Point::new(p.x as f64, p.y as f64));
                    open = true;
                }
                PathVerb::LineTo(p) => {
                    if open {
                        path.line_to(Point::new(p.x as f64, p.y as f64));
                    }
                }
                PathVerb::QuadTo { ctrl, to } => {
                    if open {
                        path.quad_to(
                            Point::new(ctrl.x as f64, ctrl.y as f64),
                            Point::new(to.x as f64, to.y as f64),
                        );
                    }
                }
                PathVerb::CubicTo { ctrl1, ctrl2, to } => {
                    if open {
                        path.curve_to(
                            Point::new(ctrl1.x as f64, ctrl1.y as f64),
                            Point::new(ctrl2.x as f64, ctrl2.y as f64),
                            Point::new(to.x as f64, to.y as f64),
                        );
                    }
                }
                PathVerb::Close => {
                    if open {
                        path.close_path();
                        open = false;
                    }
                }
            }
        }
        path
    }

    fn to_vello_color(c: Color) -> VelloColor {
        // Color stores premultiplied-agnostic linear-ish f32 components in [0, 1]; peniko expects the same component range.
        VelloColor::new([c.r, c.g, c.b, c.a])
    }

    fn to_kurbo_cap(cap: LineCap) -> Cap {
        match cap {
            LineCap::Butt => Cap::Butt,
            LineCap::Round => Cap::Round,
            LineCap::Square => Cap::Square,
        }
    }

    fn to_kurbo_join(join: LineJoin) -> Join {
        match join {
            LineJoin::Miter => Join::Miter,
            LineJoin::Round => Join::Round,
            LineJoin::Bevel => Join::Bevel,
        }
    }
}
