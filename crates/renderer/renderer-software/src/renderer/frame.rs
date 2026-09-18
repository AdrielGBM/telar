//! One frame, in three phases: plan the damage, clear what it covers, then replay the commands into it.

use std::iter;
use std::num::NonZeroU32;

use geometry_core::Rect;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::perf::{self, Phase};
use renderer_core::{
    BorderRadius, Color, DrawCommand, RenderBackend, RendererError, expand_fill_layers,
};
use smallvec::SmallVec;
use tiny_skia::{Mask, Pixmap};

use super::SoftwareRenderer;
use super::clip::{ClipMask, ClipShape};
use super::pixels::{
    LayerBox, apply_scroll_blit, clamp_to_pixels, compute_layer_bounds, cull_bounds, fill_region,
    hash_commands_with_dimensions,
};
use super::present::FrameOp;

pub(super) type Regions = SmallVec<[Rect; 8]>;

enum FrameAction {
    Present(FrameOp),
    Render(FramePlan),
}

// `damage` holds the whole-pixel regions to clear and redraw, `None` meaning the whole frame.
struct FramePlan {
    frame_op: FrameOp,
    damage: Option<Regions>,
    input_hash: u64,
}

pub(super) struct Layer {
    pixmap: Pixmap,
    opacity: f32,
    origin: (i32, i32),
    // The clips already open when the layer was pushed mask its composite, so only those opened inside it mask its content.
    clip_depth: usize,
    mask: Option<ClipMask>,
}

struct Canvas<'a> {
    pixmap: &'a mut Pixmap,
    mask: Option<&'a Mask>,
    origin: (i32, i32),
    transform: tiny_skia::Transform,
    clip: Option<Rect>,
    blur_scratch: &'a mut Vec<u8>,
}

fn on_pixels(rects: impl IntoIterator<Item = Rect>, width: u32, height: u32) -> Regions {
    rects
        .into_iter()
        .filter_map(|rect| clamp_to_pixels(rect, width, height))
        .map(|(x0, y0, x1, y1)| Rect::new(x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32))
        .collect()
}

fn repaints(damage: Option<&[Rect]>, rect: Rect) -> bool {
    damage.is_none_or(|regions| regions.iter().any(|region| region.overlaps(rect)))
}

fn take_mask(pool: &mut Vec<ClipMask>, width: u32, height: u32) -> Option<ClipMask> {
    pool.pop()
        .filter(|mask| mask.fits(width, height))
        .or_else(|| ClipMask::new(width, height))
}

impl<D, W> SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    fn plan_frame(&mut self, commands: &[DrawCommand], clear_color: Option<Color>) -> FrameAction {
        let shadows = self.poll_pending_shadows();
        let clear_color_changed = clear_color != self.prev_clear_color;
        if shadows.is_empty() && !clear_color_changed && commands == self.prev_commands.as_slice() {
            return FrameAction::Present(FrameOp::NoChange);
        }

        let change = (!self.prev_commands.is_empty()).then(|| {
            let fm = &self.font_metrics;
            self.frame_diff
                .compare(commands, &self.prev_commands, |c, m| {
                    renderer_core::culling::command_visual_rect(c, m, fm)
                })
        });
        let (damage, maybe_scroll) = change.map_or((None, None), |c| (c.damage, c.scroll));

        let input_hash = renderer_core::hash_draw_commands(commands);
        if input_hash != self.prev_commands_hash {
            self.prev_commands.clear();
            self.prev_commands.extend(commands.iter().cloned());
            self.prev_commands_hash = input_hash;
        }
        self.prev_clear_color = clear_color;

