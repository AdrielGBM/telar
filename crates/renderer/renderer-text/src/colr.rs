//! COLR v1 color-glyph rasterization (skrifa + tiny-skia).
//!
//! COLR v1 is a general color-glyph format — commonly emoji, but also colored icons and decorative glyphs. swash 0.2.x returns `None` (or an empty placement) for these glyphs (e.g. Android's NotoColorEmoji.ttf), so cosmic-text silently drops them. Both renderers re-rasterize them here with skrifa (COLR v1 paint traversal) into a tiny-skia pixmap and return straight-alpha RGBA so callers can blit (software) or upload to the glyph atlas (hardware) using the same correct baseline placement.

use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    color::{
        Brush as SkrifaBrush, ColorGlyphFormat, ColorPainter, ColorStop as SkrifaColorStop,
        CompositeMode, Extend, Transform as SkrifaTransform,
    },
    instance::{LocationRef, Size as SkrifaSize},
    metrics::Metrics as SkrifaMetrics,
    outline::{DrawSettings, OutlineGlyphCollection, OutlinePen},
    raw::{TableProvider, types::BoundingBox},
};
use tiny_skia::{
    BlendMode, Color, FillRule, GradientStop, LinearGradient, Mask, Path, PathBuilder, Pixmap,
    Point as TsPoint, RadialGradient, Shader, SpreadMode, Transform,
};

/// A rasterized COLR glyph with straight (non-premultiplied) RGBA8 pixels and swash-style placement: `placement_left` is the horizontal offset from the pen to the bitmap's left edge, `placement_top` the distance from the baseline up to the bitmap's top edge (both in pixels).
pub struct ColrGlyphBitmap {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub placement_left: i32,
    pub placement_top: i32,
}

/// Collects an outline glyph into a tiny-skia path. Wrapped in an `Option` because skrifa may emit no contours for empty glyphs, in which case there is no path to build.
struct PathBuilderPen(Option<PathBuilder>);

impl OutlinePen for PathBuilderPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.get_or_insert_with(PathBuilder::new).move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        if let Some(pb) = &mut self.0 {
            pb.line_to(x, y);
        }
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        if let Some(pb) = &mut self.0 {
            pb.quad_to(cx0, cy0, x, y);
        }
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        if let Some(pb) = &mut self.0 {
            pb.cubic_to(cx0, cy0, cx1, cy1, x, y);
        }
    }
    fn close(&mut self) {
        if let Some(pb) = &mut self.0 {
            pb.close();
        }
    }
}

fn skrifa_to_tiny_skia_transform(t: SkrifaTransform) -> Transform {
    Transform::from_row(t.xx, t.yx, t.xy, t.yy, t.dx, t.dy)
}

fn map_point(transform: &Transform, x: f32, y: f32) -> (f32, f32) {
    let mut p = TsPoint::from_xy(x, y);
    transform.map_point(&mut p);
    (p.x, p.y)
}

fn composite_to_blend(mode: CompositeMode) -> BlendMode {
    match mode {
        CompositeMode::Clear => BlendMode::Clear,
        CompositeMode::Src => BlendMode::Source,
        CompositeMode::Dest => BlendMode::Destination,
        CompositeMode::SrcOver => BlendMode::SourceOver,
        CompositeMode::DestOver => BlendMode::DestinationOver,
        CompositeMode::SrcIn => BlendMode::SourceIn,
        CompositeMode::DestIn => BlendMode::DestinationIn,
        CompositeMode::SrcOut => BlendMode::SourceOut,
        CompositeMode::DestOut => BlendMode::DestinationOut,
        CompositeMode::SrcAtop => BlendMode::SourceAtop,
        CompositeMode::DestAtop => BlendMode::DestinationAtop,
        CompositeMode::Xor => BlendMode::Xor,
        CompositeMode::Plus => BlendMode::Plus,
        CompositeMode::Screen => BlendMode::Screen,
        CompositeMode::Overlay => BlendMode::Overlay,
        CompositeMode::Darken => BlendMode::Darken,
        CompositeMode::Lighten => BlendMode::Lighten,
        CompositeMode::ColorDodge => BlendMode::ColorDodge,
        CompositeMode::ColorBurn => BlendMode::ColorBurn,
        CompositeMode::HardLight => BlendMode::HardLight,
        CompositeMode::SoftLight => BlendMode::SoftLight,
        CompositeMode::Difference => BlendMode::Difference,
        CompositeMode::Exclusion => BlendMode::Exclusion,
        CompositeMode::Multiply => BlendMode::Multiply,
        CompositeMode::HslHue => BlendMode::Hue,
        CompositeMode::HslSaturation => BlendMode::Saturation,
        CompositeMode::HslColor => BlendMode::Color,
        CompositeMode::HslLuminosity => BlendMode::Luminosity,
        _ => BlendMode::SourceOver,
    }
}

