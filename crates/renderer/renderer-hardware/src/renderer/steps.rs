use geometry_core::Rect;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::Raster;

use super::HardwareRenderer;

pub(super) enum DrawStep {
    RectBatch {
        start: u32,
        end: u32,
    },
    TextBatch {
        start: u32,
        end: u32,
    },
    LineBatch {
        start: u32,
        end: u32,
    },
    ImageBatch {
        start: u32,
        end: u32,
        bind_group: wgpu::BindGroup,
        key: (u64, Raster),
    },
    PathDraw {
        index_start: u32,
        index_end: u32,
    },
    SetScissor {
        rect: Option<Rect>,
    },
    // Rebinds the viewport uniform (group 0) to one carrying rounded-clip SDF params, so a non-nested rounded PushClip masks corners in-shader instead of allocating a mini-layer.
    SetShaderClip {
        viewport_bind_group: wgpu::BindGroup,
    },
    // Where the pass has to break. Carried as one payload so `build_segments` can move it into a `Segment`
    // instead of repacking it field by field into a second set of variants that has to be kept identical.
    Boundary(Boundary),
    ShadowPlaceholder {
        op_index: usize,
    },
    CompositeShadow {
        bind_group: wgpu::BindGroup,
    },
}

// A point where one render pass ends and another begins: everything the executor needs to open the new pass
// or to composite the finished one.
pub(super) enum Boundary {
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
        // Some(hash) when the resolved layer texture should be cached for reuse next frame; None for layers that must not be cached (backdrop blur, round-clip).
        cache_hash: Option<u64>,
        // Outer scissor to apply during the composite blit, so the layer respects parent clip rects (e.g. scroll area). None = full render target.
        scissor: Option<Rect>,
    },
    // Already-cached layer composited directly without a render pass.
    PrerenderedLayer {
        bind_group: wgpu::BindGroup,
        // Outer scissor to apply during the composite blit, so the layer respects parent clip rects (e.g. scroll area). None = full render target.
        scissor: Option<Rect>,
    },
}

pub(super) struct LayerAccum {
    pub(super) opacity: f32,
    pub(super) backdrop_blur: f32,
    pub(super) begin_step_index: usize,
    pub(super) bounds: Option<Rect>,
    // Index into the commands slice just after the PushLayer (start of layer content).
    pub(super) command_start: usize,
    // Instance buffer lengths captured at PushLayer, used to truncate on a cache hit.
    pub(super) instance_start: u32,
    pub(super) text_instance_start: u32,
    pub(super) line_instance_start: u32,
    pub(super) image_instance_start: u32,
}

impl LayerAccum {
    pub(super) fn extend(&mut self, rect: Rect) {
        self.bounds = renderer_core::extend_bounds(self.bounds, rect);
    }
}

// Tries to merge two consecutive same-type batch steps whose instance index ranges are contiguous. Returns Ok(merged) on success, Err((a, b)) if they cannot be merged.
fn try_merge_steps(a: DrawStep, b: DrawStep) -> Result<DrawStep, (DrawStep, DrawStep)> {
    match (a, b) {
        (DrawStep::RectBatch { start: s, end: e1 }, DrawStep::RectBatch { start: s2, end: e2 })
            if e1 == s2 =>
        {
            Ok(DrawStep::RectBatch { start: s, end: e2 })
        }
        (DrawStep::TextBatch { start: s, end: e1 }, DrawStep::TextBatch { start: s2, end: e2 })
            if e1 == s2 =>
        {
            Ok(DrawStep::TextBatch { start: s, end: e2 })
        }
        (DrawStep::LineBatch { start: s, end: e1 }, DrawStep::LineBatch { start: s2, end: e2 })
            if e1 == s2 =>
        {
            Ok(DrawStep::LineBatch { start: s, end: e2 })
        }
        (a, b) => Err((a, b)),
    }
}

#[inline]
pub(super) fn flush_batch(
    pending_steps: &mut Vec<DrawStep>,
    batch_start: &mut Option<u32>,
    vec_len: u32,
    variant: impl Fn(u32, u32) -> DrawStep,
) {
    if let Some(start) = batch_start.take() {
        if vec_len > start {
            pending_steps.push(variant(start, vec_len));
        }
    }
}

#[inline]
pub(super) fn flush_image_batch(
    pending_steps: &mut Vec<DrawStep>,
    batch_image_start: &mut Option<u32>,
    batch_image_bind_group: &mut Option<wgpu::BindGroup>,
    batch_image_key: &mut Option<(u64, Raster)>,
    pending_image_instances_len: u32,
) {
    if let (Some(start), Some(bind_group), Some(key)) = (
        batch_image_start.take(),
        batch_image_bind_group.take(),
        *batch_image_key,
    ) {
        if pending_image_instances_len > start {
            pending_steps.push(DrawStep::ImageBatch {
                start,
                end: pending_image_instances_len,
                bind_group,
                key,
            });
        }
    }
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> HardwareRenderer<W> {
    // Stable-sorts RectBatch/TextBatch/LineBatch within flat zones (separated by structural markers), then merges consecutive same-type batches with contiguous index ranges. Reduces 2N draw calls for a list of N items (rect+label) down to 2.
    pub(super) fn merge_opaque_batches(&mut self) {
        let steps = &mut self.pending_steps;
        let mut out = std::mem::take(&mut self.merge_out);
        out.clear();
        out.reserve(steps.len());
        let mut zone = std::mem::take(&mut self.merge_zone);
        zone.clear();

        fn flush_zone(zone: &mut Vec<DrawStep>, out: &mut Vec<DrawStep>) {
            if zone.is_empty() {
                return;
            }
            zone.sort_by_key(|s| match s {
                DrawStep::RectBatch { .. } => 0u8,
                DrawStep::LineBatch { .. } => 1,
                DrawStep::TextBatch { .. } => 2,
                _ => 3,
            });
            let mut merged: Option<DrawStep> = None;
            for step in zone.drain(..) {
                match merged.take() {
                    None => merged = Some(step),
                    Some(prev) => match try_merge_steps(prev, step) {
                        Ok(m) => merged = Some(m),
                        Err((a, b)) => {
                            out.push(a);
                            merged = Some(b);
                        }
                    },
                }
            }
            if let Some(last) = merged {
                out.push(last);
            }
        }

        for step in steps.drain(..) {
            match &step {
                DrawStep::RectBatch { .. }
                | DrawStep::TextBatch { .. }
                | DrawStep::LineBatch { .. } => {
                    zone.push(step);
                }
                _ => {
                    flush_zone(&mut zone, &mut out);
                    out.push(step);
                }
            }
        }
        flush_zone(&mut zone, &mut out);

        // Put the merged result in pending_steps and reclaim the (now-empty) former pending_steps buffer as the next merge_out.
        std::mem::swap(&mut self.pending_steps, &mut out);
        self.merge_out = out;
        self.merge_zone = zone;
    }
}
