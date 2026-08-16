use super::*;

use super::pool::{bucket_size, return_pooled_texture, take_pooled_texture};
use super::shadow::{ShadowCacheKind, ShadowKind};
use super::steps::LayerAccum;

// A layer-boundary-split render segment: a run of draw steps, or a layer begin/composite/prerendered marker. Lifted to module scope so it can appear in the phase-method signatures that build and execute segments.
pub(super) enum Segment {
    Draw {
        start: usize,
        end: usize,
    },
    BeginLayer {
        msaa_texture: wgpu::Texture,
        msaa_view: wgpu::TextureView,
        resolve_texture: wgpu::Texture,
        resolve_view: wgpu::TextureView,
        viewport_bind_group: wgpu::BindGroup,
        width: u32,
        height: u32,
        offset_x: f32,
        offset_y: f32,
        backdrop_blur: f32,
    },
    EndLayerComposite {
        bind_group: wgpu::BindGroup,
        cache_hash: Option<u64>,
        scissor: Option<Rect>,
    },
    PrerenderedLayer {
        bind_group: wgpu::BindGroup,
        scissor: Option<Rect>,
    },
}

// Owned per-frame state threaded through the render_frame phase methods. Holds only owned values (never borrows of `self`) so it survives across the `&mut self` phase calls; `retained_view` in particular is a cheap Arc-backed clone of `self.retained_view` for exactly that reason.
pub(super) struct FrameCtx {
    direct_to_surface: bool,
    // Seed the offscreen with the retained previous frame shifted by prime_delta before the main
    // pass Loads it: scroll-blit-with-clear (delta = scroll) or F1 damage priming (delta = 0).
    prime: bool,
    prime_delta: (f32, f32),
    dirty_scissor: Option<Rect>,
    load_op: wgpu::LoadOp<wgpu::Color>,
    output: Option<wgpu::SurfaceTexture>,
    surface_view: Option<wgpu::TextureView>,
    msaa_view: Option<wgpu::TextureView>,
    retained_view: Option<wgpu::TextureView>,
    frame_scratch_textures: Vec<(
        u32,
        u32,
        wgpu::TextureFormat,
        wgpu::Texture,
        wgpu::TextureView,
    )>,
}