fn extend_to_spread(extend: Extend) -> SpreadMode {
    match extend {
        Extend::Pad => SpreadMode::Pad,
        Extend::Repeat => SpreadMode::Repeat,
        Extend::Reflect => SpreadMode::Reflect,
        _ => SpreadMode::Pad,
    }
}

struct PainterLayer {
    pixmap: Pixmap,
    /// Blend mode to use when compositing this layer onto the one below on pop.
    blend_mode: BlendMode,
}

struct TinySkiaPainter<'font> {
    width: u32,
    height: u32,
    layers: Vec<PainterLayer>,
    /// Accumulated intersection masks; the last element is the most restrictive active clip.
    clip_stack: Vec<Mask>,
    /// Cumulative transforms; element 0 is the initial font-units to pixels transform.
    transform_stack: Vec<Transform>,
    outlines: OutlineGlyphCollection<'font>,
    cpal_colors: Vec<[u8; 4]>,
    foreground: [u8; 4],
}

impl<'font> TinySkiaPainter<'font> {
    fn new(
        width: u32,
        height: u32,
        initial_transform: Transform,
        outlines: OutlineGlyphCollection<'font>,
        cpal_colors: Vec<[u8; 4]>,
        foreground: [u8; 4],
    ) -> Option<Self> {
        let pixmap = Pixmap::new(width, height)?;
        Some(Self {
            width,
            height,
            layers: vec![PainterLayer {
                pixmap,
                blend_mode: BlendMode::SourceOver,
            }],
            clip_stack: Vec::new(),
            transform_stack: vec![initial_transform],
            outlines,
            cpal_colors,
            foreground,
        })
    }

    fn current_transform(&self) -> Transform {
        *self.transform_stack.last().unwrap()
    }

    fn current_clip(&self) -> Option<&Mask> {
        self.clip_stack.last()
    }

    fn top_pixmap(&mut self) -> &mut Pixmap {
        &mut self.layers.last_mut().unwrap().pixmap
    }

    /// Resolves a palette color plus a multiplier alpha into premultiplied-ready RGBA8.
    fn resolve_color(&self, palette_index: u16, alpha: f32) -> [u8; 4] {
        let base = if palette_index == 0xFFFF {
            self.foreground
        } else {
            self.cpal_colors
                .get(palette_index as usize)
                .copied()
                .unwrap_or(self.foreground)
        };
        let a = (base[3] as f32 * alpha.clamp(0.0, 1.0)).round() as u8;
        [base[0], base[1], base[2], a]
    }

    fn to_tiny_skia_color(rgba: [u8; 4]) -> Color {
        Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3])
    }

    /// Builds the gradient color stops in tiny-skia form, resolving palette indices to colors.
    fn build_stops(&self, stops: &[SkrifaColorStop]) -> Vec<GradientStop> {
        stops
            .iter()
            .map(|s| {
                let rgba = self.resolve_color(s.palette_index, s.alpha);
                GradientStop::new(s.offset, Self::to_tiny_skia_color(rgba))
            })
            .collect()
    }

    /// Pushes a new accumulated clip mask formed by intersecting `mask` with the current clip.
    fn push_intersected_clip(&mut self, mut mask: Mask) {
        if let Some(prev) = self.clip_stack.last() {
            let prev_data = prev.data();
            let new_data = mask.data_mut();
            for (n, &p) in new_data.iter_mut().zip(prev_data.iter()) {
                *n = ((*n as u16 * p as u16) / 255) as u8;
            }
        }
        self.clip_stack.push(mask);
    }

    /// Rasterizes a path into a fresh full-size mask, then intersects it onto the clip stack.
    fn clip_to_path(&mut self, path: &Path, transform: Transform) {
        let mut mask = match Mask::new(self.width, self.height) {
            Some(m) => m,
            None => return,
        };
        mask.fill_path(path, FillRule::Winding, true, transform);
        self.push_intersected_clip(mask);
    }
}