        let full = FrameAction::Render(FramePlan {
            frame_op: FrameOp::Full,
            damage: None,
            input_hash,
        });
        // Every pixel is cleared to it, so this is the one change no region can bound.
        if clear_color_changed {
            return full;
        }
        let (width, height) = (self.width, self.height);
        // A shadow that finished blurring changes the frame where its command paints, and the commands around it did not change, so its footprint is the only thing pointing at those pixels.
        let arrived = || shadows.iter().copied();
        let (damage, changed) = match (maybe_scroll, damage) {
            (Some(scroll), _) => {
                if let Some(pixmap) = &mut self.pixmap {
                    apply_scroll_blit(
                        pixmap,
                        scroll.scroll_clip,
                        scroll.delta_x as f32,
                        scroll.delta_y as f32,
                    );
                }
                let extra = scroll.extra_dirty.iter().copied();
                (
                    on_pixels(
                        iter::once(scroll.exposed_band)
                            .chain(extra.clone())
                            .chain(arrived()),
                        width,
                        height,
                    ),
                    on_pixels(
                        iter::once(scroll.scroll_clip).chain(extra).chain(arrived()),
                        width,
                        height,
                    ),
                )
            }
            (None, Some(rects)) => {
                let damage = on_pixels(rects.into_iter().chain(arrived()), width, height);
                (damage.clone(), damage)
            }
            // A change the diff cannot bound, whatever else arrived inside it.
            (None, None) => return full,
        };
        if changed.is_empty() {
            return FrameAction::Present(FrameOp::NoChange);
        }
        FrameAction::Render(FramePlan {
            frame_op: FrameOp::Regions(changed),
            damage: Some(damage),
            input_hash,
        })
    }

    pub(super) fn render(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> FrameOp {
        let plan_start = perf::now_if_enabled();
        let action = self.plan_frame(commands, clear_color);
        perf::record_since(Phase::Plan, plan_start);
        let FramePlan {
            frame_op,
            damage,
            input_hash,
        } = match action {
            FrameAction::Present(op) => return op,
            FrameAction::Render(plan) => plan,
        };

        let interpret_start = perf::now_if_enabled();
        self.clear(clear_color, damage.as_deref());
        self.draw_state.reset();
        self.clip_shapes.clear();
        self.layer_stack.clear();

        match &self.expanded_commands_cache {
            Some((cached_hash, _)) if *cached_hash == input_hash => {}
            _ => {
                let stored = expand_fill_layers(commands).unwrap_or_else(|| commands.to_vec());
                self.expanded_commands_cache = Some((input_hash, stored));
            }
        };

        let layer_boxes = {
            let commands: &[DrawCommand] = &self.expanded_commands_cache.as_ref().unwrap().1;
            let bbox_hash = hash_commands_with_dimensions(commands, self.width, self.height);
            match &self.layer_bounds_cache {
                Some((cached_hash, cached)) if *cached_hash == bbox_hash => cached.clone(),
                _ => {
                    let result =
                        compute_layer_bounds(commands, self.width, self.height, &self.font_metrics);
                    self.layer_bounds_cache = Some((bbox_hash, result.clone()));
                    result
                }
            }
        };

        // The command loop needs `&mut self` but the expanded list lives inside it, so it is moved out for the duration and restored after, preserving the expand cache exactly.
        let taken = std::mem::take(&mut self.expanded_commands_cache);
        self.run_commands(&taken.as_ref().unwrap().1, damage.as_deref(), &layer_boxes);
        self.expanded_commands_cache = taken;
        perf::record_since(Phase::Interpret, interpret_start);
        frame_op
    }

    fn clear(&mut self, clear_color: Option<Color>, damage: Option<&[Rect]>) {
        let Some(pixmap) = &mut self.pixmap else {
            return;
        };
        let color = clear_color
            .map(crate::primitives::to_skia_color)
            .unwrap_or(tiny_skia::Color::TRANSPARENT);
        match damage {
            None => pixmap.fill(color),
            Some(regions) => {
                let pixel = color.premultiply().to_color_u8();
                for region in regions {
                    fill_region(pixmap, *region, pixel);
                }
            }
        }
    }

    fn canvas(&mut self, damage: Option<&[Rect]>) -> Option<Canvas<'_>> {
        let [a, b, c, d, e, f] = self.draw_state.cumulative_matrix;
        let clip = self.draw_state.current_clip();
        // The innermost clip's rect comes from `draw_state`, already intersected with every clip around it; its radius is its own.
        let shape = clip
            .zip(self.clip_shapes.last().map(|innermost| innermost.radius))
            .map(|(rect, radius)| ClipShape { rect, radius });
        let open_clips = self.clip_shapes.len();
        let (pixmap, mask, origin) = match self.layer_stack.last_mut() {
            Some(Layer {
                pixmap,
                origin,
                clip_depth,
                mask,
                ..
            }) => {
                let mask = match shape {
                    Some(shape) if open_clips > *clip_depth => {
                        if mask.is_none() {
                            *mask = take_mask(&mut self.mask_pool, pixmap.width(), pixmap.height());
                        }
                        // The clips the layer was pushed under mask its composite instead, so they are the parent canvas's to apply.
                        let ancestors = &self.clip_shapes[*clip_depth..open_clips - 1];
                        mask.as_mut()
                            .map(|mask| mask.show(shape, ancestors, *origin, None))
                    }
                    _ => None,
                };
                (pixmap, mask, *origin)
            }
            None => {
                let surface = ClipShape {
                    rect: Rect::new(0.0, 0.0, self.width as f32, self.height as f32),
                    radius: BorderRadius::default(),
                };
                let target = match shape {
                    Some(_) => &mut self.clip_mask,
                    None => &mut self.damage_mask,
                };
                let ancestors = &self.clip_shapes[..open_clips.saturating_sub(1)];
                let mask = target
                    .as_mut()
                    .map(|mask| mask.show(shape.unwrap_or(surface), ancestors, (0, 0), damage));
                (self.pixmap.as_mut()?, mask, (0, 0))
            }
        };
        let (ox, oy) = (origin.0 as f32, origin.1 as f32);
        Some(Canvas {
            pixmap,
            mask,
            origin,
            transform: tiny_skia::Transform::from_row(a, b, c, d, e - ox, f - oy),
            clip: clip.map(|clip| Rect::new(clip.x - ox, clip.y - oy, clip.width, clip.height)),
            blur_scratch: &mut self.blur_scratch,
        })
    }

    fn open_layer(
        &mut self,
        (x, y, width, height): LayerBox,
        opacity: f32,
        backdrop_blur: f32,
    ) -> Option<Layer> {
        let mut pixmap = self
            .pixmap_pool
            .pop()
            .filter(|p| p.width() == width && p.height() == height)
            .or_else(|| Pixmap::new(width, height))?;
        pixmap.fill(tiny_skia::Color::TRANSPARENT);
        if backdrop_blur > 0.0 {
            let (parent, (parent_x, parent_y)) = match self.layer_stack.last() {
                Some(parent) => (&parent.pixmap, parent.origin),
                None => (self.pixmap.as_ref()?, (0, 0)),
            };
            pixmap.draw_pixmap(
                parent_x - x,
                parent_y - y,
                parent.as_ref(),
                &tiny_skia::PixmapPaint {
                    opacity: 1.0,
                    blend_mode: tiny_skia::BlendMode::Source,
                    quality: tiny_skia::FilterQuality::Nearest,
                },
                tiny_skia::Transform::identity(),
                None,
            );
            // A radius, as `Shadow::blur_radius` is and as the margin the dirty tracker reserves around this layer already assumes, both of them measured through this same conversion. Handed straight over as the deviation, one number meant two different blurs.
            crate::primitives::gaussian_blur(
                pixmap.data_mut(),
                width,
                height,
                renderer_core::blur_sigma(backdrop_blur),
                &mut self.blur_scratch,
            );
        }
        Some(Layer {
            pixmap,
            opacity,
            origin: (x, y),
            clip_depth: self.clip_shapes.len(),
            mask: None,
        })
    }

    fn run_commands(
        &mut self,
        commands: &[DrawCommand],
        damage: Option<&[Rect]>,
        layer_boxes: &[Option<LayerBox>],
    ) {
        let mut skipped_layers: usize = 0;

        for (index, cmd) in commands.iter().enumerate() {
            if skipped_layers > 0 {
                match cmd {
                    DrawCommand::PushLayer { .. } => skipped_layers += 1,
                    DrawCommand::PopLayer => skipped_layers -= 1,
                    _ => {}
                }
                continue;
            }

            if let DrawCommand::Rect { rect, style } = cmd
                && (rect.width <= 0.0
                    || rect.height <= 0.0
                    || (style.fill.is_none() && style.painted_border().is_none()))
            {
                continue;
            }

            let painted = renderer_core::culling::command_visual_rect(
                cmd,
                self.draw_state.cumulative_matrix,
                &self.font_metrics,
            );
            if let Some(painted) = painted {
                // A layer's pixmap starts empty, so everything in it is drawn whenever any of it is.
                let repainted = !self.layer_stack.is_empty() || repaints(damage, painted);
                if !repainted || cull_bounds(painted, self.draw_state.current_clip()) {
                    continue;
                }
            }
            // Only the commands that paint nothing have no footprint, and none of those casts a shadow to remember one for.
            let painted = painted.unwrap_or_default();

            match cmd {
                DrawCommand::Rect { rect, style } => {
                    let Some(canvas) = self.canvas(damage) else {
                        break;
                    };
                    crate::caches::with_caches(|c| {
                        crate::primitives::rect::draw_rect(
                            canvas.pixmap,
                            *rect,
                            painted,
                            style,
                            canvas.transform,
                            canvas.mask,
                            &mut c.shadow_cache,
                            &mut c.pending_shadows,
                            &mut c.recent_shadow,
                            canvas.blur_scratch,
                        );
                    });
                }
                DrawCommand::Text {
                    text,
                    spans,
                    rect,
                    style,
                } => {
                    let Some(canvas) = self.canvas(damage) else {
                        break;
                    };
                    crate::caches::with_caches(|c| {
                        crate::primitives::text::draw_text(
                            canvas.pixmap,
                            &mut c.text_shaper,
                            text,
                            spans.as_deref(),
                            *rect,
                            painted,
                            style,
                            canvas.transform,
                            canvas.mask,
                            canvas.clip,
                            canvas.blur_scratch,
                            &mut c.text_shadow_cache,
                            &mut c.pending_text_shadows,
                            &mut c.recent_text_shadow,
                        );
                    });
                }
                DrawCommand::Image { data, rect, raster } => {
                    let Some(canvas) = self.canvas(damage) else {
                        break;
                    };
                    crate::primitives::image::draw_image(
                        canvas.pixmap,
                        data,
                        *rect,
                        *raster,
                        canvas.transform,
                        canvas.mask,
                    );
                }
                DrawCommand::Line { p1, p2, style } => {
                    let Some(canvas) = self.canvas(damage) else {
                        break;
                    };
                    crate::primitives::line::draw_line(
                        canvas.pixmap,
                        *p1,
                        *p2,
                        *style,
                        canvas.transform,
                        canvas.mask,
                        canvas.clip,
                    );
                }
                DrawCommand::Path { data, style } => {
                    let Some(canvas) = self.canvas(damage) else {
                        break;
                    };
                    crate::caches::with_caches(|c| {
                        crate::primitives::path::draw_path(
                            canvas.pixmap,
                            data,
                            painted,
                            style,
                            canvas.transform,
                            canvas.mask,
                            canvas.clip,
                            canvas.blur_scratch,
                            &mut c.path_shadow_cache,
                            &mut c.pending_path_shadows,
                            &mut c.recent_path_shadow,
                        );
                    });
                }
                DrawCommand::PushClip { rect, radius } => {
                    let rect = renderer_core::transform_clip_rect(
                        self.draw_state.cumulative_matrix,
                        *rect,
                    );
                    self.draw_state.push_clip(rect);
                    // Its own rect rather than the one its parents cut it down to: that is where its corners are, and corners moved inwards would cut what the clip covers.
                    self.clip_shapes.push(ClipShape {
                        rect,
                        radius: *radius,
                    });
                }
                DrawCommand::PopClip => {
                    self.draw_state.pop_clip();
                    self.clip_shapes.pop();
                }
                DrawCommand::PushMatrix { matrix } => self.draw_state.push_matrix(*matrix),
                DrawCommand::PopMatrix => self.draw_state.pop_matrix(),
                DrawCommand::PushLayer {
                    opacity,
                    backdrop_blur,
                } => {
                    let opened = layer_boxes[index]
                        .filter(|&(x, y, width, height)| {
                            !self.layer_stack.is_empty()
                                || repaints(
                                    damage,
                                    Rect::new(x as f32, y as f32, width as f32, height as f32),
                                )
                        })
                        .and_then(|layer_box| self.open_layer(layer_box, *opacity, *backdrop_blur));
                    match opened {
                        Some(layer) => self.layer_stack.push(layer),
                        None => skipped_layers = 1,
                    }
                }
                DrawCommand::PopLayer => {
                    let Some(layer) = self.layer_stack.pop() else {
                        continue;
                    };
                    if let Some(canvas) = self.canvas(damage) {
                        canvas.pixmap.draw_pixmap(
                            layer.origin.0 - canvas.origin.0,
                            layer.origin.1 - canvas.origin.1,
                            layer.pixmap.as_ref(),
                            &tiny_skia::PixmapPaint {
                                opacity: layer.opacity,
                                blend_mode: tiny_skia::BlendMode::SourceOver,
                                quality: tiny_skia::FilterQuality::Nearest,
                            },
                            tiny_skia::Transform::identity(),
                            canvas.mask,
                        );
                    }
                    self.pixmap_pool.push(layer.pixmap);
                    self.mask_pool.extend(layer.mask);
                }
                // Structure, for a backend whose output is a document. Every command inside carries the position it was laid out at, so skipping the markers draws the same frame.
                DrawCommand::PushElement { .. } | DrawCommand::PopElement => {}
            }
        }
    }
}

