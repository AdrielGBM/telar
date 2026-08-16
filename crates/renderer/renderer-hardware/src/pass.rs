//! One shape for every render pass this renderer opens.

/// Begins a render pass with a single colour attachment and nothing else.
///
/// Every pass here is that: no depth-stencil, no occlusion query, no timestamps, no multiview. Only the
/// label, the target view, an optional resolve target and the load/store ops ever differ, so those are all a
/// caller states — the fifteen descriptors that used to spell out the same five `None`s each cannot drift
/// apart any more.
pub(crate) fn color_pass<'encoder>(
    encoder: &'encoder mut wgpu::CommandEncoder,
    label: &str,
    view: &wgpu::TextureView,
    resolve_target: Option<&wgpu::TextureView>,
    ops: wgpu::Operations<wgpu::Color>,
) -> wgpu::RenderPass<'encoder> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target,
            depth_slice: None,
            ops,
        })],
        depth_stencil_attachment: None,
        occlusion_query_set: None,
        timestamp_writes: None,
        multiview_mask: None,
    })
}

/// The ops for a pass that clears to transparent and keeps the result — the common opening.
pub(crate) fn clear_store() -> wgpu::Operations<wgpu::Color> {
    wgpu::Operations {
        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        store: wgpu::StoreOp::Store,
    }
}

/// The ops for a pass that draws on top of what is already there.
pub(crate) fn load_store() -> wgpu::Operations<wgpu::Color> {
    wgpu::Operations {
        load: wgpu::LoadOp::Load,
        store: wgpu::StoreOp::Store,
    }
}
