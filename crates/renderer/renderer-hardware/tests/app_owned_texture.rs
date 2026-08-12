//! An application draws into a texture of its own and Telar composes it into the frame.
//!
//! The device comes from Telar, which is the only direction that exists: it opens the window, picks the
//! backend and negotiates the features, and the application asks for what it is already drawing with. That
//! is what makes the texture shareable at all — two devices cannot exchange one.
//!
//! Its own test binary because the shared device is a process-wide `OnceLock`, and this one wants to be the
//! renderer that populates it.

use std::sync::Arc;

use geometry_core::Rect;
use platform_headless::HeadlessWindow;
use renderer_core::{Color, DrawCommand, ImageFilter, RenderBackend};
use renderer_text::TextShaperConfig;
use telar_renderer_hardware::{
    HardwareRenderer, HardwareRendererConfig,
    gpu::{self, wgpu},
};

#[test]
fn telar_composes_a_texture_the_application_filled() {
    let (w, h) = (64u32, 48u32);
    let mut renderer = match pollster::block_on(HardwareRenderer::<HeadlessWindow>::new_headless(
        w,
        h,
        None,
        false,
        TextShaperConfig::default(),
        HardwareRendererConfig::default(),
    )) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping: no GPU adapter available: {e:?}");
            return;
        }
    };

    let shared = gpu::shared().expect("a live renderer means Telar has a device to lend");

    let (tw, th) = (16u32, 16u32);
    let mine = shared.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("application-owned texture"),
        size: wgpu::Extent3d {
            width: tw,
            height: th,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    // Opaque, so premultiplied and straight alpha agree and the readback can be compared directly.
    const MINE: [u8; 4] = [220, 40, 160, 255];
    shared.queue.write_texture(
        mine.as_image_copy(),
        &MINE.repeat((tw * th) as usize),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(tw * 4),
            rows_per_image: Some(th),
        },
        mine.size(),
    );

    let view = mine.create_view(&wgpu::TextureViewDescriptor::default());
    let data = Arc::new(gpu::image(view, 1, tw, th));
    renderer.begin_frame(w, h, 1.0, 1).expect("begin_frame");
    renderer
        .render_frame(
            &[DrawCommand::Image {
                data,
                rect: Rect::new(0.0, 0.0, w as f32, h as f32),
                filter: ImageFilter::Nearest,
            }],
            Some(Color::rgb(0.0, 0.0, 0.0)),
        )
        .expect("render_frame");

    let pixels = renderer.read_rgba().expect("read_rgba");
    let center = (((h / 2) * w + (w / 2)) * 4) as usize;
    assert_eq!(
        &pixels[center..center + 3],
        &MINE[..3],
        "the frame should show the application's texture, not something Telar uploaded"
    );
}