impl ColorPainter for TinySkiaPainter<'_> {
    fn push_transform(&mut self, transform: SkrifaTransform) {
        let current = self.current_transform();
        // skrifa transforms are relative to the current cumulative transform, so pre-concatenate.
        let next = current.pre_concat(skrifa_to_tiny_skia_transform(transform));
        self.transform_stack.push(next);
    }

    fn pop_transform(&mut self) {
        if self.transform_stack.len() > 1 {
            self.transform_stack.pop();
        }
    }

    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        let transform = self.current_transform();
        let outline = match self.outlines.get(glyph_id) {
            Some(o) => o,
            None => {
                // No outline: clip to nothing so subsequent fills draw nothing.
                if let Some(mask) = Mask::new(self.width, self.height) {
                    self.push_intersected_clip(mask);
                }
                return;
            }
        };
        let mut pen = PathBuilderPen(None);
        let settings =
            DrawSettings::unhinted(skrifa::instance::Size::unscaled(), LocationRef::default());
        if outline.draw(settings, &mut pen).is_err() {
            if let Some(mask) = Mask::new(self.width, self.height) {
                self.push_intersected_clip(mask);
            }
            return;
        }
        match pen.0.and_then(|pb| pb.finish()) {
            Some(path) => self.clip_to_path(&path, transform),
            None => {
                if let Some(mask) = Mask::new(self.width, self.height) {
                    self.push_intersected_clip(mask);
                }
            }
        }
    }

    fn push_clip_box(&mut self, clip_box: BoundingBox<f32>) {
        let transform = self.current_transform();
        let mut pb = PathBuilder::new();
        pb.push_rect(
            tiny_skia::Rect::from_ltrb(
                clip_box.x_min,
                clip_box.y_min,
                clip_box.x_max,
                clip_box.y_max,
            )
            .unwrap_or_else(|| tiny_skia::Rect::from_xywh(0.0, 0.0, 0.0, 0.0).unwrap()),
        );
        if let Some(path) = pb.finish() {
            self.clip_to_path(&path, transform);
        } else if let Some(mask) = Mask::new(self.width, self.height) {
            self.push_intersected_clip(mask);
        }
    }

    fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    fn fill(&mut self, brush: SkrifaBrush<'_>) {
        let transform = self.current_transform();
        let width = self.width;
        let height = self.height;
        // The clip restricts where the fill lands; without one the whole layer is painted.
        let clip = self.current_clip().cloned();

        let mut paint = tiny_skia::Paint::default();
        paint.anti_alias = true;

        match brush {
            SkrifaBrush::Solid {
                palette_index,
                alpha,
            } => {
                let rgba = self.resolve_color(palette_index, alpha);
                paint.shader = Shader::SolidColor(Self::to_tiny_skia_color(rgba));
            }
            SkrifaBrush::LinearGradient {
                p0,
                p1,
                color_stops,
                extend,
            } => {
                let stops = self.build_stops(color_stops);
                let (x0, y0) = map_point(&transform, p0.x, p0.y);
                let (x1, y1) = map_point(&transform, p1.x, p1.y);
                if let Some(shader) = LinearGradient::new(
                    TsPoint::from_xy(x0, y0),
                    TsPoint::from_xy(x1, y1),
                    stops,
                    extend_to_spread(extend),
                    Transform::identity(),
                ) {
                    paint.shader = shader;
                } else if let Some(first) = color_stops.first() {
                    let rgba = self.resolve_color(first.palette_index, first.alpha);
                    paint.shader = Shader::SolidColor(Self::to_tiny_skia_color(rgba));
                } else {
                    return;
                }
            }
            SkrifaBrush::RadialGradient {
                c0,
                r0,
                c1,
                r1,
                color_stops,
                extend,
            } => {
                let stops = self.build_stops(color_stops);
                // tiny-skia supports the COLR two-circle radial model directly.
                let (x0, y0) = map_point(&transform, c0.x, c0.y);
                let (x1, y1) = map_point(&transform, c1.x, c1.y);
                // Approximated via the transform's average axis scale.
                let sx = (transform.sx * transform.sx + transform.ky * transform.ky).sqrt();
                let sy = (transform.kx * transform.kx + transform.sy * transform.sy).sqrt();
                let radius_scale = (sx + sy) * 0.5;
                let start_radius = r0 * radius_scale;
                let end_radius = r1 * radius_scale;
                let shader = RadialGradient::new(
                    TsPoint::from_xy(x0, y0),
                    start_radius,
                    TsPoint::from_xy(x1, y1),
                    end_radius,
                    stops,
                    extend_to_spread(extend),
                    Transform::identity(),
                );
                if let Some(shader) = shader {
                    paint.shader = shader;
                } else if let Some(first) = color_stops.first() {
                    let rgba = self.resolve_color(first.palette_index, first.alpha);
                    paint.shader = Shader::SolidColor(Self::to_tiny_skia_color(rgba));
                } else {
                    return;
                }
            }
            SkrifaBrush::SweepGradient { color_stops, .. } => {
                // tiny-skia has no sweep gradient, so approximate with the first stop's solid colour.
                if let Some(first) = color_stops.first() {
                    let rgba = self.resolve_color(first.palette_index, first.alpha);
                    paint.shader = Shader::SolidColor(Self::to_tiny_skia_color(rgba));
                } else {
                    return;
                }
            }
        }

        if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, width as f32, height as f32) {
            let pixmap = self.top_pixmap();
            pixmap.fill_rect(rect, &paint, Transform::identity(), clip.as_ref());
        }
    }

    fn push_layer(&mut self, composite_mode: CompositeMode) {
        let pixmap = match Pixmap::new(self.width, self.height) {
            Some(p) => p,
            None => return,
        };
        self.layers.push(PainterLayer {
            pixmap,
            blend_mode: composite_to_blend(composite_mode),
        });
    }

    fn pop_layer(&mut self) {
        self.pop_layer_impl(None);
    }

    fn pop_layer_with_mode(&mut self, mode: CompositeMode) {
        self.pop_layer_impl(Some(composite_to_blend(mode)));
    }
}

