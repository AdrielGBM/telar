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

// Outcome of the planning phase: either the frame can be presented immediately (nothing visible
// changed) or it must be cleared and re-rendered with the computed plan.
enum FrameAction {
    Present(FrameOp),
    Render(FramePlan),
}

// Everything the render phase needs from planning: how to classify the present, which on-screen
// regions to clear/render (None = full frame), and the command hash used to key the expand cache.
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
    // Planning phase: fast-path detection (skip-if-unchanged, scroll blit), dirty-rect computation,
    // present classification, prev-frame bookkeeping, and the skip_rect expansion. Returns
    // FrameAction::Present for the early-outs that only re-present the existing pixmap, or
    // FrameAction::Render carrying the plan for a full clear + command replay.
    fn plan_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<FrameAction, RendererError> {
        // Poll background shadow workers and move finished pixmaps into their caches. Returns true if any completed this frame, in which case we must re-render even if the command list is unchanged so the newly-available shadow gets drawn.
        let shadow_arrived = self.poll_pending_shadows();

        // Optimization 1: skip the entire render when nothing changed; just re-present the existing pixmap. A shadow that just finished computing forces a redraw so it can appear.
        if !shadow_arrived
            && commands == self.prev_commands.as_slice()
            && clear_color == self.prev_clear_color
        {
            return Ok(FrameAction::Present(FrameOp::NoChange));
        }

        // Optimization 2: scroll blit. When the only change is a single PushTransform ty-shift (a scroll event), shift the existing pixel rows in place and only re-render the exposed band plus any out-of-clip overlays that changed (e.g. the scrollbar).
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

        // Optimization 3: compute the on-screen regions that changed so we can clear and re-render only those. Disjoint changes (e.g. a header and a scrollbar) are kept as separate rects instead of a viewport-spanning union, so the untouched center can be skipped.
        let dirty_rect: Option<SmallVec<[Rect; 8]>> = if let Some(ref sb) = maybe_scroll {
            // Scroll blit case: only re-render the newly exposed band and any changed overlays.
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

        // Classify this frame's damage for the present buffer: a bounded set of changed regions can refresh an aged buffer incrementally; a clear-color change or unbounded change re-swizzles fully. A scroll's whole clip moved, so it counts as damage covering the clip plus the displaced overlays. Built from the raw (un-expanded) dirty regions, which are exactly the visually-changed pixels.
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

        // Clear either the dirty regions only or the full pixmap when a structural change forces a full re-render; IMPORTANT: compute both the tiny-skia clear rect and the geometry rect used for command-skipping from the same clamped bounds because the naive (dr.x-1).max(0) / dr.width+2 formula shifts the rect right/down when dr has negative coordinates (off-screen content), so fill_rect would clear a larger on-screen area than `dr` describes — causing commands outside `dr` to have their pixels cleared and then be skipped, which makes them disappear.
        let skip_rect: Option<SmallVec<[Rect; 8]>> = match dirty_rect {
            Some(drs) if !drs.is_empty() => {
                // Precompute each command's window-space visual rect once so expanding every dirty region is O(rects + commands) rather than O(rects * commands).
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
                    // Expand the region to fully contain every command it partially intersects: a partially-overlapping command is still fully redrawn, overwriting pixels of earlier commands that fall outside the region and won't be redrawn themselves.
                    let mut sr = Rect {
                        x: x0,
                        y: y0,
                        width: x1 - x0,
                        height: y1 - y0,
                    };
                    // A single pass is insufficient when expansion brings new commands into range; iterate until the region stops growing (bounded by command count in the worst case, but converges in 1-2 passes in practice).
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
                    // Every dirty region was off-screen — nothing visible changed.
                    return Ok(FrameAction::Present(FrameOp::NoChange));
                }
                Some(out)
            }
            _ => None,
        };

        // If the clear color changed, the dirty-rect only covers command-changed regions, leaving background areas untouched with stale pixels from the previous frame. Force a full clear.
        let skip_rect = if clear_color_changed { None } else { skip_rect };

        Ok(FrameAction::Render(FramePlan {
            frame_op,
            skip_rect,
            input_hash: current_hash,
        }))
    }

    // Clear phase: fill either the given on-screen regions or the whole pixmap with the clear color.
    fn clear_pixmap(
        &mut self,
        clear_color: Option<Color>,
        skip_rect: &Option<SmallVec<[Rect; 8]>>,
    ) {
        if let (Some(color), Some(pixmap)) = (clear_color, &mut self.pixmap) {
            if let Some(rects) = skip_rect {
                for sr in rects.iter() {
                    let skia_rect = tiny_skia::Rect::from_xywh(sr.x, sr.y, sr.width, sr.height);
                    if let Some(r) = skia_rect {
                        let mut paint = tiny_skia::Paint::default();
                        paint.set_color(crate::primitives::to_skia_color(color));
                        paint.blend_mode = tiny_skia::BlendMode::Source;
                        pixmap.fill_rect(r, &paint, tiny_skia::Transform::identity(), None);
                    } else {
                        pixmap.fill(crate::primitives::to_skia_color(color));
                        break;
                    }
                }
            } else {
                pixmap.fill(crate::primitives::to_skia_color(color));
            }
        }
    }

    // Render phase: replay the (fill-layer-expanded) command list into the pixmap, honoring the
    // dirty-region skip, clip mask, matrix and layer stacks, and precomputed layer bounding boxes.
    fn run_commands(
        &mut self,
        commands: &[DrawCommand],
        skip_rect: &Option<SmallVec<[Rect; 8]>>,
        layer_bboxes: &[Option<(i32, i32, u32, u32)>],
    ) {
        // Nesting depth of PushLayer commands skipped because their bbox doesn't overlap skip_rect; their pixels are already correct from apply_scroll_blit.
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

            // Optimization 3: skip draw commands whose visual bounds don't overlap the dirty region. Only applies at the top level (not inside layers): a layer is a fresh isolated pixmap rendered from scratch every frame, so all its commands must run regardless of which window-space region is dirty.
            if let Some(dirty_rects) = skip_rect {
                if !inside_layer {
                    if let Some(vr) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cumulative_matrix,
                        &self.font_metrics,
                    ) {
                        if dirty_rects.iter().all(|dr| !vr.overlaps(*dr)) {
                            continue;
                        }
                    }
                }
            }

            match cmd {
                DrawCommand::Rect { rect, style } => {
                    let rect = *rect;
                    let style = **style;
                    if rect.width <= 0.0
                        || rect.height <= 0.0
                        || (style.fill.is_none() && style.stroke.is_none())
                    {
                        continue;
                    }
                    if let Some(vr) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cumulative_matrix,
                        &self.font_metrics,
                    ) {
                        if cull_bounds(vr, self.draw_state.current_clip()) {
                            continue;
                        }
                    }
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
                    crate::primitives::rect::draw_rect(
                        pixmap,
                        rect,
                        &style,
                        transform,
                        clip,
                        &mut self.shadow_cache,
                        &mut self.pending_shadows,
                        &mut self.blur_scratch,
                    );
                }
                DrawCommand::Text { text, rect, style } => {
                    let rect = *rect;
                    let style = **style;
                    if let Some(vr) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cumulative_matrix,
                        &self.font_metrics,
                    ) {
                        if cull_bounds(vr, self.draw_state.current_clip()) {
                            continue;
                        }
                    }
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
                    crate::primitives::text::draw_text(
                        pixmap,
                        &mut self.text_shaper,
                        text,
                        rect,
                        &style,
                        transform,
                        clip,
                        if inside_layer {
                            None
                        } else {
                            self.draw_state.current_clip()
                        },
                        &mut self.blur_scratch,
                        &mut self.text_pixmap_cache,
                        &mut self.text_shadow_cache,
                        &mut self.pending_text_shadows,
                    );
                }
                DrawCommand::Image { data, rect, filter } => {
                    if let Some(vr) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cumulative_matrix,
                        &self.font_metrics,
                    ) {
                        if cull_bounds(vr, self.draw_state.current_clip()) {
                            continue;
                        }
                    }
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
                        pixmap,
                        data,
                        &mut self.image_cache,
                        *rect,
                        *filter,
                        transform,
                        clip,
                    );
                }
                DrawCommand::Line { p1, p2, style } => {
                    if let Some(vr) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cumulative_matrix,
                        &self.font_metrics,
                    ) {
                        if cull_bounds(vr, self.draw_state.current_clip()) {
                            continue;
                        }
                    }
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
                    if let Some(vr) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cumulative_matrix,
                        &self.font_metrics,
                    ) {
                        if cull_bounds(vr, self.draw_state.current_clip()) {
                            continue;
                        }
                    }
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
                    crate::primitives::path::draw_path(
                        pixmap,
                        data,
                        &style,
                        transform,
                        clip,
                        if inside_layer {
                            None
                        } else {
                            self.draw_state.current_clip()
                        },
                        &mut self.blur_scratch,
                        &mut self.path_shadow_cache,
                        &mut self.pending_path_shadows,
                    );
                }
                DrawCommand::PushClip { rect, radius } => {
                    let prev_dirty = self.clip_mask_dirty;
                    let effective = self.draw_state.push_clip(*rect);
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
                    // During scroll_blit, skip layers outside the dirty region: their pixels are already correct from apply_scroll_blit and re-compositing would double-apply the layer's opacity.
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
            }
        }
    }
}

impl<D, W> RenderBackend for SoftwareRenderer<D, W>
where
    D: HasDisplayHandle,
    W: HasWindowHandle,
{
    fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        _scale_factor: f32,
        _generation: u64,
    ) -> Result<(), RendererError> {
        // `scale_factor` and `generation` are ignored because draw commands arrive pre-scaled by the caller; software backend does not need to track them.

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
            // Surface buffers are recreated on resize, so their age resets; drop the change log to avoid replaying onto a fresh buffer.
            self.present_history.clear();
            // Headless mode has no surface to resize; the pixmap above is the only target.
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
        Ok(())
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

        // Task 2.12: skip compute_layer_bounds when commands and dimensions haven't changed.
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

        // Borrow split: the command loop needs `&mut self`, but the expanded list lives inside `self`. Move it out for the duration of the loop and restore it after, preserving the expand cache exactly.
        let taken = std::mem::take(&mut self.expanded_commands_cache);
        self.run_commands(&taken.as_ref().unwrap().1, &skip_rect, &layer_bboxes);
        self.expanded_commands_cache = taken;

        self.present_pixmap(frame_op)
    }
}
