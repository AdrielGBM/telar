//! One frame, in three phases: plan the damage, clear what it covers, then replay the commands into it.

use std::num::NonZeroU32;

use geometry_core::Rect;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, DrawCommand, RenderBackend, RendererError, expand_fill_layers};
use smallvec::SmallVec;
use tiny_skia::Pixmap;

use super::SoftwareRenderer;
use super::pixels::{
    apply_scroll_blit, clamp_to_pixels, compute_layer_bounds, cull_bounds, fill_mask_region,
    fill_rounded_mask, hash_commands_with_dimensions, repaint_mask,
};
use super::present::FrameOp;

// Either the frame can be presented immediately, nothing visible having changed, or it must be cleared and re-rendered with the computed plan.
enum FrameAction {
    Present(FrameOp),
    Render(FramePlan),
}

// How to classify the present, which on-screen regions to clear and render (`None` is the full frame), and the command hash keying the expand cache.
struct FramePlan {
    frame_op: FrameOp,
    skip_rect: Option<SmallVec<[Rect; 8]>>,
    input_hash: u64,
}

impl<D, W> SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    // Fast-path detection, dirty-rect computation, present classification and `skip_rect` expansion. Returns `Present` for the early-outs that only re-present the existing pixmap.
    fn plan_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<FrameAction, RendererError> {
        // Returns true if any completed, in which case the frame must re-render even on an unchanged command list so the newly available shadow gets drawn.
        let shadow_arrived = self.poll_pending_shadows();

        // Nothing changed, so re-present the existing pixmap. A shadow that just finished forces a redraw.
        if !shadow_arrived
            && commands == self.prev_commands.as_slice()
            && clear_color == self.prev_clear_color
        {
            return Ok(FrameAction::Present(FrameOp::NoChange));
        }

        // When the only change is a single transform y-shift, shift the existing rows in place and re-render only the exposed band plus any out-of-clip overlays that changed.
        let maybe_scroll = if !self.prev_commands.is_empty() {
            renderer_core::dirty::detect_scroll_blit(commands, &self.prev_commands)
        } else {
            None
        };
        if let Some(ref sb) = maybe_scroll {
            if let Some(pixmap) = &mut self.pixmap {
                apply_scroll_blit(pixmap, sb.scroll_clip, sb.delta_x as f32, sb.delta_y as f32);
            }
        }

        // Disjoint changes are kept as separate rects rather than a viewport-spanning union, so the untouched centre can be skipped.
        let dirty_rect: Option<SmallVec<[Rect; 8]>> = if let Some(ref sb) = maybe_scroll {
            // Scroll blit: only the newly exposed band and any changed overlays.
            let mut v: SmallVec<[Rect; 8]> = SmallVec::new();
            v.push(sb.exposed_band);
            v.extend(sb.extra_dirty.iter().copied());
            Some(v)
        } else if self.prev_commands.is_empty() {
            None // first frame → full clear
        } else {
            renderer_core::dirty::compute_dirty_rect(commands, &self.prev_commands, |cmd, m| {
                renderer_core::culling::command_visual_rect(cmd, m, &self.font_metrics)
            })
        };

        let clear_color_changed = clear_color != self.prev_clear_color;

        // A bounded set of changed regions can refresh an aged buffer incrementally; a clear-colour or unbounded change re-swizzles fully. Built from the raw, un-expanded regions, which are exactly the changed pixels.
        let frame_op = if clear_color_changed {
            FrameOp::Full
        } else if let Some(ref sb) = maybe_scroll {
            let mut regions: SmallVec<[Rect; 8]> = SmallVec::new();
            regions.push(sb.scroll_clip);
            regions.extend(sb.extra_dirty.iter().copied());
            FrameOp::Regions(regions)
        } else {
            match &dirty_rect {
                Some(drs) if !drs.is_empty() => FrameOp::Regions(drs.clone()),
                _ => FrameOp::Full,
            }
        };

        let current_hash = renderer_core::hash_draw_commands(commands);
        if current_hash != self.prev_commands_hash {
            self.prev_commands.clear();
            self.prev_commands.extend(commands.iter().cloned());
            self.prev_commands_hash = current_hash;
        }
        self.prev_clear_color = clear_color;

        // Both the tiny-skia clear rect and the geometry rect used for command-skipping come from the same clamped bounds: the naive `(dr.x - 1).max(0)` formula shifts the rect right and down for off-screen content, so the clear would wipe a larger area than `dr` describes and the commands over it would be skipped.
        let skip_rect: Option<SmallVec<[Rect; 8]>> = match dirty_rect {
            Some(drs) if !drs.is_empty() => {
                // Precomputed once, so expanding every dirty region is O(rects + commands) rather than O(rects * commands).
                let mut visual_rects: Vec<Rect> = Vec::with_capacity(commands.len());
                renderer_core::for_each_with_matrix(commands, |cmd, matrix| {
                    if let Some(vr) =
                        renderer_core::culling::command_visual_rect(cmd, matrix, &self.font_metrics)
                    {
                        visual_rects.push(vr);
                    }
                });

                let mut out: SmallVec<[Rect; 8]> = SmallVec::new();
                for dr in drs.iter() {
                    if dr.width <= 0.0 || dr.height <= 0.0 {
                        continue;
                    }
                    let x0 = (dr.x - 1.0).max(0.0);
                    let y0 = (dr.y - 1.0).max(0.0);
                    let x1 = (dr.x + dr.width + 1.0).min(self.width as f32);
                    let y1 = (dr.y + dr.height + 1.0).min(self.height as f32);
                    if x1 <= x0 || y1 <= y0 {
                        continue;
                    }
                    // A partially overlapping command is still fully redrawn, overwriting pixels of earlier commands that fall outside the region and will not be redrawn themselves.
                    let mut sr = Rect {
                        x: x0,
                        y: y0,
                        width: x1 - x0,
                        height: y1 - y0,
                    };
                    // One pass is not enough when expansion brings new commands into range, so iterate until the region stops growing — bounded by the command count, and converging in one or two passes in practice.
                    loop {
                        let before = sr;
                        for vr in &visual_rects {
                            if vr.overlaps(sr) {
                                let nx = sr.x.min(vr.x);
                                let ny = sr.y.min(vr.y);
                                let nx2 = (sr.x + sr.width).max(vr.x + vr.width);
                                let ny2 = (sr.y + sr.height).max(vr.y + vr.height);
                                sr = Rect {
                                    x: nx,
                                    y: ny,
                                    width: nx2 - nx,
                                    height: ny2 - ny,
                                };
                            }
                        }
                        if sr == before {
                            break;
                        }
                    }
                    let fx0 = sr.x.max(0.0);
                    let fy0 = sr.y.max(0.0);
                    let fx1 = (sr.x + sr.width).min(self.width as f32);
                    let fy1 = (sr.y + sr.height).min(self.height as f32);
                    if fx1 > fx0 && fy1 > fy0 {
                        out.push(Rect {
                            x: fx0,
                            y: fy0,
                            width: fx1 - fx0,
                            height: fy1 - fy0,
                        });
                    }
                }
                if out.is_empty() {
                    // Every dirty region was off-screen, so nothing visible changed.
                    return Ok(FrameAction::Present(FrameOp::NoChange));
                }
                Some(out)
            }
            _ => None,
        };

        // The dirty rect only covers command-changed regions, leaving background areas with stale pixels.
        let skip_rect = if clear_color_changed { None } else { skip_rect };

        Ok(FrameAction::Render(FramePlan {
            frame_op,
            skip_rect,
            input_hash: current_hash,
        }))
    }

    // A transparent surface still clears its dirty regions to fully transparent rather than skipping the clear: otherwise pixels vacated by shifted content keep the previous frame and leave a ghost. The `Source` blend overwrites them instead of compositing the new frame over the stale one.
    fn clear_pixmap(
        &mut self,
        clear_color: Option<Color>,
        skip_rect: &Option<SmallVec<[Rect; 8]>>,
    ) {
        let Some(pixmap) = &mut self.pixmap else {
            return;
        };
        let color = clear_color
            .map(crate::primitives::to_skia_color)
            .unwrap_or(tiny_skia::Color::TRANSPARENT);
        if let Some(rects) = skip_rect {
            for sr in rects.iter() {
                match tiny_skia::Rect::from_xywh(sr.x, sr.y, sr.width, sr.height) {
                    Some(r) => {
                        let mut paint = tiny_skia::Paint::default();
                        paint.set_color(color);
                        paint.blend_mode = tiny_skia::BlendMode::Source;
                        pixmap.fill_rect(r, &paint, tiny_skia::Transform::identity(), None);
                    }
                    None => {
                        pixmap.fill(color);
                        break;
                    }
                }
            }
        } else {
            pixmap.fill(color);
        }
    }

    // Replays the expanded command list, honouring the dirty-region skip, the clip mask, the matrix and layer stacks, and the precomputed layer bounding boxes.
    fn run_commands(
        &mut self,
        commands: &[DrawCommand],
        skip_rect: &Option<SmallVec<[Rect; 8]>>,
        layer_bboxes: &[Option<(i32, i32, u32, u32)>],
    ) {
        // Skipped because their bbox does not overlap `skip_rect`; their pixels are already correct from the blit.
        let mut skip_layer_depth: usize = 0;

        for (cmd_idx, cmd) in commands.iter().enumerate() {
            if skip_layer_depth > 0 {
                match cmd {
                    DrawCommand::PushLayer { .. } => skip_layer_depth += 1,
                    DrawCommand::PopLayer => skip_layer_depth -= 1,
                    _ => {}
                }
                continue;
            }

            if self.pixmap.is_none() {
                break;
            }

            let inside_layer = !self.layer_stack.is_empty();
            let (layer_ox, layer_oy) = self
                .layer_stack
                .last()
                .map(|(_, _, ox, oy)| (*ox, *oy))
                .unwrap_or((0, 0));

            let [ma, mb, mc, md, me, mf] = self.draw_state.cumulative_matrix;
            let transform = tiny_skia::Transform::from_row(
                ma,
                mb,
                mc,
                md,
                me - layer_ox as f32,
                mf - layer_oy as f32,
            );

            // Ahead of the shared visual rect below, so a rect that draws nothing never pays for computing one.
            if let DrawCommand::Rect { rect, style } = cmd
                && (rect.width <= 0.0
                    || rect.height <= 0.0
                    || (style.fill.is_none() && style.painted_border().is_none()))
            {
                continue;
            }

            // One visual rect for the whole body. `None` for the state commands, which is what keeps hoisting it safe: a state command has no bounds and must never be skipped.
            if let Some(vr) = renderer_core::culling::command_visual_rect(
                cmd,
                self.draw_state.cumulative_matrix,
                &self.font_metrics,
            ) {
                // Only at the top level: a layer is a fresh isolated pixmap rendered from scratch every frame, so all its commands must run whatever region is dirty.
                if let Some(dirty_rects) = skip_rect
                    && !inside_layer
                    && dirty_rects.iter().all(|dr| !vr.overlaps(*dr))
                {
                    continue;
                }
                if cull_bounds(vr, self.draw_state.current_clip()) {
                    continue;
                }
            }

            match cmd {
                DrawCommand::Rect { rect, style } => {
                    let rect = *rect;
                    let style = **style;
                    let pixmap = if let Some((layer, _, _, _)) = self.layer_stack.last_mut() {
                        layer
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if self.draw_state.current_clip().is_some() && !inside_layer {
                        self.clip_mask_buffer.as_ref()
                    } else {
                        None
                    };
                    let blur_scratch = &mut self.blur_scratch;
                    crate::caches::with_caches(|c| {
                        crate::primitives::rect::draw_rect(
                            pixmap,
                            rect,
                            &style,
                            transform,
                            clip,
                            &mut c.shadow_cache,
                            &mut c.pending_shadows,
                            &mut c.recent_shadow,
                            blur_scratch,
                        );
                    });
                }
                DrawCommand::Text {
                    text,
                    spans,
                    rect,
                    style,
                } => {
                    let rect = *rect;
                    let style = (**style).clone();
                    let pixmap = if let Some((top, _, _, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if self.draw_state.current_clip().is_some() && !inside_layer {
                        self.clip_mask_buffer.as_ref()
                    } else {
                        None
                    };
                    let outer_clip = if inside_layer {
                        None
                    } else {
                        self.draw_state.current_clip()
                    };
                    let blur_scratch = &mut self.blur_scratch;
                    crate::caches::with_caches(|c| {
                        crate::primitives::text::draw_text(
                            pixmap,
                            &mut c.text_shaper,
                            text,
                            spans.as_deref(),
                            rect,
                            &style,
                            transform,
                            clip,
                            outer_clip,
                            blur_scratch,
                            &mut c.text_shadow_cache,
                            &mut c.pending_text_shadows,
                            &mut c.recent_text_shadow,
                        );
                    });
                }
                DrawCommand::Image { data, rect, raster } => {
                    let pixmap = if let Some((top, _, _, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if self.draw_state.current_clip().is_some() && !inside_layer {
                        self.clip_mask_buffer.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::image::draw_image(
                        pixmap, data, *rect, *raster, transform, clip,
                    );
                }
                DrawCommand::Line { p1, p2, style } => {
                    let pixmap = if let Some((top, _, _, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if self.draw_state.current_clip().is_some() && !inside_layer {
                        self.clip_mask_buffer.as_ref()
                    } else {
                        None
                    };
                    crate::primitives::line::draw_line(
                        pixmap,
                        *p1,
                        *p2,
                        *style,
                        transform,
                        clip,
                        if inside_layer {
                            None
                        } else {
                            self.draw_state.current_clip()
                        },
                    );
                }
                DrawCommand::Path { data, style } => {
                    let style = **style;
                    let pixmap = if let Some((top, _, _, _)) = self.layer_stack.last_mut() {
                        top
                    } else {
                        self.pixmap.as_mut().unwrap()
                    };
                    let clip = if self.draw_state.current_clip().is_some() && !inside_layer {
                        self.clip_mask_buffer.as_ref()
                    } else {
                        None
                    };
                    let outer_clip = if inside_layer {
                        None
                    } else {
                        self.draw_state.current_clip()
                    };
                    let blur_scratch = &mut self.blur_scratch;
                    crate::caches::with_caches(|c| {
                        crate::primitives::path::draw_path(
                            pixmap,
                            data,
                            &style,
                            transform,
                            clip,
                            outer_clip,
                            blur_scratch,
                            &mut c.path_shadow_cache,
                            &mut c.pending_path_shadows,
                            &mut c.recent_path_shadow,
                        );
                    });
                }
                DrawCommand::PushClip { rect, radius } => {
                    let prev_dirty = self.clip_mask_dirty;
                    // Clip rects arrive in the emitting widget's local space, so map through the active matrix; the mask is painted in window pixels.
                    let clip_rect = renderer_core::transform_clip_rect(
                        self.draw_state.cumulative_matrix,
                        *rect,
                    );
                    let effective = self.draw_state.push_clip(clip_rect);
                    if let Some(ref mut m) = self.clip_mask_buffer {
                        if radius.is_zero() {
                            repaint_mask(m, effective, prev_dirty, self.width, self.height);
                        } else {
                            if let Some(prev) = prev_dirty {
                                if prev != effective {
                                    if let Some(region) =
                                        clamp_to_pixels(prev, self.width, self.height)
                                    {
                                        fill_mask_region(
                                            m.data_mut(),
                                            self.width as usize,
                                            region,
                                            0,
                                        );
                                    }
                                }
                            }
                            fill_rounded_mask(m, effective, *radius);
                        }
                    }
                    self.clip_mask_dirty = Some(effective);
                }
                DrawCommand::PopClip => {
                    let prev_dirty = self.clip_mask_dirty;
                    let effective = self.draw_state.pop_clip();
                    match effective {
                        Some(r) => {
                            if let Some(ref mut m) = self.clip_mask_buffer {
                                repaint_mask(m, r, prev_dirty, self.width, self.height);
                            }
                            self.clip_mask_dirty = Some(r);
                        }
                        None => {
                            if let (Some(ref mut m), Some(prev_rect)) =
                                (self.clip_mask_buffer.as_mut(), prev_dirty)
                            {
                                if let Some(region) =
                                    clamp_to_pixels(prev_rect, self.width, self.height)
                                {
                                    fill_mask_region(m.data_mut(), self.width as usize, region, 0);
                                }
                            }
                            self.clip_mask_dirty = None;
                        }
                    }
                }
                DrawCommand::PushMatrix { matrix } => {
                    self.draw_state.push_matrix(*matrix);
                }
                DrawCommand::PopMatrix => {
                    self.draw_state.pop_matrix();
                }
                DrawCommand::PushLayer {
                    opacity,
                    backdrop_blur,
                } => {
                    // Their pixels are already correct from the blit, and re-compositing would double-apply the layer's opacity.
                    if let Some(dirty_rects) = skip_rect {
                        if !inside_layer {
                            if let Some((ox, oy, bw, bh)) = layer_bboxes[cmd_idx] {
                                let layer_rect = Rect {
                                    x: ox as f32,
                                    y: oy as f32,
                                    width: bw as f32,
                                    height: bh as f32,
                                };
                                if dirty_rects.iter().all(|dr| !layer_rect.overlaps(*dr)) {
                                    skip_layer_depth = 1;
                                    continue;
                                }
                            }
                        }
                    }
                    let (ox, oy, bw, bh) =
                        layer_bboxes[cmd_idx].unwrap_or((0, 0, self.width, self.height));
                    let layer = self
                        .pixmap_pool
                        .pop()
                        .filter(|p| p.width() == bw && p.height() == bh)
                        .or_else(|| tiny_skia::Pixmap::new(bw, bh));
                    if let Some(mut l) = layer {
                        if *backdrop_blur > 0.0 {
                            let (pox, poy) = self
                                .layer_stack
                                .last()
                                .map(|(_, _, pox, poy)| (*pox, *poy))
                                .unwrap_or((0, 0));
                            let parent = if let Some((top, _, _, _)) = self.layer_stack.last() {
                                top
                            } else {
                                self.pixmap.as_ref().unwrap()
                            };
                            l.fill(tiny_skia::Color::TRANSPARENT);
                            l.draw_pixmap(
                                pox - ox,
                                poy - oy,
                                parent.as_ref(),
                                &tiny_skia::PixmapPaint {
                                    opacity: 1.0,
                                    blend_mode: tiny_skia::BlendMode::Source,
                                    quality: tiny_skia::FilterQuality::Nearest,
                                },
                                tiny_skia::Transform::identity(),
                                None,
                            );
                            crate::primitives::gaussian_blur(
                                l.data_mut(),
                                bw,
                                bh,
                                *backdrop_blur,
                                &mut self.blur_scratch,
                            );
                        } else {
                            l.fill(tiny_skia::Color::TRANSPARENT);
                        }
                        self.layer_stack.push((l, *opacity, ox, oy));
                    }
                }
                DrawCommand::PopLayer => {
                    if let Some((layer, opacity, ox, oy)) = self.layer_stack.pop() {
                        let (parent_ox, parent_oy) = self
                            .layer_stack
                            .last()
                            .map(|(_, _, pox, poy)| (*pox, *poy))
                            .unwrap_or((0, 0));
                        let target = if let Some((top, _, _, _)) = self.layer_stack.last_mut() {
                            top
                        } else {
                            self.pixmap.as_mut().unwrap()
                        };
                        target.draw_pixmap(
                            ox - parent_ox,
                            oy - parent_oy,
                            layer.as_ref(),
                            &tiny_skia::PixmapPaint {
                                opacity,
                                blend_mode: tiny_skia::BlendMode::SourceOver,
                                quality: tiny_skia::FilterQuality::Nearest,
                            },
                            tiny_skia::Transform::identity(),
                            None,
                        );
                        self.pixmap_pool.push(layer);
                    }
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
            self.clip_mask_buffer = tiny_skia::Mask::new(width, height);
            self.clip_mask_dirty = None;
            self.pixmap_pool.clear();
            self.prev_commands.clear();
            self.prev_commands_hash = 0;
            self.prev_clear_color = None;
            self.expanded_commands_cache = None;
            self.layer_bounds_cache = None;
            // Surface buffers are recreated on resize and their age resets, so the change log would replay onto a fresh buffer.
            self.present_history.clear();
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
        let FramePlan {
            frame_op,
            skip_rect,
            input_hash,
        } = match self.plan_frame(commands, clear_color)? {
            FrameAction::Present(op) => return self.present_pixmap(op),
            FrameAction::Render(plan) => plan,
        };

        self.clear_pixmap(clear_color, &skip_rect);

        self.draw_state.reset();
        self.layer_stack.clear();

        match &self.expanded_commands_cache {
            Some((cached_hash, _)) if *cached_hash == input_hash => {}
            _ => {
                let stored = expand_fill_layers(commands).unwrap_or_else(|| commands.to_vec());
                self.expanded_commands_cache = Some((input_hash, stored));
            }
        };

        // Skipped when commands and dimensions have not changed.
        let layer_bboxes = {
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
        self.run_commands(&taken.as_ref().unwrap().1, &skip_rect, &layer_bboxes);
        self.expanded_commands_cache = taken;

        self.present_pixmap(frame_op)
    }
}