impl TinySkiaPainter<'_> {
    fn pop_layer_impl(&mut self, override_mode: Option<BlendMode>) {
        if self.layers.len() <= 1 {
            return;
        }
        let layer = self.layers.pop().unwrap();
        let mut blend_mode = override_mode.unwrap_or(layer.blend_mode);
        // Source clears the destination entirely, which is almost never the intent for an overlay.
        if blend_mode == BlendMode::Source {
            blend_mode = BlendMode::SourceOver;
        }
        let dest = self.top_pixmap();
        dest.draw_pixmap(
            0,
            0,
            layer.pixmap.as_ref(),
            &tiny_skia::PixmapPaint {
                blend_mode,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

/// Rasterizes a single COLR v1 color glyph at `physical_font_size` pixels. Returns straight-alpha RGBA8 plus swash-style placement, or `None` if the glyph has no COLR v1 record or the font is unusable. `foreground` resolves COLR paints that reference the text (foreground) palette index.
pub fn rasterize_colr_glyph(
    font_bytes: &[u8],
    face_index: u32,
    glyph_id: u16,
    physical_font_size: f32,
    foreground: [u8; 4],
) -> Option<ColrGlyphBitmap> {
    if physical_font_size <= 0.0 {
        return None;
    }
    let font_ref = FontRef::from_index(font_bytes, face_index).ok()?;
    let upem = font_ref.head().map(|h| h.units_per_em()).unwrap_or(1000) as f32;
    if upem <= 0.0 {
        return None;
    }

    let color_glyph = font_ref
        .color_glyphs()
        .get_with_format(GlyphId::new(glyph_id as u32), ColorGlyphFormat::ColrV1)?;

    let scale = physical_font_size / upem;

    // Font-level glyph bounds, so glyphs with large descenders or horizontal overhangs are never clipped.
    let font_metrics = SkrifaMetrics::new(
        &font_ref,
        SkrifaSize::new(physical_font_size),
        LocationRef::default(),
    );
    let (ascent_px, descent_depth_px, width_px) = match font_metrics.bounds {
        Some(b) => (b.y_max, (-b.y_min).max(0.0), b.x_max),
        None => (
            font_metrics.ascent,
            (-font_metrics.descent).max(0.0),
            physical_font_size,
        ),
    };
    // +2 guards against 1-pixel AA overshoot on each edge.
    let baseline_in_pixmap = ascent_px.ceil() as u32 + 2;
    let dim_h = (baseline_in_pixmap + descent_depth_px.ceil() as u32 + 2).max(1);
    let dim_w = (width_px.ceil() as u32 + 2).max(1);

    // Font units to pixels with a Y-flip; the baseline sits at `baseline_in_pixmap`.
    let initial = Transform::from_row(scale, 0.0, 0.0, -scale, 0.0, baseline_in_pixmap as f32);

    let outlines = font_ref.outline_glyphs();
    let cpal_colors = load_cpal_colors(&font_ref);
    let mut painter =
        TinySkiaPainter::new(dim_w, dim_h, initial, outlines, cpal_colors, foreground)?;

    color_glyph
        .paint(LocationRef::default(), &mut painter)
        .ok()?;

    let pixmap = painter.layers.into_iter().next().unwrap().pixmap;

    // tiny-skia pixels are premultiplied; the glyph atlas expects straight alpha.
    let mut rgba = Vec::with_capacity((dim_w * dim_h * 4) as usize);
    for px in pixmap.pixels() {
        let c = px.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }

    Some(ColrGlyphBitmap {
        rgba,
        width: dim_w,
        height: dim_h,
        placement_left: 0,
        placement_top: baseline_in_pixmap as i32,
    })
}

/// Loads the font's first CPAL palette into an index to RGBA8 lookup table.
fn load_cpal_colors(font_ref: &FontRef<'_>) -> Vec<[u8; 4]> {
    let Some(cpal) = font_ref.cpal().ok() else {
        return Vec::new();
    };
    let Some(records) = cpal.color_records_array().and_then(|r| r.ok()) else {
        return Vec::new();
    };
    records
        .iter()
        .map(|rec| [rec.red, rec.green, rec.blue, rec.alpha])
        .collect()
}