// F1: confine a top-level layer composite to the dirty rect. A layer's mini-layer can be larger than
// the dirty region (e.g. a translucent panel), and its composite blends at opacity over the parent —
// so without confinement it re-blends over the preserved previous frame outside the dirty rect and
// accumulates opacity every frame. Intersecting with the dirty scissor keeps the composite inside the
// region that was reset to the clear color, matching a full repaint. `None` dirty scissor = no damage,
// so the composite keeps its own clip; a layer fully outside the dirty rect has empty (culled) content,
// so falling back to the dirty rect there composites nothing.
fn confine_to_dirty(scissor: Option<Rect>, dirty: Option<Rect>) -> Option<Rect> {
    match dirty {
        None => scissor,
        Some(ds) => Some(scissor.and_then(|s| s.intersect(ds)).unwrap_or(ds)),
    }
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> RenderBackend
    for HardwareRenderer<W>
{
    // Scaling is folded into the shader's transform, so this path wants logical-pixel commands and must not
    // be handed the CPU-scaled ones the software rasteriser needs.
    fn applies_scale_factor(&self) -> bool {
        true
    }

    fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        scale_factor: f32,
        generation: u64,
    ) -> Result<(), RendererError> {
        // Shared with other render threads, exclusive against surface lifecycle: reconfigure (swapchain
        // recreation) must not overlap another window's surface teardown (see renderer_core::gpu_sync).
        let _gpu = renderer_core::gpu_sync::render_guard();
        self.scale_factor = scale_factor;
        self.incoming_generation = generation;
        if width != self.width || height != self.height || self.config.is_none() {
            // Pooled layer textures are sized to the previous surface dimensions and would be unusable at the new size; drop them so we don't leak GPU memory for textures we will never reuse.
            self.layer_texture_pool.clear();
            // Backdrop-blur scratch textures are sized to the old surface; drop them on resize for the same reason.
            self.texture_pool.clear();
            // Cached layer textures are sized to the old surface; their hashes also encode the old dimensions, so drop them on resize.
            self.layer_resolved_cache.clear();
            self.layer_resolved_cache_order.clear();
            self.width = width;
            self.height = height;
            if width > 0 && height > 0 {
                tracing::debug!(
                    "hw begin_frame: reconfigure {}x{} scale={}",
                    width,
                    height,
                    scale_factor
                );
                self.reconfigure(width, height);
            } else {
                tracing::warn!(
                    "hw begin_frame: zero size {}x{}, skipping reconfigure",
                    width,
                    height
                );
            }
        }
        self.layer_cache_pixel_budget = 4 * self.width as u64 * self.height as u64;
        self.shader_clip_active = false;
        self.shader_clip_depth = 0;
        self.shader_clip_outer_scissor = None;
        self.clear_pending();
        self.publish_cache_stats();
        // Reclaim the previous frame's composite uniform buffers; the previous frame was already submitted/presented so they are no longer referenced by in-flight GPU work.
        self.composite_pipeline.recycle_params_buffers();
        self.retained_blit_pipeline.recycle_params_buffers();
        self.viewport_buffer_pool_index = 0;
        Ok(())
    }

    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<(), RendererError> {
        // Shared with other render threads, exclusive against surface lifecycle: the acquire / present in this
        // frame must not overlap another window's surface teardown, which corrupts the shared driver and
        // segfaults inside vkAcquireNextImageKHR (see renderer_core::gpu_sync).
        let _gpu = renderer_core::gpu_sync::render_guard();
        tracing::debug!(
            "hw render_frame: {} commands, clear={}",
            commands.len(),
            clear_color.is_some()
        );
        renderer_core::perf::tick();
        let _frame_span = renderer_core::perf::span(renderer_core::perf::Phase::Frame);
        // Direct-to-swapchain fast path: when the frame clears (so there is no cross-frame scroll-blit that needs LoadOp::Load) and nothing samples the top-level target (no backdrop blur), render straight into the swapchain texture on the single-sample (Android) path. This drops the offscreen render target and its per-frame full-screen copy to the surface. MSAA (desktop, samples>1) still needs the offscreen to resolve, and a backdrop-blur layer needs a sampleable parent, so both fall back to the offscreen path.
        let frame_has_backdrop_blur = commands.iter().any(
            |c| matches!(c, DrawCommand::PushLayer { backdrop_blur, .. } if *backdrop_blur > 0.0),
        );
        // Top-level layer composites are confined to the dirty rect (see confine_to_dirty), so F1
        // damage priming is correct for opacity PushLayers, fill-layer-expanded translucent rounded
        // rects, and nested rounded PushClips — their composites no longer touch preserved pixels
        // outside the dirty region. Only a backdrop-blur layer still blocks damage: it samples the
        // parent frame, which outside the dirty rect is the primed *previous* frame, so the blur near
        // the dirty boundary would pull in stale content.
        let frame_blocks_damage = frame_has_backdrop_blur;
        // F1 on the single-sample (mobile) path damage-tracks by Loading the persistent msaa_texture,
        // which requires rendering through the offscreen rather than straight into the rotating
        // swapchain — so direct-to-surface is disabled whenever damage tracking is on. (TELAR_HW_DAMAGE=0
        // restores direct-to-surface + full repaints.)
        // An app-owned target is never drawn into directly: the frame has to arrive there through a blend, and a direct draw would replace what the application put in the texture instead.
        let direct_to_surface = self.msaa_samples == 1
            && clear_color.is_some()
            && !frame_has_backdrop_blur
            && !hw_damage_with_clear_enabled()
            && !self.app_owned_target;

        if self.try_idle_blit(direct_to_surface)? {
            return Ok(());
        }

        self.draw_state.reset();

        let interpret_start = renderer_core::perf::now_if_enabled();
        let (scroll_blit, prime, prime_delta, dirty_scissor, damage) = self.analyze_frame(
            commands,
            clear_color,
            frame_has_backdrop_blur,
            frame_blocks_damage,
        );

        renderer_core::perf::note_damage(damage);
        // F1: when damage tracking, repaint the app background (clear_color) inside the dirty scissor
        // so ghosts of moved/removed content left behind by the preserved previous frame are erased.
        let damage_bg = match (damage, dirty_scissor, clear_color) {
            (true, Some(ds), Some(c)) => Some((ds, c)),
            _ => None,
        };
        self.interpret_commands(commands, dirty_scissor, scroll_blit.as_ref(), damage_bg);
        renderer_core::perf::record_since(renderer_core::perf::Phase::Interpret, interpret_start);

        let gpu_start = renderer_core::perf::now_if_enabled();
        let mut ctx = FrameCtx {
            direct_to_surface,
            prime,
            prime_delta,
            dirty_scissor,
            load_op: wgpu::LoadOp::Load,
            output: None,
            surface_view: None,
            msaa_view: None,
            retained_view: None,
            frame_scratch_textures: Vec::new(),
        };

        if self.acquire_surface_and_upload(clear_color, &mut ctx)? {
            return Ok(());
        }

        // Single-sample damage: msaa_texture already holds the previous frame, so Load it (preserving
        // everything outside the dirty scissor) instead of clearing — the injected background rect
        // repaints clear_color only inside the dirty rect. (No effect on the MSAA prime-quad path.)
        if damage && self.msaa_samples == 1 {
            ctx.load_op = wgpu::LoadOp::Load;
        }

        // Single encoder for both the shadow pre-passes and the main pass; wgpu inserts the necessary barriers between render passes, so a separate pre-encoder and extra queue.submit are unnecessary.
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("telar-encoder"),
            });

        self.resolve_shadows(&mut encoder);

        let (steps, segments) = self.build_segments(&mut ctx)?;

        self.execute_segments(&mut encoder, &mut ctx, &steps, segments)?;

        let result = self.present(encoder, ctx, commands);
        renderer_core::perf::record_since(renderer_core::perf::Phase::Gpu, gpu_start);
        result
    }
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> HardwareRenderer<W> {
    fn try_idle_blit(&mut self, direct_to_surface: bool) -> Result<bool, RendererError> {
        // Idle-frame fast path: skip full pipeline and blit retained texture when content generation and viewport are unchanged. Disabled under direct-to-surface (nothing retains the last frame to blit from); idle frames simply re-render at the keepalive cadence instead. Also disabled headless (surface None): there is no swapchain to present into and read_rgba wants each render_frame to leave a freshly-composited frame in offscreen_output, so headless always takes the main path.
        if !direct_to_surface
            && self.surface.is_some()
            && self.incoming_generation == self.prev_generation
            && self.retained_view.is_some()
            && !self.viewport_dirty
            && self.config.is_some()
            && self.width > 0
            && self.height > 0
        {
            let acquired = {
                let _swapchain = super::swapchain_lock();
                self.surface.as_ref().unwrap().get_current_texture()
            };
            let output = match acquired {
                wgpu::CurrentSurfaceTexture::Success(t) => t,
                wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                    tracing::debug!("hw idle-blit: suboptimal surface");
                    t
                }
                wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                    tracing::warn!("hw idle-blit: surface Lost/Outdated, reconfiguring");
                    if let Some(config) = &self.config.clone() {
                        let _swapchain = super::swapchain_rebuild_lock();
                        self.surface
                            .as_ref()
                            .unwrap()
                            .configure(&self.device, config);
                    }
                    self.clear_pending();
                    return Ok(true);
                }
                wgpu::CurrentSurfaceTexture::Timeout => {
                    tracing::warn!("hw idle-blit: Timeout, skipping frame");
                    self.clear_pending();
                    return Ok(true);
                }
                wgpu::CurrentSurfaceTexture::Occluded => {
                    tracing::warn!("hw idle-blit: Occluded, skipping frame");
                    self.clear_pending();
                    return Ok(true);
                }
                other => {
                    self.clear_pending();
                    return Err(RendererError::Present(format!("surface error: {other:?}")));
                }
            };
            let surface_view = output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            {
                // Idle-blit source: on the msaa_samples==1 (Android) path, sample msaa_texture directly — it still holds the last active frame (its render pass stores and idle frames never write it), so the per-frame msaa→retained copy is gone. On MSAA>1 msaa_texture is multisampled and unsamplable, so use the resolved retained texture.
                let idle_source_view = match (self.msaa_samples, self.msaa_texture.as_ref()) {
                    (1, Some(t)) => t.create_view(&wgpu::TextureViewDescriptor::default()),
                    _ => self.retained_view.clone().unwrap(), // safe: outer if checks is_some()
                };
                let retained_bg = self.retained_blit_pipeline.create_bind_group(
                    &self.device,
                    &self.queue,
                    &idle_source_view,
                    [
                        0.0,
                        0.0,
                        self.width as f32 / self.scale_factor,
                        self.height as f32 / self.scale_factor,
                    ],
                    1.0,
                    0.0,
                    [1.0, 1.0],
                );
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("telar-idle-blit"),
                        });
                {
                    let mut blit = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("telar-idle-blit-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &surface_view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                        multiview_mask: None,
                    });
                    blit.set_pipeline(&self.retained_blit_pipeline.pipeline);
                    blit.set_bind_group(0, &self.viewport_bind_group, &[]);
                    blit.set_bind_group(1, &retained_bg, &[]);
                    blit.draw(0..6, 0..1);
                }
                self.queue.submit(std::iter::once(encoder.finish()));
            }
            tracing::debug!("hw idle-blit: presenting");
            {
                let _swapchain = super::swapchain_lock();
                self.queue.present(output);
            }
            self.clear_pending();
            return Ok(true);
        }
        Ok(false)
    }

    fn analyze_frame(
        &self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
        frame_has_backdrop_blur: bool,
        frame_blocks_damage: bool,
    ) -> (
        Option<renderer_core::ScrollBlit>,
        bool,
        (f32, f32),
        Option<Rect>,
        bool,
    ) {
        // scroll_blit normally requires LoadOp::Load (clear_color forces LoadOp::Clear). The experimental scroll-blit-with-clear path keeps the optimization for a cleared frame by priming the offscreen with the previous frame shifted by the scroll delta (so only the exposed band needs redrawing); restricted to the MSAA (desktop, explicit-init-pass) path with a retained previous frame and no backdrop blur.
        let allow_scroll_with_clear = hw_scroll_blit_enabled()
            && clear_color.is_some()
            && self.retained_view.is_some()
            && !frame_has_backdrop_blur
            && self.msaa_samples > 1;
        let scroll_blit = if clear_color.is_none() || allow_scroll_with_clear {
            renderer_core::dirty::detect_scroll_blit(commands, &self.prev_commands)
        } else {
            None
        };
        // When priming, the offscreen is seeded with the shifted previous frame instead of a plain clear, and only the exposed band is redrawn.
        let scroll_prime = allow_scroll_with_clear && scroll_blit.is_some();
        let prime_delta = scroll_blit
            .as_ref()
            .map(|sb| (sb.delta_x as f32, sb.delta_y as f32))
            .unwrap_or((0.0, 0.0));
        // F1: damage-track an opaque-clear frame like scroll-with-clear but for an arbitrary dirty rect
        // and zero delta (prime the previous frame, repaint only the dirty scissor). Same gates as
        // scroll-with-clear, plus no PushLayer (an opacity/blur layer re-composited from only its dirty
        // slice over the primed frame is wrong — deferred), and only when the dirty region is small
        // enough to beat a plain full clear+repaint.
        // Two damage substrates for the previous frame: MSAA desktop resolves into retained_view and
        // re-seeds the multisample target with a prime quad; single-sample mobile keeps the previous
        // frame in msaa_texture itself (it persists — its render pass stores and idle frames never
        // write it), so its damage path Loads msaa_texture directly (see render_frame's load_op).
        // Requires an OPAQUE clear: the injected background rect draws through the premultiplied-alpha
        // rect pipeline, so a translucent clear_color would blend over (not replace) the primed frame
        // inside the dirty rect and accumulate error each frame, diverging from a full LoadOp::Clear.
        let allow_damage_with_clear = hw_damage_with_clear_enabled()
            && clear_color.is_some_and(|c| c.a >= 1.0)
            && !frame_blocks_damage
            && ((self.msaa_samples > 1 && self.retained_view.is_some()) || self.msaa_samples == 1);
        // A transparent frame Loads instead of Clearing, which assumes the target still holds the previous
        // frame. That holds on the single-sample path, where the offscreen persists across frames (see the
        // note above), and NOT on the multisample one: there the frame is resolved out to `retained_view`
        // and the multisample target is left with no dependable copy of what was last presented. Repainting
        // only the dirty rect over that dropped everything that had not changed this frame — on a
        // transparent bar the chips blinked out one at a time, and hovering, which dirties the rect under
        // the cursor, brought one back for a single frame. The opaque path is immune because it re-seeds
        // with a prime quad from `retained_view` first; giving the transparent path the same prime needs an
        // erase-to-transparent inside the dirty rect (the primed pixels are not overwritten by a background
        // fill, so a moved element would leave a ghost), which is why this refuses rather than primes.
        let allow_damage_transparent = clear_color.is_none() && self.msaa_samples == 1;
        // Multiple dirty rects are collapsed to their bounding union because GPUs support only a single scissor rect per pass (hardware limitation asymmetry vs. software backend which can clip per-rect).
        let dirty_scissor: Option<Rect> = if scroll_blit.is_none()
            && !self.prev_commands.is_empty()
            && (allow_damage_transparent || allow_damage_with_clear)
        {
            renderer_core::dirty::compute_dirty_rect(commands, &self.prev_commands, |cmd, m| {
                renderer_core::culling::command_visual_rect(cmd, m, &self.font_metrics)
            })
            .and_then(|rects| rects.into_iter().reduce(Rect::union))
            .filter(|ds| self.damage_worth_priming(*ds))
        } else {
            None
        };
        let damage = allow_damage_with_clear && dirty_scissor.is_some();
        // The prime quad only serves the multisample (desktop) target; the single-sample damage path
        // Loads its persistent msaa_texture instead, so it primes without a quad.
        let prime = scroll_prime || (damage && self.msaa_samples > 1);
        (scroll_blit, prime, prime_delta, dirty_scissor, damage)
    }

    // A near-full-surface dirty rect costs more to damage-prime (retained quad + repaint) than a plain
    // full clear+repaint, so only prime when the dirty region stays under a fraction of the surface.
    fn damage_worth_priming(&self, ds: Rect) -> bool {
        let logical_w = self.width as f32 / self.scale_factor;
        let logical_h = self.height as f32 / self.scale_factor;
        let surface_area = (logical_w * logical_h).max(1.0);
        let dirty_area = ds.width.max(0.0) * ds.height.max(0.0);
        dirty_area <= surface_area * 0.6
    }

    fn interpret_commands(
        &mut self,
        commands: &[DrawCommand],
        dirty_scissor: Option<Rect>,
        scroll_blit: Option<&renderer_core::ScrollBlit>,
        damage_bg: Option<(Rect, Color)>,
    ) {
        let mut current_scissor: Option<Rect> = None;
        let mut scissor_layer_stack: Vec<Option<Rect>> = Vec::new(); // saves/restores current_scissor across PushLayer/PopLayer; layers disable frustum culling inside their bounds
        let mut layer_accum_stack: Vec<LayerAccum> = Vec::new();
        // Composite bind_groups for rounded PushClip mini-layers, consumed at the matching PopClip.
        let mut round_clip_composite: Vec<wgpu::BindGroup> = Vec::new();
        // Parallel to draw_state clip stack: true = rounded mini-layer, false = scissor rect.
        let mut clip_is_round: Vec<bool> = Vec::new();
        let expanded_commands = expand_fill_layers(commands);
        let commands: &[DrawCommand] = expanded_commands.as_deref().unwrap_or(commands);

        // F1 damage prime: the init pass seeded the whole target with the previous frame. Confine the
        // main pass to the dirty scissor (a leading SetScissor{None} resolves to dirty_scissor in
        // execute_segments) and repaint the app background there so ghosts of moved/removed content
        // are erased before the changed + overlapping-unchanged commands redraw on top.
        if let Some((dirty_rect, clear_col)) = damage_bg {
            self.pending_steps.push(DrawStep::SetScissor { rect: None });
            // Inflate 1px so the physical scissor (which floors/ceils) is fully covered by the fill.
            let bg_rect = Rect::new(
                dirty_rect.x - 1.0,
                dirty_rect.y - 1.0,
                dirty_rect.width + 2.0,
                dirty_rect.height + 2.0,
            );
            let bg_style = renderer_core::RectStyle {
                fill: Some(renderer_core::Paint::Solid(clear_col)),
                ..Default::default()
            };
            if self.batch_rect_start.is_none() {
                self.batch_rect_start = Some(self.pending_instances.len() as u32);
            }
            let inst = crate::primitives::rect::prepare_rect(
                bg_rect,
                &bg_style,
                geometry_core::Transform::IDENTITY.to_array(),
            );
            self.pending_instances.push(inst);
        }

        for (cmd_idx, cmd) in commands.iter().enumerate() {
            // Ahead of the bounds below, and that order is the point: a rect that draws nothing would otherwise
            // union into the layer accumulator and grow the layer texture to cover it.
            if let DrawCommand::Rect { rect, style } = cmd
                && (rect.width <= 0.0
                    || rect.height <= 0.0
                    || (style.fill.is_none() && style.stroke.is_none()))
            {
                continue;
            }

            // One set of bounds for the whole body: every drawing arm asked the same question of the same
            // command. `None` for the state commands (clip/matrix/layer), which must never be culled.
            if let Some(bounds) = renderer_core::culling::command_visual_rect(
                cmd,
                self.draw_state.cumulative_matrix,
                &self.font_metrics,
            ) {
                if cull_bounds(bounds, current_scissor, dirty_scissor, scroll_blit) {
                    continue;
                }
                if let Some(accum) = layer_accum_stack.last_mut() {
                    accum.extend(bounds);
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
                    self.flush_text();
                    self.flush_line();
                    self.flush_image();
                    if self.batch_rect_start.is_none() {
                        self.batch_rect_start = Some(self.pending_instances.len() as u32);
                    }
                    let inst = crate::primitives::rect::prepare_rect(
                        rect,
                        &style,
                        self.draw_state.cumulative_matrix,
                    );
                    self.pending_instances.push(inst);
                }
                DrawCommand::Text { text, rect, style } => {
                    let rect = *rect;
                    let style = **style;
                    self.flush_rect();
                    self.flush_line();
                    self.flush_image();
                    let (text_tx, text_ty) = self.draw_state.apply_point(rect.x, rect.y);
                    let (text_tx2, text_ty2) = self
                        .draw_state
                        .apply_point(rect.x + rect.width, rect.y + rect.height);
                    let translated = Rect::new(
                        text_tx,
                        text_ty,
                        (text_tx2 - text_tx).abs(),
                        (text_ty2 - text_ty).abs(),
                    );
                    if let Some(shadow) = style.shadow {
                        self.flush_text();

                        let shadow_rect = Rect::new(
                            translated.x + shadow.offset_x,
                            translated.y + shadow.offset_y,
                            translated.width,
                            translated.height,
                        );
                        let shadow_layout = renderer_core::ShadowLayout::compute(
                            shadow.blur_radius,
                            shadow_rect.x,
                            shadow_rect.x + shadow_rect.width,
                            shadow_rect.y,
                            shadow_rect.y + shadow_rect.height,
                            self.scale_factor,
                        );
                        let sigma_phys = shadow_layout.sigma;
                        let origin_x = shadow_layout.origin_x;
                        let origin_y = shadow_layout.origin_y;
                        let texture_width_logical = shadow_layout.texture_width_logical;
                        let texture_height_logical = shadow_layout.texture_height_logical;
                        let texture_width = shadow_layout.texture_width;
                        let texture_height = shadow_layout.texture_height;

                        let shadow_style = renderer_core::TextStyle {
                            paint: renderer_core::Paint::Solid(shadow.color),
                            shadow: None,
                            ..style
                        };
                        let instance_start = self.pending_shadow_instances.len() as u32;
                        crate::caches::with_shared(|caches| {
                            crate::primitives::text::prepare_text(
                                &mut caches.text_shaper,
                                text,
                                shadow_rect,
                                &shadow_style,
                                self.scale_factor,
                                &mut self.pending_shadow_instances,
                                &mut self.glyph_scratch,
                            )
                        });
                        let instance_end = self.pending_shadow_instances.len() as u32;
                        for inst in &mut self.pending_shadow_instances[instance_start as usize..] {
                            inst.dest_rect[0] -= origin_x;
                            inst.dest_rect[1] -= origin_y;
                        }

                        self.pending_shadows.push(ShadowOp {
                            kind: ShadowKind::Text {
                                instance_start,
                                instance_end,
                            },
                            sigma: sigma_phys,
                            texture_width,
                            texture_height,
                            dest: [
                                origin_x,
                                origin_y,
                                texture_width_logical as f32,
                                texture_height_logical as f32,
                            ],
                        });
                        self.pending_steps.push(DrawStep::ShadowPlaceholder {
                            op_index: self.pending_shadows.len() - 1,
                        });
                    }
                    if self.batch_text_start.is_none() {
                        self.batch_text_start = Some(self.pending_text_instances.len() as u32);
                    }
                    crate::caches::with_shared(|caches| {
                        crate::primitives::text::prepare_text(
                            &mut caches.text_shaper,
                            text,
                            translated,
                            &style,
                            self.scale_factor,
                            &mut self.pending_text_instances,
                            &mut self.glyph_scratch,
                        )
                    });
                }
                DrawCommand::RichText { runs, rect, base } => {
                    let rect = *rect;
                    let base = **base;
                    self.flush_rect();
                    self.flush_line();
                    self.flush_image();
                    let (tx, ty) = self.draw_state.apply_point(rect.x, rect.y);
                    let (tx2, ty2) = self
                        .draw_state
                        .apply_point(rect.x + rect.width, rect.y + rect.height);
                    let translated = Rect::new(tx, ty, (tx2 - tx).abs(), (ty2 - ty).abs());
                    if self.batch_text_start.is_none() {
                        self.batch_text_start = Some(self.pending_text_instances.len() as u32);
                    }
                    crate::caches::with_shared(|caches| {
                        crate::primitives::text::prepare_rich_text(
                            &mut caches.text_shaper,
                            runs,
                            translated,
                            &base,
                            self.scale_factor,
                            &mut self.pending_text_instances,
                            &mut self.glyph_scratch,
                        )
                    });
                }
                DrawCommand::Image { data, rect, filter } => {
                    self.flush_rect();
                    self.flush_text();
                    self.flush_line();
                    let key = (data.id, *filter);
                    if self.batch_image_start.is_none() || self.batch_image_key != Some(key) {
                        self.flush_image();
                        self.batch_image_key = Some(key);
                        self.batch_image_start = Some(self.pending_image_instances.len() as u32);
                        self.batch_image_bind_group = crate::caches::with_shared(|caches| {
                            caches
                                .images
                                .bind_group(&self.device, &self.queue, &data, *filter)
                        })
                        .flatten();
                    }
                    let (ix1, iy1) = self.draw_state.apply_point(rect.x, rect.y);
                    let (ix2, iy2) = self.draw_state.apply_point(rect.x + rect.width, rect.y);
                    let (ix3, iy3) = self.draw_state.apply_point(rect.x, rect.y + rect.height);
                    let (ix4, iy4) = self
                        .draw_state
                        .apply_point(rect.x + rect.width, rect.y + rect.height);
                    let imin_x = ix1.min(ix2).min(ix3).min(ix4);
                    let imin_y = iy1.min(iy2).min(iy3).min(iy4);
                    let imax_x = ix1.max(ix2).max(ix3).max(ix4);
                    let imax_y = iy1.max(iy2).max(iy3).max(iy4);
                    let translated = Rect::new(imin_x, imin_y, imax_x - imin_x, imax_y - imin_y);
                    self.pending_image_instances
                        .push(crate::primitives::image::prepare_image(translated));
                }
                DrawCommand::Line { p1, p2, style } => {
                    self.flush_rect();
                    self.flush_text();
                    self.flush_image();
                    if self.batch_line_start.is_none() {
                        self.batch_line_start = Some(self.pending_line_instances.len() as u32);
                    }
                    use geometry_core::Point;
                    let (lx1, ly1) = self.draw_state.apply_point(p1.x, p1.y);
                    let (lx2, ly2) = self.draw_state.apply_point(p2.x, p2.y);
                    let tp1 = Point::new(lx1, ly1);
                    let tp2 = Point::new(lx2, ly2);
                    self.pending_line_instances
                        .push(crate::primitives::line::prepare_line(
                            tp1,
                            tp2,
                            *style,
                            self.draw_state.cumulative_matrix,
                        ));
                }
                DrawCommand::Path { data, style } => {
                    let style = **style;
                    self.flush_all();

                    if let Some(shadow) = style.shadow {
                        let shadow_fill = style
                            .fill
                            .map(|_| renderer_core::Paint::Solid(shadow.color));
                        let shadow_stroke = style.stroke.map(|s| renderer_core::Stroke {
                            paint: renderer_core::Paint::Solid(shadow.color),
                            ..s
                        });
                        let shadow_style = renderer_core::PathStyle {
                            fill: shadow_fill,
                            stroke: shadow_stroke,
                            shadow: None,
                            fill_rule: style.fill_rule,
                        };

                        let sv_start = self.pending_shadow_path_vertices.len();
                        let si_start = self.pending_shadow_path_indices.len() as u32;
                        crate::caches::with_shared(|caches| {
                            crate::primitives::path::prepare_path(
                                &mut caches.path_tess,
                                data,
                                &shadow_style,
                                &mut self.pending_shadow_path_vertices,
                                &mut self.pending_shadow_path_indices,
                                &mut self.pending_shadow_path_fill_data,
                            )
                        });
                        let si_end = self.pending_shadow_path_indices.len() as u32;

                        if si_end > si_start {
                            let (mut min_x, mut min_y, mut max_x, mut max_y) =
                                (f32::MAX, f32::MAX, f32::NEG_INFINITY, f32::NEG_INFINITY);
                            for v in &self.pending_shadow_path_vertices[sv_start..] {
                                min_x = min_x.min(v.position[0]);
                                min_y = min_y.min(v.position[1]);
                                max_x = max_x.max(v.position[0]);
                                max_y = max_y.max(v.position[1]);
                            }

                            let (wmin_x, wmin_y) = self.draw_state.apply_point(min_x, min_y);
                            let (wmax_x, wmax_y) = self.draw_state.apply_point(max_x, max_y);
                            let world_min_x = wmin_x.min(wmax_x) + shadow.offset_x;
                            let world_min_y = wmin_y.min(wmax_y) + shadow.offset_y;
                            let world_max_x = wmin_x.max(wmax_x) + shadow.offset_x;
                            let world_max_y = wmin_y.max(wmax_y) + shadow.offset_y;

                            let shadow_layout = renderer_core::ShadowLayout::compute(
                                shadow.blur_radius,
                                world_min_x,
                                world_max_x,
                                world_min_y,
                                world_max_y,
                                self.scale_factor,
                            );
                            let sigma_phys = shadow_layout.sigma;
                            let origin_x = shadow_layout.origin_x;
                            let origin_y = shadow_layout.origin_y;
                            let texture_width_logical = shadow_layout.texture_width_logical;
                            let texture_height_logical = shadow_layout.texture_height_logical;
                            let texture_width = shadow_layout.texture_width;
                            let texture_height = shadow_layout.texture_height;

                            for v in &mut self.pending_shadow_path_vertices[sv_start..] {
                                let (wx, wy) =
                                    self.draw_state.apply_point(v.position[0], v.position[1]);
                                v.position[0] = wx + shadow.offset_x - origin_x;
                                v.position[1] = wy + shadow.offset_y - origin_y;
                            }

                            self.pending_shadows.push(ShadowOp {
                                kind: ShadowKind::Path {
                                    index_start: si_start,
                                    index_end: si_end,
                                },
                                sigma: sigma_phys,
                                texture_width,
                                texture_height,
                                dest: [
                                    origin_x,
                                    origin_y,
                                    texture_width_logical as f32,
                                    texture_height_logical as f32,
                                ],
                            });
                            self.pending_steps.push(DrawStep::ShadowPlaceholder {
                                op_index: self.pending_shadows.len() - 1,
                            });
                        }
                    }

                    {
                        let vertex_start = self.pending_path_vertices.len();
                        let index_start = self.pending_path_indices.len() as u32;
                        let fill_data_start = self.pending_path_fill_data.len();
                        crate::caches::with_shared(|caches| {
                            crate::primitives::path::prepare_path(
                                &mut caches.path_tess,
                                data,
                                &style,
                                &mut self.pending_path_vertices,
                                &mut self.pending_path_indices,
                                &mut self.pending_path_fill_data,
                            )
                        });
                        for v in &mut self.pending_path_vertices[vertex_start..] {
                            let (wx, wy) =
                                self.draw_state.apply_point(v.position[0], v.position[1]);
                            v.position[0] = wx;
                            v.position[1] = wy;
                        }
                        for fd in &mut self.pending_path_fill_data[fill_data_start..] {
                            let (gx0, gy0) =
                                self.draw_state.apply_point(fd.grad_p0[0], fd.grad_p0[1]);
                            let (gx1, gy1) =
                                self.draw_state.apply_point(fd.grad_p1[0], fd.grad_p1[1]);
                            fd.grad_p0 = [gx0, gy0];
                            fd.grad_p1 = [gx1, gy1];
                        }
                        let index_end = self.pending_path_indices.len() as u32;
                        if index_end > index_start {
                            self.pending_steps.push(DrawStep::PathDraw {
                                index_start,
                                index_end,
                            });
                        }
                    }
                }
                DrawCommand::PushClip { rect, radius } => {
                    self.flush_all();
                    // Clip rects arrive in the emitting widget's local space; map through the active matrix so scissors and rounded mini-layers land in window space (composes with scroll/layout transforms).
                    let rect = &renderer_core::transform_clip_rect(
                        self.draw_state.cumulative_matrix,
                        *rect,
                    );
                    if radius.is_zero() {
                        let effective = self.draw_state.push_clip(*rect);
                        current_scissor = Some(effective);
                        clip_is_round.push(false);
                        self.pending_steps.push(DrawStep::SetScissor {
                            rect: Some(effective),
                        });
                    } else if !self.shader_clip_active {
                        // Non-nested rounded clip: mask corners in-shader via the viewport SDF, no mini-layer. A scissor to the clip rect still bounds the cheap pixels. TODO(sprint3-t8): a PushLayer nested inside this shader clip renders into its own pass without the SDF mask, so the layer's corners are not rounded; such cases still need the mini-layer fallback.
                        let effective = self.draw_state.push_clip(*rect);
                        clip_is_round.push(false);
                        self.shader_clip_active = true;
                        self.shader_clip_depth = clip_is_round.len();
                        self.shader_clip_outer_scissor = current_scissor;
                        current_scissor = Some(effective);
                        let clip_vp_bg =
                            self.take_shader_clip_viewport_bind_group(*rect, radius.top_left);
                        self.pending_steps.push(DrawStep::SetShaderClip {
                            viewport_bind_group: clip_vp_bg,
                        });
                        self.pending_steps.push(DrawStep::SetScissor {
                            rect: Some(effective),
                        });
                    } else {
                        // Nested rounded clip: fall back to a mini-layer, draw into it, composite with SDF mask at PopClip.
                        scissor_layer_stack.push(current_scissor);
                        current_scissor = None;
                        self.draw_state.push_clip(*rect);
                        clip_is_round.push(true);
                        let ox = rect.x.floor().max(0.0);
                        let oy = rect.y.floor().max(0.0);
                        let texture_width_logical = (rect.width.ceil() as u32).max(1);
                        let texture_height_logical = (rect.height.ceil() as u32).max(1);
                        let texture_width = ((texture_width_logical as f32 * self.scale_factor)
                            .ceil() as u32)
                            .min(self.width.max(1));
                        let texture_height = ((texture_height_logical as f32 * self.scale_factor)
                            .ceil() as u32)
                            .min(self.height.max(1));
                        let bucket_w = bucket_size(texture_width);
                        let bucket_h = bucket_size(texture_height);
                        let (msaa_texture, msaa_view, resolve_texture, resolve_view) =
                            if let Some(pos) =
                                self.layer_texture_pool
                                    .iter()
                                    .position(|p: &PooledTexture| {
                                        p.bucket_width == bucket_w && p.bucket_height == bucket_h
                                    })
                            {
                                let p = self.layer_texture_pool.remove(pos);
                                (
                                    p.msaa_texture,
                                    p.msaa_view,
                                    p.resolve_texture,
                                    p.resolve_view,
                                )
                            } else {
                                self.layer_pipeline.create_layer_textures(
                                    &self.device,
                                    bucket_w,
                                    bucket_h,
                                )
                            };
                        // Use physical bucket dimensions: to_ndc scales logical coords by scale_factor, so size must be physical to map [0, logical_w] correctly to NDC [-1, 1].
                        let layer_vp = Viewport::new(
                            [bucket_w as f32, bucket_h as f32],
                            [ox * self.scale_factor, oy * self.scale_factor],
                            self.scale_factor,
                        );
                        let layer_vp_bg = self.take_layer_viewport_bind_group(layer_vp);
                        let uv_scale = [
                            texture_width as f32 / bucket_w as f32,
                            texture_height as f32 / bucket_h as f32,
                        ];
                        // composite_bg borrows resolve_view before it moves into BeginLayer
                        let composite_bg = self.composite_pipeline.create_bind_group(
                            &self.device,
                            &self.queue,
                            &resolve_view,
                            [
                                ox,
                                oy,
                                texture_width_logical as f32,
                                texture_height_logical as f32,
                            ],
                            1.0,
                            radius.top_left,
                            uv_scale,
                        );
                        self.pending_steps.push(DrawStep::BeginLayer {
                            msaa_texture,
                            msaa_view,
                            resolve_texture,
                            resolve_view,
                            viewport_bind_group: layer_vp_bg,
                            width: bucket_w,
                            height: bucket_h,
                            offset_x: ox,
                            offset_y: oy,
                            backdrop_blur: 0.0,
                        });
                        // Pool return for clip layer textures is handled by the EndLayerComposite execution path.
                        round_clip_composite.push(composite_bg);
                    }
                }
                DrawCommand::PopClip => {
                    self.flush_all();
                    let popped_depth = clip_is_round.len();
                    if clip_is_round.pop() == Some(true) {
                        let composite_bg = round_clip_composite
                            .pop()
                            .expect("round_clip_composite underflow");
                        self.draw_state.pop_clip();
                        current_scissor = scissor_layer_stack.pop().flatten();
                        self.pending_steps.push(DrawStep::EndLayerComposite {
                            bind_group: composite_bg,
                            // Round-clip layers draw the clip mask into the texture, so their content is not safely cacheable by command hash.
                            cache_hash: None,
                            scissor: current_scissor,
                        });
                        self.pending_steps.push(DrawStep::SetScissor {
                            rect: current_scissor,
                        });
                    } else if self.shader_clip_active && popped_depth == self.shader_clip_depth {
                        // Matching PopClip for the active in-shader rounded clip: restore the unclipped viewport and the outer scissor.
                        self.draw_state.pop_clip();
                        self.shader_clip_active = false;
                        current_scissor = self.shader_clip_outer_scissor;
                        let base_vp_bg = self.take_shader_clip_viewport_bind_group(
                            Rect::new(0.0, 0.0, 0.0, 0.0),
                            0.0,
                        );
                        self.pending_steps.push(DrawStep::SetShaderClip {
                            viewport_bind_group: base_vp_bg,
                        });
                        self.pending_steps.push(DrawStep::SetScissor {
                            rect: current_scissor,
                        });
                    } else {
                        let effective = self.draw_state.pop_clip();
                        current_scissor = effective;
                        self.pending_steps
                            .push(DrawStep::SetScissor { rect: effective });
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
                    self.flush_all();
                    // Disable frustum culling inside the layer to avoid incorrect culling by an outer PushClip; save scissor for restore at PopLayer.
                    scissor_layer_stack.push(current_scissor);
                    current_scissor = None;
                    layer_accum_stack.push(LayerAccum {
                        opacity: *opacity,
                        backdrop_blur: *backdrop_blur,
                        begin_step_index: self.pending_steps.len(),
                        bounds: None,
                        command_start: cmd_idx + 1,
                        instance_start: self.pending_instances.len() as u32,
                        text_instance_start: self.pending_text_instances.len() as u32,
                        line_instance_start: self.pending_line_instances.len() as u32,
                        image_instance_start: self.pending_image_instances.len() as u32,
                    });
                }
                DrawCommand::PopLayer => {
                    self.flush_all();
                    current_scissor = scissor_layer_stack.pop().flatten();
                    if let Some(accum) = layer_accum_stack.pop() {
                        // Fully-culled opacity layer: all of its content was culled (e.g. it scrolled outside the dirty band, or is off-screen), so it composites nothing — emit no layer passes at all instead of an empty full-screen layer texture + render/resolve/composite passes. Mirrors the software renderer's skip_layer_depth. Authoritative emptiness check is "produced no draw steps" (the flush_all above already flushed this layer's instances into steps); a drawn primitive always emits a step even when its bounds are absent. Backdrop-blur layers are kept (they sample the framebuffer even without their own content).
                        if self.pending_steps.len() == accum.begin_step_index
                            && accum.backdrop_blur == 0.0
                        {
                            self.pending_instances
                                .truncate(accum.instance_start as usize);
                            self.pending_text_instances
                                .truncate(accum.text_instance_start as usize);
                            self.pending_line_instances
                                .truncate(accum.line_instance_start as usize);
                            self.pending_image_instances
                                .truncate(accum.image_instance_start as usize);
                            continue;
                        }
                        let (
                            offset_x,
                            offset_y,
                            texture_width,
                            texture_height,
                            texture_width_logical,
                            texture_height_logical,
                        ) = if let Some(b) = accum.bounds {
                            let ox = b.x.floor().max(0.0);
                            let oy = b.y.floor().max(0.0);
                            let wl = (b.width.ceil() as u32).max(1);
                            let hl = (b.height.ceil() as u32).max(1);
                            let wp = ((wl as f32 * self.scale_factor).ceil() as u32)
                                .min(self.width.max(1));
                            let hp = ((hl as f32 * self.scale_factor).ceil() as u32)
                                .min(self.height.max(1));
                            (ox, oy, wp, hp, wl, hl)
                        } else {
                            let wl = (self.width as f32 / self.scale_factor).ceil() as u32;
                            let hl = (self.height as f32 / self.scale_factor).ceil() as u32;
                            (0.0, 0.0, self.width.max(1), self.height.max(1), wl, hl)
                        };
                        // Propagate this layer's visual footprint to the parent layer so nested layers are included in the parent's bounds (and thus its texture size).
                        if let Some(parent) = layer_accum_stack.last_mut() {
                            let footprint = Rect::new(
                                offset_x,
                                offset_y,
                                texture_width_logical as f32,
                                texture_height_logical as f32,
                            );
                            parent.bounds =
                                Some(parent.bounds.map_or(footprint, |b| b.union(footprint)));
                        }
                        // Backdrop-blur layers read framebuffer content, so they are never cacheable.
                        let layer_hash: Option<u64> = if accum.backdrop_blur == 0.0 {
                            use std::hash::{Hash, Hasher};
                            let base = renderer_core::hash_draw_commands(
                                &commands[accum.command_start..cmd_idx],
                            );
                            let mut h = FxHasher::default();
                            base.hash(&mut h);
                            accum.opacity.to_bits().hash(&mut h);
                            // Use the unclamped floored world bounds (not offset_x/y which are max'd to 0) so that different scroll positions with the same clamped offset don't alias to the same cache entry and produce stale composites.
                            let (hash_bx, hash_by) = accum
                                .bounds
                                .map_or((0.0f32, 0.0f32), |b| (b.x.floor(), b.y.floor()));
                            hash_bx.to_bits().hash(&mut h);
                            hash_by.to_bits().hash(&mut h);
                            texture_width.hash(&mut h);
                            texture_height.hash(&mut h);
                            // Text shaping and rasterization depend on the scale factor; mix it in so a scale change without a resize invalidates stale entries.
                            self.scale_factor.to_bits().hash(&mut h);
                            Some(h.finish())
                        } else {
                            None
                        };
                        let cache_hit =
                            layer_hash.is_some_and(|h| self.layer_resolved_cache.contains_key(&h));
                        let bucket_w = bucket_size(texture_width);
                        let bucket_h = bucket_size(texture_height);
                        if cache_hit {
                            let hash = layer_hash.unwrap();
                            let uv_scale = [
                                texture_width as f32 / bucket_w as f32,
                                texture_height as f32 / bucket_h as f32,
                            ];
                            let bind_group = {
                                let (_, cached_view, _) = &self.layer_resolved_cache[&hash];
                                self.composite_pipeline.create_bind_group(
                                    &self.device,
                                    &self.queue,
                                    cached_view,
                                    [
                                        offset_x,
                                        offset_y,
                                        texture_width_logical as f32,
                                        texture_height_logical as f32,
                                    ],
                                    accum.opacity,
                                    0.0,
                                    uv_scale,
                                )
                            };
                            // Refresh LRU position so reused layers are not evicted first.
                            if let Some(pos) = self
                                .layer_resolved_cache_order
                                .iter()
                                .position(|k| *k == hash)
                            {
                                self.layer_resolved_cache_order.remove(pos);
                            }
                            self.layer_resolved_cache_order.push_back(hash);
                            // The layer content emitted DrawSteps and instance data we no longer need; drop them so they neither render nor leave dangling instance ranges.
                            self.pending_steps.truncate(accum.begin_step_index);
                            self.pending_instances
                                .truncate(accum.instance_start as usize);
                            self.pending_text_instances
                                .truncate(accum.text_instance_start as usize);
                            self.pending_line_instances
                                .truncate(accum.line_instance_start as usize);
                            self.pending_image_instances
                                .truncate(accum.image_instance_start as usize);
                            self.pending_steps.push(DrawStep::PrerenderedLayer {
                                bind_group,
                                scissor: current_scissor,
                            });
                            // Re-apply the outer scissor after the segment boundary. Skip when None: emitting (0,0,w,h) inside the nested layer render pass would use window dimensions on a smaller texture and fail validation.
                            if let Some(s) = current_scissor {
                                self.pending_steps
                                    .push(DrawStep::SetScissor { rect: Some(s) });
                            }
                        } else {
                            let (msaa_texture, msaa_view, resolve_texture, resolve_view) =
                                if let Some(pos) =
                                    self.layer_texture_pool
                                        .iter()
                                        .position(|p: &PooledTexture| {
                                            p.bucket_width == bucket_w
                                                && p.bucket_height == bucket_h
                                        })
                                {
                                    let p = self.layer_texture_pool.remove(pos);
                                    (
                                        p.msaa_texture,
                                        p.msaa_view,
                                        p.resolve_texture,
                                        p.resolve_view,
                                    )
                                } else {
                                    self.layer_pipeline.create_layer_textures(
                                        &self.device,
                                        bucket_w,
                                        bucket_h,
                                    )
                                };
                            // Physical bucket dimensions: to_ndc multiplies logical coords by scale_factor, so using physical size correctly maps logical content into the physical texture.
                            let layer_vp = Viewport::new(
                                [bucket_w as f32, bucket_h as f32],
                                [offset_x * self.scale_factor, offset_y * self.scale_factor],
                                self.scale_factor,
                            );
                            let layer_vp_bg = self.take_layer_viewport_bind_group(layer_vp);
                            let uv_scale = [
                                texture_width as f32 / bucket_w as f32,
                                texture_height as f32 / bucket_h as f32,
                            ];
                            // Composite bind group uses window-absolute dest rect in logical pixels; parent viewport (set 0) converts it to NDC.
                            let composite_bg = self.composite_pipeline.create_bind_group(
                                &self.device,
                                &self.queue,
                                &resolve_view,
                                [
                                    offset_x,
                                    offset_y,
                                    texture_width_logical as f32,
                                    texture_height_logical as f32,
                                ],
                                accum.opacity,
                                0.0,
                                uv_scale,
                            );
                            self.pending_steps.insert(
                                accum.begin_step_index,
                                DrawStep::BeginLayer {
                                    msaa_texture,
                                    msaa_view,
                                    resolve_texture,
                                    resolve_view,
                                    viewport_bind_group: layer_vp_bg,
                                    width: bucket_w,
                                    height: bucket_h,
                                    offset_x,
                                    offset_y,
                                    backdrop_blur: accum.backdrop_blur,
                                },
                            );
                            self.pending_steps.push(DrawStep::EndLayerComposite {
                                bind_group: composite_bg,
                                // Only cache when no dirty-scissor is active; otherwise the layer's draws may be clipped to the dirty region, leaving a partially-rendered texture.
                                cache_hash: if dirty_scissor.is_none() {
                                    layer_hash
                                } else {
                                    None
                                },
                                scissor: current_scissor,
                            });
                            // Re-apply the outer scissor after the segment boundary. Skip when None: emitting (0,0,w,h) inside the nested layer render pass would use window dimensions on a smaller texture and fail validation.
                            if let Some(s) = current_scissor {
                                self.pending_steps
                                    .push(DrawStep::SetScissor { rect: Some(s) });
                            }
                        }
                    }
                }
            }
        }

        self.flush_all();
    }

    fn acquire_surface_and_upload(
        &mut self,
        clear_color: Option<Color>,
        ctx: &mut FrameCtx,
    ) -> Result<bool, RendererError> {
        ctx.load_op = if let Some(c) = clear_color {
            let c_arr = c.to_array();
            wgpu::LoadOp::Clear(wgpu::Color {
                r: c_arr[0] as f64,
                g: c_arr[1] as f64,
                b: c_arr[2] as f64,
                a: c_arr[3] as f64,
            })
        } else {
            wgpu::LoadOp::Load
        };

        if self.config.is_none() || self.width == 0 || self.height == 0 {
            tracing::warn!(
                "hw render_frame: skipping, config={} w={} h={}",
                self.config.is_some(),
                self.width,
                self.height
            );
            self.clear_pending();
            return Ok(true);
        }

        // Windowed: acquire the swapchain texture to present into. Headless (surface None): `output` stays None and every draw targets `offscreen_output` instead.
        let output: Option<wgpu::SurfaceTexture> = if self.surface.is_some() {
            let acquired = {
                let _swapchain = super::swapchain_lock();
                self.surface.as_ref().unwrap().get_current_texture()
            };
            match acquired {
                wgpu::CurrentSurfaceTexture::Success(t) => Some(t),
                wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                    tracing::debug!("hw render_frame: suboptimal surface, rendering anyway");
                    Some(t)
                }
                wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                    tracing::warn!(
                        "hw render_frame: surface Lost/Outdated, reconfiguring {}x{}",
                        self.width,
                        self.height
                    );
                    if let Some(config) = &self.config.clone() {
                        let _swapchain = super::swapchain_rebuild_lock();
                        self.surface
                            .as_ref()
                            .unwrap()
                            .configure(&self.device, config);
                    }
                    self.clear_pending();
                    return Ok(true);
                }
                wgpu::CurrentSurfaceTexture::Timeout => {
                    tracing::warn!("hw render_frame: surface Timeout, skipping frame");
                    self.clear_pending();
                    return Ok(true);
                }
                wgpu::CurrentSurfaceTexture::Occluded => {
                    tracing::warn!("hw render_frame: surface Occluded, skipping frame");
                    self.clear_pending();
                    return Ok(true);
                }
                other => {
                    self.clear_pending();
                    return Err(RendererError::Present(format!("surface error: {other:?}")));
                }
            }
        } else {
            None
        };
        ctx.output = output;

        if self.viewport_dirty {
            let viewport = Viewport::new(
                [self.width as f32, self.height as f32],
                [0.0; 2],
                self.scale_factor,
            );
            self.queue
                .write_buffer(&self.viewport_buffer, 0, bytemuck::bytes_of(&viewport));
            self.viewport_dirty = false;
        }

        crate::caches::with_shared(|caches| {
            caches
                .atlas
                .sync(&self.queue, &mut caches.text_shaper.atlas)
        });

        if !self.pending_instances.is_empty() {
            let h = hash_pod_slice(&self.pending_instances);
            if h != self.prev_rect_hash {
                self.rect_pipeline
                    .instances
                    .ensure_capacity(&self.device, self.pending_instances.len());
                self.queue.write_buffer(
                    &self.rect_pipeline.instances.instances_buffer,
                    0,
                    bytemuck::cast_slice(&self.pending_instances),
                );
                self.prev_rect_hash = h;
            }
        } else {
            self.prev_rect_hash = 0;
        }

        if !self.pending_text_instances.is_empty() {
            let h = hash_pod_slice(&self.pending_text_instances);
            if h != self.prev_text_hash {
                self.text_pipeline
                    .instances
                    .ensure_capacity(&self.device, self.pending_text_instances.len());
                self.queue.write_buffer(
                    &self.text_pipeline.instances.instances_buffer,
                    0,
                    bytemuck::cast_slice(&self.pending_text_instances),
                );
                self.prev_text_hash = h;
            }
        } else {
            self.prev_text_hash = 0;
        }

        if !self.pending_line_instances.is_empty() {
            let h = hash_pod_slice(&self.pending_line_instances);
            if h != self.prev_line_hash {
                self.line_pipeline
                    .instances
                    .ensure_capacity(&self.device, self.pending_line_instances.len());
                self.queue.write_buffer(
                    &self.line_pipeline.instances.instances_buffer,
                    0,
                    bytemuck::cast_slice(&self.pending_line_instances),
                );
                self.prev_line_hash = h;
            }
        } else {
            self.prev_line_hash = 0;
        }

        if !self.pending_image_instances.is_empty() {
            let h = hash_pod_slice(&self.pending_image_instances);
            if h != self.prev_image_hash {
                self.image_pipeline
                    .instances
                    .ensure_capacity(&self.device, self.pending_image_instances.len());
                self.queue.write_buffer(
                    &self.image_pipeline.instances.instances_buffer,
                    0,
                    bytemuck::cast_slice(&self.pending_image_instances),
                );
                self.prev_image_hash = h;
            }
        } else {
            self.prev_image_hash = 0;
        }

        if !self.pending_path_vertices.is_empty() {
            self.path_pipeline.ensure_capacity(
                &self.device,
                self.pending_path_vertices.len(),
                self.pending_path_indices.len(),
            );
            self.queue.write_buffer(
                &self.path_pipeline.vertex_buffer,
                0,
                bytemuck::cast_slice(&self.pending_path_vertices),
            );
            self.queue.write_buffer(
                &self.path_pipeline.index_buffer,
                0,
                bytemuck::cast_slice(&self.pending_path_indices),
            );
        }

        if !self.pending_path_fill_data.is_empty() {
            self.path_pipeline
                .fill_data
                .ensure_capacity(&self.device, self.pending_path_fill_data.len());
            self.queue.write_buffer(
                &self.path_pipeline.fill_data.buffer,
                0,
                bytemuck::cast_slice(&self.pending_path_fill_data),
            );
        }

        Ok(false)
    }

    fn resolve_shadows(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let has_text_shadows = self
            .pending_shadows
            .iter()
            .any(|op| matches!(op.kind, ShadowKind::Text { .. }))
            && !self.pending_shadow_instances.is_empty();
        let has_path_shadows = self
            .pending_shadows
            .iter()
            .any(|op| matches!(op.kind, ShadowKind::Path { .. }));

        let shadow_results: Vec<Option<wgpu::BindGroup>> = if has_text_shadows || has_path_shadows {
            // Reuse the retained shadow instance buffer + bind group when the instance data is unchanged; otherwise (re)create and cache them. This avoids a create_buffer_init + create_bind_group round-trip every frame for static shadows.
            let shadow_instances_bg_opt = if has_text_shadows {
                let instances_hash = hash_pod_slice(&self.pending_shadow_instances);
                let cache_valid = self
                    .shadow_instances_cache
                    .as_ref()
                    .is_some_and(|(h, _, _)| *h == instances_hash);
                if !cache_valid {
                    let buf = self
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("telar-shadow-instances"),
                            contents: bytemuck::cast_slice(&self.pending_shadow_instances),
                            usage: wgpu::BufferUsages::STORAGE,
                        });
                    let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("telar-shadow-instances-bg"),
                        layout: &self.text_pipeline.instances.instances_bind_group_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: buf.as_entire_binding(),
                        }],
                    });
                    self.shadow_instances_cache = Some((instances_hash, buf, bg));
                }
                // create_bind_group returns an owned Arc-backed handle, so clone to hand a copy to the draw loop while keeping the cached one.
                self.shadow_instances_cache
                    .as_ref()
                    .map(|(_, _, bg)| bg.clone())
            } else {
                None
            };

            let shadow_path_vb_opt = if has_path_shadows {
                Some(
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("telar-shadow-path-vb"),
                            contents: bytemuck::cast_slice(&self.pending_shadow_path_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                )
            } else {
                None
            };
            let shadow_path_ib_opt = if has_path_shadows {
                Some(
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("telar-shadow-path-ib"),
                            contents: bytemuck::cast_slice(&self.pending_shadow_path_indices),
                            usage: wgpu::BufferUsages::INDEX,
                        }),
                )
            } else {
                None
            };
            let shadow_path_fd_bg_opt = if has_path_shadows {
                let fd_buf = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("telar-shadow-path-fd"),
                        contents: bytemuck::cast_slice(&self.pending_shadow_path_fill_data),
                        usage: wgpu::BufferUsages::STORAGE,
                    });
                let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("telar-shadow-path-fd-bg"),
                    layout: &self.path_pipeline.fill_data.bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: fd_buf.as_entire_binding(),
                    }],
                });
                Some(bg)
            } else {
                None
            };

            let mut results: Vec<Option<wgpu::BindGroup>> =
                Vec::with_capacity(self.pending_shadows.len());

            // Hoist the shared geometry bind groups/buffers out of the loop so both shadow kinds can read them. They are only created when the corresponding shadow kind is present.
            let shadow_instances_bg = shadow_instances_bg_opt;
            let shadow_path_vb = shadow_path_vb_opt;
            let shadow_path_ib = shadow_path_ib_opt;
            let shadow_path_fd_bg = shadow_path_fd_bg_opt;

            for op in &self.pending_shadows {
                let key = match &op.kind {
                    ShadowKind::Text {
                        instance_start,
                        instance_end,
                    } => {
                        let instance_count = instance_end - instance_start;
                        let instances_hash = hash_pod_slice(
                            &self.pending_shadow_instances
                                [*instance_start as usize..*instance_end as usize],
                        );
                        ShadowCacheKey {
                            kind: ShadowCacheKind::Text {
                                instance_start: *instance_start,
                                instance_count,
                                instances_hash,
                            },
                            sigma_bits: op.sigma.to_bits(),
                            texture_width: op.texture_width,
                            texture_height: op.texture_height,
                        }
                    }
                    ShadowKind::Path {
                        index_start,
                        index_end,
                    } => {
                        let index_count = index_end - index_start;
                        let geometry_hash = {
                            let verts = &self.pending_shadow_path_vertices;
                            let idxs = &self.pending_shadow_path_indices
                                [*index_start as usize..*index_end as usize];
                            let h = hash_pod_slice(verts);
                            let mut hasher = FxHasher::default();
                            h.hash(&mut hasher);
                            hash_pod_slice(idxs).hash(&mut hasher);
                            hasher.finish()
                        };
                        ShadowCacheKey {
                            kind: ShadowCacheKind::Path {
                                index_start: *index_start,
                                index_count,
                                geometry_hash,
                            },
                            sigma_bits: op.sigma.to_bits(),
                            texture_width: op.texture_width,
                            texture_height: op.texture_height,
                        }
                    }
                };

                // Cloned out of the cache — a `TextureView` is an `Arc` handle, so this names the same GPU object —
                // because the bind group is built from `self.composite_pipeline`, and the cache's borrow cannot be
                // held across that.
                let cached_view = crate::caches::with_shared(|caches| {
                    caches
                        .shadow_resolved
                        .get(&key)
                        .map(|(_, view)| view.clone())
                })
                .flatten();
                if let Some(cached_view) = cached_view {
                    let cbw = bucket_size(op.texture_width);
                    let cbh = bucket_size(op.texture_height);
                    let bg = self.composite_pipeline.create_bind_group(
                        &self.device,
                        &self.queue,
                        &cached_view,
                        op.dest,
                        1.0,
                        0.0,
                        [
                            op.texture_width as f32 / cbw as f32,
                            op.texture_height as f32 / cbh as f32,
                        ],
                    );
                    results.push(Some(bg));
                    continue;
                }

                let cap_bucket_w = bucket_size(op.texture_width);
                let cap_bucket_h = bucket_size(op.texture_height);
                let (cap_msaa_texture, cap_msaa_view, cap_resolve_texture, cap_resolve_view) =
                    if let Some(pos) =
                        self.shadow_capture_pool
                            .iter()
                            .position(|p: &PooledTexture| {
                                p.bucket_width == cap_bucket_w && p.bucket_height == cap_bucket_h
                            })
                    {
                        let p = self.shadow_capture_pool.remove(pos);
                        (
                            p.msaa_texture,
                            p.msaa_view,
                            p.resolve_texture,
                            p.resolve_view,
                        )
                    } else {
                        self.layer_pipeline.create_layer_textures(
                            &self.device,
                            cap_bucket_w,
                            cap_bucket_h,
                        )
                    };

                // Use bucket dimensions: shadow-texture vertices are 0-based local coords, so the viewport size must match the physical texture, not the logical one.
                let vp_data = Viewport::new(
                    [cap_bucket_w as f32, cap_bucket_h as f32],
                    [0.0, 0.0],
                    self.scale_factor,
                );
                let vp_buf = crate::primitives::create_viewport_buffer(
                    &self.device,
                    "telar-shadow-vp",
                    &vp_data,
                );
                let shadow_vp_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("telar-shadow-vp-bg"),
                    layout: &self.viewport_bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: vp_buf.as_entire_binding(),
                    }],
                });

                {
                    let cap_draw_view = if self.msaa_samples > 1 {
                        &cap_msaa_view
                    } else {
                        &cap_resolve_view
                    };
                    let cap_resolve_opt = if self.msaa_samples > 1 {
                        Some(&cap_resolve_view)
                    } else {
                        None
                    };
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("telar-shadow-capture"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: cap_draw_view,
                            resolve_target: cap_resolve_opt,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: if self.msaa_samples > 1 && cap_resolve_opt.is_some() {
                                    wgpu::StoreOp::Discard
                                } else {
                                    wgpu::StoreOp::Store
                                },
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                        multiview_mask: None,
                    });
                    match &op.kind {
                        ShadowKind::Text {
                            instance_start,
                            instance_end,
                        } => {
                            let shadow_instances_bg = shadow_instances_bg.as_ref().unwrap();
                            pass.set_pipeline(&self.text_pipeline.pipeline);
                            pass.set_bind_group(0, &shadow_vp_bg, &[]);
                            pass.set_bind_group(1, shadow_instances_bg, &[]);
                            pass.set_bind_group(2, &self.text_pipeline.atlas_bind_group, &[]);
                            pass.draw(0..6, *instance_start..*instance_end);
                        }
                        ShadowKind::Path {
                            index_start,
                            index_end,
                        } => {
                            let shadow_path_vb = shadow_path_vb.as_ref().unwrap();
                            let shadow_path_ib = shadow_path_ib.as_ref().unwrap();
                            let shadow_path_fd_bg = shadow_path_fd_bg.as_ref().unwrap();
                            pass.set_pipeline(&self.path_pipeline.pipeline);
                            pass.set_bind_group(0, &shadow_vp_bg, &[]);
                            pass.set_bind_group(1, shadow_path_fd_bg, &[]);
                            pass.set_vertex_buffer(0, shadow_path_vb.slice(..));
                            pass.set_index_buffer(
                                shadow_path_ib.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.draw_indexed(*index_start..*index_end, 0, 0..1);
                        }
                    }
                }

                let (blurred_texture, blurred_view) = self.blur_pipeline.apply(
                    &self.device,
                    &mut *encoder,
                    &cap_resolve_view,
                    cap_bucket_w,
                    cap_bucket_h,
                    op.sigma,
                );
                let shadow_uv_scale = [
                    op.texture_width as f32 / cap_bucket_w as f32,
                    op.texture_height as f32 / cap_bucket_h as f32,
                ];
                let bg = self.composite_pipeline.create_bind_group(
                    &self.device,
                    &self.queue,
                    &blurred_view,
                    op.dest,
                    1.0,
                    0.0,
                    shadow_uv_scale,
                );
                results.push(Some(bg));
                crate::caches::with_shared(|caches| {
                    caches
                        .shadow_resolved
                        .insert(key, (blurred_texture, blurred_view))
                });
                self.shadow_capture_pool.push(PooledTexture {
                    msaa_texture: cap_msaa_texture,
                    msaa_view: cap_msaa_view,
                    resolve_texture: cap_resolve_texture,
                    resolve_view: cap_resolve_view,
                    bucket_width: cap_bucket_w,
                    bucket_height: cap_bucket_h,
                });
            }

            // Shadow passes are recorded into the shared `encoder` and submitted with the main pass; no separate submit here.
            results
        } else {
            Vec::new()
        };

        let mut shadow_results = shadow_results;
        for step in &mut self.pending_steps {
            if let DrawStep::ShadowPlaceholder { op_index } = step {
                if let Some(entry) = shadow_results.get_mut(*op_index) {
                    if let Some(bg) = entry.take() {
                        *step = DrawStep::CompositeShadow { bind_group: bg };
                    }
                }
            }
        }
    }

    fn build_segments(
        &mut self,
        ctx: &mut FrameCtx,
    ) -> Result<(Vec<DrawStep>, Vec<Segment>), RendererError> {
        // Image-batching pre-pass: stable-sort each run of consecutive ImageBatch steps by (id, filter) so non-adjacent draws of the same image become adjacent. This is safe for z-order because the reorder is confined to a single run with no intervening non-image steps.
        {
            let steps = &mut self.pending_steps;
            let mut i = 0;
            while i < steps.len() {
                if matches!(steps[i], DrawStep::ImageBatch { .. }) {
                    let mut j = i + 1;
                    while j < steps.len() && matches!(steps[j], DrawStep::ImageBatch { .. }) {
                        j += 1;
                    }
                    if j - i > 1 {
                        // ImageFilter is not Ord; map it to a u8 for sorting.
                        let filter_ord = |f: ImageFilter| match f {
                            ImageFilter::Nearest => 0u8,
                            ImageFilter::Linear => 1u8,
                        };
                        steps[i..j].sort_by(|a, b| {
                            let (ka, kb) = match (a, b) {
                                (
                                    DrawStep::ImageBatch { key: ka, .. },
                                    DrawStep::ImageBatch { key: kb, .. },
                                ) => (*ka, *kb),
                                _ => unreachable!(),
                            };
                            (ka.0, filter_ord(ka.1)).cmp(&(kb.0, filter_ord(kb.1)))
                        });
                    }
                    i = j;
                } else {
                    i += 1;
                }
            }
        }

        self.merge_opaque_batches();

        // Windowed: the swapchain texture. Headless: the persistent offscreen target that read_rgba later copies from.
        let surface_view = match ctx.output.as_ref() {
            Some(o) => o
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
            None => self
                .offscreen_output
                .as_ref()
                .ok_or_else(|| {
                    RendererError::Backend("offscreen_output missing in headless mode".into())
                })?
                .create_view(&wgpu::TextureViewDescriptor::default()),
        };
        ctx.surface_view = Some(surface_view);

        // Under direct-to-surface the main target IS the presentation texture (swapchain windowed, offscreen headless), so every existing `msaa_view` reference (top-level draw passes and layer composites) renders straight to it; the trailing copy/resolve is then skipped.
        let msaa_view = if ctx.direct_to_surface {
            match ctx.output.as_ref() {
                Some(o) => o
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default()),
                None => self
                    .offscreen_output
                    .as_ref()
                    .ok_or_else(|| {
                        RendererError::Backend("offscreen_output missing in headless mode".into())
                    })?
                    .create_view(&wgpu::TextureViewDescriptor::default()),
            }
        } else {
            self.msaa_texture
                .as_ref()
                .ok_or_else(|| {
                    RendererError::Backend(
                        "msaa_texture not initialized; call reconfigure first".into(),
                    )
                })?
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        ctx.msaa_view = Some(msaa_view);

        ctx.retained_view = Some(self.retained_view.clone().ok_or_else(|| {
            RendererError::Backend("retained_view not initialized; call begin_frame first".into())
        })?);

        let mut steps = std::mem::take(&mut self.pending_steps);
        let mut segments: Vec<Segment> = Vec::new();
        // Walk steps emitting Segment::Draw with index ranges; extract layer-boundary steps in place via std::mem::replace to avoid moving ownership-bearing variants.
        let mut current_start: usize = 0;
        for i in 0..steps.len() {
            let is_boundary = matches!(
                steps[i],
                DrawStep::BeginLayer { .. }
                    | DrawStep::EndLayerComposite { .. }
                    | DrawStep::PrerenderedLayer { .. }
            );
            if !is_boundary {
                continue;
            }
            if i > current_start {
                segments.push(Segment::Draw {
                    start: current_start,
                    end: i,
                });
            }
            let taken = std::mem::replace(&mut steps[i], DrawStep::SetScissor { rect: None });
            match taken {
                DrawStep::BeginLayer {
                    msaa_texture,
                    msaa_view: lmv,
                    resolve_texture,
                    resolve_view: lrv,
                    viewport_bind_group,
                    width,
                    height,
                    offset_x,
                    offset_y,
                    backdrop_blur,
                } => {
                    segments.push(Segment::BeginLayer {
                        msaa_texture,
                        msaa_view: lmv,
                        resolve_texture,
                        resolve_view: lrv,
                        viewport_bind_group,
                        width,
                        height,
                        offset_x,
                        offset_y,
                        backdrop_blur,
                    });
                }
                DrawStep::EndLayerComposite {
                    bind_group,
                    cache_hash,
                    scissor,
                } => {
                    segments.push(Segment::EndLayerComposite {
                        bind_group,
                        cache_hash,
                        scissor,
                    });
                }
                DrawStep::PrerenderedLayer {
                    bind_group,
                    scissor,
                } => {
                    segments.push(Segment::PrerenderedLayer {
                        bind_group,
                        scissor,
                    });
                }
                _ => unreachable!(),
            }
            current_start = i + 1;
        }
        if current_start < steps.len() {
            segments.push(Segment::Draw {
                start: current_start,
                end: steps.len(),
            });
        }
        Ok((steps, segments))
    }

    fn execute_segments(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ctx: &mut FrameCtx,
        steps: &[DrawStep],
        segments: Vec<Segment>,
    ) -> Result<(), RendererError> {
        // Views are owned clones (cheap Arc bumps) so no borrow of `ctx` is held across the `ctx.frame_scratch_textures` writes below.
        let msaa_view = ctx
            .msaa_view
            .clone()
            .expect("msaa_view set in build_segments");
        let retained_view = ctx
            .retained_view
            .clone()
            .expect("retained_view set in build_segments");
        let load_op = ctx.load_op;
        let dirty_scissor = ctx.dirty_scissor;
        let prime = ctx.prime;
        let prime_delta = ctx.prime_delta;

        // The top-level target needs `load_op` (usually a full-screen Clear) applied once before anything Loads it. A dedicated no-draw init pass costs a full-screen tile store+load every frame on tiled mobile GPUs; when the first segment is itself a top-level Draw, fold the clear into that pass instead. Gated to the single-sample (mobile tiler) path: immediate-mode desktop GPUs gain little and keep the simpler explicit-init pass. Falls back to the standalone init pass when the frame opens with a layer (nothing draws to the top-level target first).
        // Prime the offscreen with the retained previous frame translated by prime_delta before the main pass Loads it: scroll-blit-with-clear (delta = scroll, redraw only the exposed band) or F1 damage priming (delta = 0, redraw only the dirty scissor). The clear (load_op) is fully covered by the full-screen quad, so its value is irrelevant on primed frames.
        let prime_bind_group = if prime {
            let logical_w = self.width as f32 / self.scale_factor;
            let logical_h = self.height as f32 / self.scale_factor;
            Some(self.composite_pipeline.create_bind_group(
                &self.device,
                &self.queue,
                &retained_view,
                [prime_delta.0, prime_delta.1, logical_w, logical_h],
                1.0,
                0.0,
                [1.0, 1.0],
            ))
        } else {
            None
        };

        let fold_init_clear =
            self.msaa_samples == 1 && matches!(segments.first(), Some(Segment::Draw { .. }));
        if !fold_init_clear {
            let mut init = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("telar-main-init"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            if let Some(ref bg) = prime_bind_group {
                init.set_pipeline(&self.composite_pipeline.pipeline);
                init.set_bind_group(0, &self.viewport_bind_group, &[]);
                init.set_bind_group(1, bg, &[]);
                init.draw(0..6, 0..1);
            }
        }

        let mut layer_stack: Vec<(
            wgpu::Texture,
            wgpu::TextureView, // msaa view (render target)
            wgpu::Texture,
            wgpu::TextureView, // resolve view
            wgpu::BindGroup,   // per-layer viewport bind group
            u32,               // layer texture width
            u32,               // layer texture height
        )> = Vec::new();
        // Marks draw segments preceding EndLayerComposite to inline MSAA resolve into the drawing pass, skipping the dedicated resolve pass.
        let mut inline_resolve_targets: Vec<bool> = vec![false; segments.len()];
        for i in 0..segments.len() {
            if let (Segment::Draw { .. }, Some(Segment::EndLayerComposite { .. })) =
                (&segments[i], segments.get(i + 1))
            {
                inline_resolve_targets[i] = true;
            }
        }

        let mut endlayer_resolve_done: Vec<bool> = vec![false; segments.len()];
        for i in 0..segments.len() {
            if matches!(segments[i], Segment::EndLayerComposite { .. })
                && i > 0
                && inline_resolve_targets[i - 1]
            {
                endlayer_resolve_done[i] = true;
            }
        }

        for (seg_idx, segment) in segments.into_iter().enumerate() {
            match segment {
                Segment::Draw { start, end } => {
                    let draw_steps = &steps[start..end];
                    let inline_resolve = inline_resolve_targets[seg_idx];
                    let attach_view: &wgpu::TextureView =
                        if let Some((_, lmv, _, lrv, _, _, _)) = layer_stack.last() {
                            if self.msaa_samples > 1 { lmv } else { lrv }
                        } else {
                            &msaa_view
                        };
                    let resolve_view_opt: Option<&wgpu::TextureView> =
                        if inline_resolve && self.msaa_samples > 1 {
                            layer_stack.last().map(|(_, _, _, rv, _, _, _)| rv)
                        } else {
                            None
                        };
                    // Scissors must be clamped to the attachment actually bound below, which inside a layer is
                    // that layer's texture — smaller than the surface. Clamping to the surface instead lets a
                    // clip nested in a layer name pixels outside the attachment, which wgpu rejects as fatal.
                    let (target_w, target_h) = layer_stack
                        .last()
                        .map(|l| (l.5, l.6))
                        .unwrap_or((self.width, self.height));

                    // When the init pass was folded away (first segment is this top-level Draw), apply the frame's clear here instead of Loading an uninitialised target.
                    let pass_load = if seg_idx == 0 && fold_init_clear {
                        load_op
                    } else {
                        wgpu::LoadOp::Load
                    };
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("telar-render-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: attach_view,
                            resolve_target: resolve_view_opt,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: pass_load,
                                store: if resolve_view_opt.is_some() {
                                    wgpu::StoreOp::Discard // MSAA samples not needed after inline resolve
                                } else {
                                    wgpu::StoreOp::Store
                                },
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                        multiview_mask: None,
                    });

                    let active_vp_bg = if let Some((_, _, _, _, vp_bg, _, _)) = layer_stack.last() {
                        vp_bg
                    } else {
                        &self.viewport_bind_group
                    };
                    render_pass.set_bind_group(0, active_vp_bg, &[]);

                    for step in draw_steps {
                        match step {
                            DrawStep::RectBatch { start, end } => {
                                render_pass.set_pipeline(&self.rect_pipeline.pipeline);
                                render_pass.set_bind_group(
                                    1,
                                    &self.rect_pipeline.instances.instances_bind_group,
                                    &[],
                                );
                                render_pass.draw(0..6, *start..*end);
                            }
                            DrawStep::TextBatch { start, end } => {
                                render_pass.set_pipeline(&self.text_pipeline.pipeline);
                                render_pass.set_bind_group(
                                    1,
                                    &self.text_pipeline.instances.instances_bind_group,
                                    &[],
                                );
                                render_pass.set_bind_group(
                                    2,
                                    &self.text_pipeline.atlas_bind_group,
                                    &[],
                                );
                                render_pass.draw(0..6, *start..*end);
                            }
                            DrawStep::LineBatch { start, end } => {
                                render_pass.set_pipeline(&self.line_pipeline.pipeline);
                                render_pass.set_bind_group(
                                    1,
                                    &self.line_pipeline.instances.instances_bind_group,
                                    &[],
                                );
                                render_pass.draw(0..6, *start..*end);
                            }
                            DrawStep::ImageBatch {
                                start,
                                end,
                                bind_group,
                                key: _,
                            } => {
                                render_pass.set_pipeline(&self.image_pipeline.pipeline);
                                render_pass.set_bind_group(
                                    1,
                                    &self.image_pipeline.instances.instances_bind_group,
                                    &[],
                                );
                                render_pass.set_bind_group(2, bind_group, &[]);
                                render_pass.draw(0..6, *start..*end);
                            }
                            DrawStep::PathDraw {
                                index_start,
                                index_end,
                            } => {
                                render_pass.set_pipeline(&self.path_pipeline.pipeline);
                                render_pass.set_bind_group(
                                    1,
                                    &self.path_pipeline.fill_data.bind_group,
                                    &[],
                                );
                                render_pass.set_vertex_buffer(
                                    0,
                                    self.path_pipeline.vertex_buffer.slice(..),
                                );
                                render_pass.set_index_buffer(
                                    self.path_pipeline.index_buffer.slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );
                                render_pass.draw_indexed(*index_start..*index_end, 0, 0..1);
                            }
                            DrawStep::SetScissor { rect } => {
                                let clipped_rect = match (*rect, dirty_scissor) {
                                    (r, None) => r,
                                    (None, Some(ds)) => Some(ds),
                                    (Some(r), Some(ds)) => r.intersect(ds).or(Some(r)),
                                };
                                match clipped_rect {
                                    None => {
                                        render_pass.set_scissor_rect(0, 0, target_w, target_h);
                                    }
                                    Some(r) => {
                                        let (x, y, w, h) = physical_scissor(
                                            r,
                                            target_w,
                                            target_h,
                                            self.scale_factor,
                                        );
                                        render_pass.set_scissor_rect(x, y, w, h);
                                    }
                                }
                            }
                            DrawStep::SetShaderClip {
                                viewport_bind_group,
                            } => {
                                render_pass.set_bind_group(0, viewport_bind_group, &[]);
                            }
                            DrawStep::CompositeShadow { bind_group } => {
                                render_pass.set_pipeline(&self.composite_pipeline.pipeline);
                                render_pass.set_bind_group(1, bind_group, &[]);
                                render_pass.draw(0..6, 0..1);
                            }
                            DrawStep::ShadowPlaceholder { .. } => {}
                            DrawStep::BeginLayer { .. }
                            | DrawStep::EndLayerComposite { .. }
                            | DrawStep::PrerenderedLayer { .. } => {
                                unreachable!("layer boundaries are split into segments")
                            }
                        }
                    }
                }

                Segment::BeginLayer {
                    msaa_texture,
                    msaa_view: layer_msaa_view,
                    resolve_texture,
                    resolve_view,
                    viewport_bind_group,
                    width,
                    height,
                    offset_x,
                    offset_y,
                    backdrop_blur,
                } => {
                    {
                        let clear_target = if self.msaa_samples > 1 {
                            &layer_msaa_view
                        } else {
                            &resolve_view
                        };
                        let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("telar-layer-clear"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: clear_target,
                                resolve_target: None,
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color {
                                        r: 0.0,
                                        g: 0.0,
                                        b: 0.0,
                                        a: 0.0,
                                    }),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            occlusion_query_set: None,
                            timestamp_writes: None,
                            multiview_mask: None,
                        });
                    }

                    if backdrop_blur > 0.0 {
                        // Always sample from the root (main) MSAA for backdrop blur. Any layers above this point in the stack (e.g. a rounded-clip mini-layer) are transparent at this moment — blurring their content would yield nothing. The root MSAA has the fully-rendered app content that the blur should sample.
                        let (parent_w, parent_h) = (self.width, self.height);
                        let parent_msaa_view: &wgpu::TextureView = &msaa_view;

                        // Superset usage so any pooled texture of this size/format can serve as either the resolve target or the crop destination interchangeably.
                        let scratch_usage = wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::COPY_SRC
                            | wgpu::TextureUsages::COPY_DST
                            | wgpu::TextureUsages::TEXTURE_BINDING;
                        let temp_resolve_entry = take_pooled_texture(
                            &self.device,
                            &mut self.texture_pool,
                            parent_w.max(1),
                            parent_h.max(1),
                            self.surface_format,
                            "telar-backdrop-resolve",
                            scratch_usage,
                        );
                        let temp_resolve = &temp_resolve_entry.3;
                        let temp_resolve_view = &temp_resolve_entry.4;

                        if self.msaa_samples > 1 {
                            let _resolve = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("telar-backdrop-parent-resolve"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: parent_msaa_view,
                                    resolve_target: Some(temp_resolve_view),
                                    depth_slice: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        // Store: the parent MSAA is still needed after this resolve so EndLayerComposite can load it to composite the layer on top. Discard here caused a black screen on immediate-mode GPUs (desktop).
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                occlusion_query_set: None,
                                timestamp_writes: None,
                                multiview_mask: None,
                            });
                        } else {
                            encoder.copy_texture_to_texture(
                                wgpu::TexelCopyTextureInfo {
                                    texture: self.msaa_texture.as_ref().unwrap(),
                                    mip_level: 0,
                                    origin: wgpu::Origin3d::ZERO,
                                    aspect: wgpu::TextureAspect::All,
                                },
                                wgpu::TexelCopyTextureInfo {
                                    texture: temp_resolve,
                                    mip_level: 0,
                                    origin: wgpu::Origin3d::ZERO,
                                    aspect: wgpu::TextureAspect::All,
                                },
                                wgpu::Extent3d {
                                    width: parent_w,
                                    height: parent_h,
                                    depth_or_array_layers: 1,
                                },
                            );
                        }

                        let ox_px = offset_x.floor().max(0.0) as u32;
                        let oy_px = offset_y.floor().max(0.0) as u32;
                        let crop_w = width.min(parent_w.saturating_sub(ox_px));
                        let crop_h = height.min(parent_h.saturating_sub(oy_px));

                        let cropped_entry = take_pooled_texture(
                            &self.device,
                            &mut self.texture_pool,
                            crop_w.max(1),
                            crop_h.max(1),
                            self.surface_format,
                            "telar-backdrop-crop",
                            scratch_usage,
                        );
                        let cropped = &cropped_entry.3;
                        let cropped_view = &cropped_entry.4;

                        if crop_w > 0 && crop_h > 0 {
                            encoder.copy_texture_to_texture(
                                wgpu::TexelCopyTextureInfo {
                                    texture: temp_resolve,
                                    mip_level: 0,
                                    origin: wgpu::Origin3d {
                                        x: ox_px,
                                        y: oy_px,
                                        z: 0,
                                    },
                                    aspect: wgpu::TextureAspect::All,
                                },
                                wgpu::TexelCopyTextureInfo {
                                    texture: cropped,
                                    mip_level: 0,
                                    origin: wgpu::Origin3d::ZERO,
                                    aspect: wgpu::TextureAspect::All,
                                },
                                wgpu::Extent3d {
                                    width: crop_w,
                                    height: crop_h,
                                    depth_or_array_layers: 1,
                                },
                            );
                        }

                        let (_blurred_tex, blurred_view) = self.blur_pipeline.apply(
                            &self.device,
                            &mut *encoder,
                            cropped_view,
                            crop_w.max(1),
                            crop_h.max(1),
                            backdrop_blur,
                        );

                        let backdrop_bg = self.composite_pipeline.create_bind_group(
                            &self.device,
                            &self.queue,
                            &blurred_view,
                            [offset_x, offset_y, crop_w as f32, crop_h as f32],
                            1.0,
                            0.0,
                            [1.0, 1.0],
                        );
                        {
                            let backdrop_target = if self.msaa_samples > 1 {
                                &layer_msaa_view
                            } else {
                                &resolve_view
                            };
                            let mut backdrop_pass =
                                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                    label: Some("telar-backdrop-composite"),
                                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                        view: backdrop_target,
                                        resolve_target: None,
                                        depth_slice: None,
                                        ops: wgpu::Operations {
                                            load: wgpu::LoadOp::Load,
                                            store: wgpu::StoreOp::Store,
                                        },
                                    })],
                                    depth_stencil_attachment: None,
                                    occlusion_query_set: None,
                                    timestamp_writes: None,
                                    multiview_mask: None,
                                });
                            backdrop_pass.set_pipeline(&self.composite_pipeline.pipeline);
                            backdrop_pass.set_bind_group(0, &viewport_bind_group, &[]);
                            backdrop_pass.set_bind_group(1, &backdrop_bg, &[]);
                            backdrop_pass.draw(0..6, 0..1);
                        }
                        // Hold these scratch textures until after submit; returning them to the pool now would let a later layer in this same encoder reuse and overwrite them before the GPU reads them.
                        ctx.frame_scratch_textures.push(temp_resolve_entry);
                        ctx.frame_scratch_textures.push(cropped_entry);
                    }

                    layer_stack.push((
                        msaa_texture,
                        layer_msaa_view,
                        resolve_texture,
                        resolve_view,
                        viewport_bind_group,
                        width,
                        height,
                    ));
                }

                Segment::EndLayerComposite {
                    bind_group,
                    cache_hash,
                    scissor,
                } => {
                    let (l_msaa_tex, l_msaa_view, l_resolve_tex, l_resolve_view, _, lw, lh) =
                        layer_stack
                            .pop()
                            .expect("layer_stack underflow on EndLayerComposite");

                    // When msaa_samples==1, draws already targeted resolve_view directly so no resolve pass is needed.
                    if !endlayer_resolve_done[seg_idx] && self.msaa_samples > 1 {
                        let _resolve = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("telar-layer-resolve"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &l_msaa_view,
                                resolve_target: Some(&l_resolve_view),
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Discard, // MSAA samples not needed after resolve
                                },
                            })],
                            depth_stencil_attachment: None,
                            occlusion_query_set: None,
                            timestamp_writes: None,
                            multiview_mask: None,
                        });
                    }

                    // When msaa_samples==1 (Android) draws target the resolve view (tuple index 3), not the MSAA view (index 1); using the wrong view causes composited content to land on a texture the outer layer never reads, making nested layers disappear.
                    let parent_view: &wgpu::TextureView =
                        if let Some((_, lmv, _, lrv, _, _, _)) = layer_stack.last() {
                            if self.msaa_samples > 1 { lmv } else { lrv }
                        } else {
                            &msaa_view
                        };

                    // composite_pipeline must be used here (not layer_pipeline): its BGL expects viewport at set 0 and composite params at set 1, incompatible with layer_pipeline's single-set layout.
                    let parent_vp_bg: &wgpu::BindGroup =
                        if let Some((_, _, _, _, vp_bg, _, _)) = layer_stack.last() {
                            vp_bg
                        } else {
                            &self.viewport_bind_group
                        };
                    // The layer being composited is already popped, so this is the PARENT — the attachment the
                    // blit writes into, and therefore what its scissor must be clamped against.
                    let (parent_w, parent_h) = layer_stack
                        .last()
                        .map(|l| (l.5, l.6))
                        .unwrap_or((self.width, self.height));

                    {
                        let mut blit = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("telar-layer-blit"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: parent_view,
                                resolve_target: None,
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            occlusion_query_set: None,
                            timestamp_writes: None,
                            multiview_mask: None,
                        });
                        blit.set_pipeline(&self.composite_pipeline.pipeline);
                        blit.set_bind_group(0, parent_vp_bg, &[]);
                        blit.set_bind_group(1, &bind_group, &[]);
                        // Confine only a TOP-LEVEL composite (parent is the main target) to the dirty
                        // rect; a nested composite writes into a parent layer in that layer's own
                        // coordinate space, where the window-space dirty rect does not apply.
                        let composite_scissor = if layer_stack.is_empty() {
                            confine_to_dirty(scissor, dirty_scissor)
                        } else {
                            scissor
                        };
                        if let Some(s) = composite_scissor {
                            let (x, y, w, h) =
                                physical_scissor(s, parent_w, parent_h, self.scale_factor);
                            blit.set_scissor_rect(x, y, w, h);
                        }
                        blit.draw(0..6, 0..1);
                    }

                    if let Some(hash) = cache_hash {
                        // Retain the resolved texture so the next frame can composite it directly. The MSAA half is not cacheable (it is consumed by the resolve), so it drops instead of returning to the pool.
                        let pixel_count = lw as u64 * lh as u64;
                        self.layer_resolved_cache
                            .insert(hash, (l_resolve_tex, l_resolve_view, pixel_count));
                        self.layer_resolved_cache_order.push_back(hash);
                        let mut total_pixels: u64 = self
                            .layer_resolved_cache
                            .values()
                            .map(|(_, _, px)| *px)
                            .sum();
                        while total_pixels > self.layer_cache_pixel_budget {
                            match self.layer_resolved_cache_order.pop_front() {
                                Some(oldest) if oldest == hash => {
                                    // Never evict the entry we just inserted; if it alone exceeds the budget keep it for this frame.
                                    self.layer_resolved_cache_order.push_front(oldest);
                                    break;
                                }
                                Some(oldest) => {
                                    if let Some((_, _, px)) =
                                        self.layer_resolved_cache.remove(&oldest)
                                    {
                                        total_pixels = total_pixels.saturating_sub(px);
                                    }
                                }
                                None => break,
                            }
                        }
                    } else {
                        self.layer_texture_pool.push(PooledTexture {
                            msaa_texture: l_msaa_tex,
                            msaa_view: l_msaa_view,
                            resolve_texture: l_resolve_tex,
                            resolve_view: l_resolve_view,
                            bucket_width: lw,
                            bucket_height: lh,
                        });
                    }
                }

                Segment::PrerenderedLayer {
                    bind_group,
                    scissor,
                } => {
                    // Composite the cached layer texture onto the current target (parent layer or surface) without rendering the layer content.
                    let parent_view: &wgpu::TextureView =
                        if let Some((_, lmv, _, lrv, _, _, _)) = layer_stack.last() {
                            if self.msaa_samples > 1 { lmv } else { lrv }
                        } else {
                            &msaa_view
                        };
                    let parent_vp_bg: &wgpu::BindGroup =
                        if let Some((_, _, _, _, vp_bg, _, _)) = layer_stack.last() {
                            vp_bg
                        } else {
                            &self.viewport_bind_group
                        };
                    // The layer being composited is already popped, so this is the PARENT — the attachment the
                    // blit writes into, and therefore what its scissor must be clamped against.
                    let (parent_w, parent_h) = layer_stack
                        .last()
                        .map(|l| (l.5, l.6))
                        .unwrap_or((self.width, self.height));
                    let mut blit = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("telar-prerendered-layer-blit"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: parent_view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                        multiview_mask: None,
                    });
                    blit.set_pipeline(&self.composite_pipeline.pipeline);
                    blit.set_bind_group(0, parent_vp_bg, &[]);
                    blit.set_bind_group(1, &bind_group, &[]);
                    let composite_scissor = if layer_stack.is_empty() {
                        confine_to_dirty(scissor, dirty_scissor)
                    } else {
                        scissor
                    };
                    if let Some(s) = composite_scissor {
                        let (x, y, w, h) =
                            physical_scissor(s, parent_w, parent_h, self.scale_factor);
                        blit.set_scissor_rect(x, y, w, h);
                    }
                    blit.draw(0..6, 0..1);
                }
            }
        }
        Ok(())
    }

    /// Draws the composed frame onto the presentation target.
    ///
    /// Clears first when the target is Telar's — a swapchain image is recycled and whatever the last
    /// frame left in it is not this frame's background — and loads when it belongs to the application,
    /// which is what makes composing *into* its picture different from replacing it. The pipeline blends
    /// premultiplied-alpha over either way, so the two differ only in what is underneath.
    fn blit_to_target(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        source: &wgpu::BindGroup,
    ) {
        let mut blit = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("telar-retained-blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: if self.app_owned_target {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        blit.set_pipeline(&self.retained_blit_pipeline.pipeline);
        blit.set_bind_group(0, &self.viewport_bind_group, &[]);
        blit.set_bind_group(1, source, &[]);
        blit.draw(0..6, 0..1);
    }

    fn present(
        &mut self,
        mut encoder: wgpu::CommandEncoder,
        ctx: FrameCtx,
        orig_commands: &[DrawCommand],
    ) -> Result<(), RendererError> {
        let FrameCtx {
            direct_to_surface,
            msaa_view,
            surface_view,
            retained_view,
            output,
            mut frame_scratch_textures,
            ..
        } = ctx;
        let msaa_view = msaa_view.expect("msaa_view set in build_segments");
        let surface_view = surface_view.expect("surface_view set in build_segments");
        let retained_view = retained_view.expect("retained_view set in build_segments");

        if direct_to_surface {
            // Already rendered straight into the swapchain texture; no copy/resolve to the surface needed.
        } else if self.msaa_samples > 1 {
            // Resolve MSAA into retained_view so the idle-blit path has valid content next frame.
            {
                let _final = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("telar-final-resolve"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &msaa_view,
                        resolve_target: Some(&retained_view),
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Discard,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
            }
            let retained_bg = self.retained_blit_pipeline.create_bind_group(
                &self.device,
                &self.queue,
                &retained_view,
                [
                    0.0,
                    0.0,
                    self.width as f32 / self.scale_factor,
                    self.height as f32 / self.scale_factor,
                ],
                1.0,
                0.0,
                [1.0, 1.0],
            );
            self.blit_to_target(&mut encoder, &surface_view, &retained_bg);
        } else if self.app_owned_target {
            // The copy below would replace what the application drew in its texture; blend the frame over it instead. `msaa_texture` carries TEXTURE_BINDING on this single-sample path (see `reconfigure`), so it is sampleable without the resolve step the MSAA branch needs.
            let msaa_source = self
                .msaa_texture
                .as_ref()
                .ok_or_else(|| RendererError::Backend("msaa_texture missing for blit".into()))?
                .create_view(&wgpu::TextureViewDescriptor::default());
            let source_bg = self.retained_blit_pipeline.create_bind_group(
                &self.device,
                &self.queue,
                &msaa_source,
                [
                    0.0,
                    0.0,
                    self.width as f32 / self.scale_factor,
                    self.height as f32 / self.scale_factor,
                ],
                1.0,
                0.0,
                [1.0, 1.0],
            );
            self.blit_to_target(&mut encoder, &surface_view, &source_bg);
        } else {
            // Android (msaa_samples==1): copy directly to surface to avoid alpha-compositing artifacts on Adreno drivers.
            let msaa_tex = self
                .msaa_texture
                .as_ref()
                .ok_or_else(|| RendererError::Backend("msaa_texture missing for copy".into()))?;
            // Windowed: copy into the swapchain texture. Headless: into the offscreen target read_rgba reads.
            let dest_texture: &wgpu::Texture = match output.as_ref() {
                Some(o) => &o.texture,
                None => self.offscreen_output.as_ref().ok_or_else(|| {
                    RendererError::Backend("offscreen_output missing in headless mode".into())
                })?,
            };
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: msaa_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: dest_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
            );
            // The former second full-screen copy (msaa_texture → retained_texture, to feed the idle-blit) is gone: the idle-blit now samples msaa_texture directly, halving the per-active-frame copy_texture_to_texture traffic on Android. retained_texture stays allocated but unused on this path — it is shared with the MSAA>1 (Desktop) branch.
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        // Safe to recycle now: the encoder is submitted, so no in-flight pass within this frame can alias these textures.
        for entry in frame_scratch_textures.drain(..) {
            return_pooled_texture(
                &mut self.texture_pool,
                entry,
                self.max_texture_pool_per_size,
            );
        }

        // Headless has no swapchain: the frame already lives in offscreen_output, so present is a windowed-only no-op.
        if let Some(output) = output {
            tracing::debug!("hw render_frame: presenting {}x{}", self.width, self.height);
            let present_start = renderer_core::perf::now_if_enabled();
            {
                let _swapchain = super::swapchain_lock();
                self.queue.present(output);
            }
            renderer_core::perf::record_since(renderer_core::perf::Phase::Present, present_start);
        }
        // generation already bumps iff content changed (same invariant the idle-blit fast path relies on), so it replaces the per-frame O(n) hash_draw_commands here; the is_empty() guard repopulates prev_commands after a resize cleared it without a content change.
        if self.incoming_generation != self.prev_generation || self.prev_commands.is_empty() {
            // F2: reuse the existing allocation (clear + refill) instead of allocating a fresh Vec each content frame.
            self.prev_commands.clear();
            self.prev_commands.extend_from_slice(orig_commands);
        }
        self.prev_generation = self.incoming_generation;
        self.clear_pending();
        Ok(())
    }
}
