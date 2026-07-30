use wgpu::Device;

use super::{BlurParams, BlurPipeline};

impl BlurPipeline {
    pub(super) fn apply_gaussian(
        &mut self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        src_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        sigma: f32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture_usage =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;

        // Extract field references before the closure so rustc counts them as reads.
        let pipeline = &self.pipeline;
        let bind_group_layout = &self.bind_group_layout;
        let sampler = &self.sampler;
        let use_immediates = self.use_immediates;

        let (intermediate, intermediate_view) = if let Some(pos) = self
            .intermediate_pool
            .iter()
            .position(|(_, _, w, h)| *w == width && *h == height)
        {
            let (t, v, _, _) = self.intermediate_pool.remove(pos);
            (t, v)
        } else {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("telar-blur-intermediate"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: texture_usage,
                view_formats: &[],
            });
            let v = t.create_view(&wgpu::TextureViewDescriptor::default());
            (t, v)
        };

        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("telar-blur-output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: texture_usage,
            view_formats: &[],
        });
        let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());

        let texture_size = [width as f32, height as f32];

        let make_bg_with_params = |view: &wgpu::TextureView, params: &BlurParams| {
            if use_immediates {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("telar-blur-bind-group"),
                    layout: bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                    ],
                })
            } else {
                use wgpu::util::DeviceExt;
                let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("telar-blur-params"),
                    contents: bytemuck::bytes_of(params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("telar-blur-bind-group"),
                    layout: bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: params_buf.as_entire_binding(),
                        },
                    ],
                })
            }
        };

        {
            let params = BlurParams {
                direction: [1.0, 0.0],
                texture_size,
                sigma,
                _pad: [0.0; 3],
            };
            let bg = make_bg_with_params(src_view, &params);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("telar-blur-h-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &intermediate_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bg, &[]);
            if use_immediates {
                pass.set_immediates(0, bytemuck::bytes_of(&params));
            }
            pass.draw(0..6, 0..1);
        }

        {
            let params = BlurParams {
                direction: [0.0, 1.0],
                texture_size,
                sigma,
                _pad: [0.0; 3],
            };
            let bg = make_bg_with_params(&intermediate_view, &params);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("telar-blur-v-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bg, &[]);
            if use_immediates {
                pass.set_immediates(0, bytemuck::bytes_of(&params));
            }
            pass.draw(0..6, 0..1);
        }

        // Return the intermediate texture to the pool for reuse on the next blur call. The horizontal pass uses Clear on load, so any stale contents from a previous call are overwritten.
        self.intermediate_pool
            .push((intermediate, intermediate_view, width, height));

        (output, output_view)
    }
}