impl<D, W> RenderBackend for SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    // Built up front on the thread that will draw, so the first frame does not pay for loading fonts mid-frame.
    fn bind_to_render_thread(&mut self) {
        self.ensure_caches();
    }

    // Matches the horizon its own entries are bound by, so a sweep any earlier would evict nothing.
    fn idle_sweep_after(&self) -> Option<std::time::Duration> {
        Some(renderer_cache::limits::CPU_IDLE)
    }

    fn sweep_idle_caches(&mut self) {
        crate::caches::sweep_idle();
    }

    fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        _scale_factor: f32,
        _generation: u64,
    ) -> Result<(), RendererError> {
        // Draw commands arrive pre-scaled, so the software backend tracks neither.
        self.ensure_caches();

        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
            self.pixmap = Pixmap::new(width, height);
            self.clip_mask = ClipMask::new(width, height);
            self.damage_mask = ClipMask::new(width, height);
            self.pixmap_pool.clear();
            self.mask_pool.clear();
            self.prev_commands.clear();
            self.prev_commands_hash = 0;
            self.prev_clear_color = None;
            self.expanded_commands_cache = None;
            self.layer_bounds_cache = None;
            // The logged regions were measured at the old size, so the next present refreshes and declares everything.
            self.present_log.reset();
            // Headless has no surface to resize; the pixmap above is the only target.
            if let (Some(w), Some(h), Some(surface)) = (
                NonZeroU32::new(width),
                NonZeroU32::new(height),
                self.surface.as_mut(),
            ) {
                surface
                    .resize(w, h)
                    .map_err(|e| RendererError::Resize(e.to_string()))?;
            }
        }
        crate::caches::publish_stats();
        Ok(())
    }

    // Only the headless renderer keeps a CPU-side pixmap to hand back; the windowed path presents to its softbuffer surface and holds none.
    fn read_rgba(&self) -> Option<Vec<u8>> {
        self.pixmap.as_ref().map(|p| p.data().to_vec())
    }

    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<(), RendererError> {
        let _frame_span = perf::span(Phase::Frame);
        let frame_op = self.render(commands, clear_color);
        self.present_pixmap(frame_op)
    }
}
