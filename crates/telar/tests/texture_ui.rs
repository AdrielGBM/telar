//! Telar composes into a texture the application owns, at a resolution the application picked.
//!
//! The mirror of `renderer-hardware/tests/app_owned_texture.rs`, which proves the other direction. Its own test binary because the shared device is a process-wide `OnceLock` and this one wants to open it.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use telar::gpu::wgpu;
use telar::{
    Color, Component, Event, EventResult, LayoutItem, LayoutStyle, NodeId, PointerButton,
    PointerSource, Rect, RectStyle, RenderNode, ShapeStyle, TextureUi,
};

const APP: [u8; 4] = [40, 90, 200, 255];
const UI: Color = Color::rgba(1.0, 0.0, 0.0, 1.0);

// Paints a solid rect over the top-left quarter of its box and records where the pointer landed, in its own coordinates. Hand-written so the test asserts against numbers it chose itself.
struct Marker {
    node: NodeId,
    rect: Rect,
    presses: Rc<RefCell<Vec<(f64, f64)>>>,
}

impl Component for Marker {
    fn view(&self) -> RenderNode {
        RenderNode::Primitive(telar::DrawCommand::Rect {
            rect: self.rect,
            style: std::sync::Arc::new(RectStyle::default().with_fill(UI)),
        })
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::PointerPressed { x, y, .. } = event {
            self.presses.borrow_mut().push((*x, *y));
            return EventResult::Handled;
        }
        EventResult::Ignored
    }
}

impl LayoutItem for Marker {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

fn app_texture(gpu: &telar::gpu::SharedGpu, width: u32, height: u32) -> wgpu::Texture {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("application-owned target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    // Opaque, so premultiplied and straight alpha agree and the readback compares directly.
    gpu.queue.write_texture(
        texture.as_image_copy(),
        &APP.repeat((width * height) as usize),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        texture.size(),
    );
    texture
}

fn read_back(gpu: &telar::gpu::SharedGpu, texture: &wgpu::Texture) -> Vec<u8> {
    let (width, height) = (texture.width(), texture.height());
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = (width * 4).div_ceil(align) * align;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded * height) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );
    gpu.queue.submit(std::iter::once(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");
    let data = slice.get_mapped_range().expect("map");
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded) as usize;
        out.extend_from_slice(&data[start..start + (width * 4) as usize]);
    }
    drop(data);
    buffer.unmap();
    out
}

#[test]
fn telar_draws_into_the_application_picture_and_takes_the_pointer_with_it() {
    let Ok(gpu) = telar::gpu::open() else {
        common::skip_without_gpu("the app-owned texture seam");
        return;
    };

    // 320×180 — a resolution the window is not, which is the point of the target being the app's.
    let (w, h) = (320u32, 180u32);
    let target = app_texture(&gpu, w, h);
    let presses = Rc::new(RefCell::new(Vec::new()));

    let built = presses.clone();
    let mut ui = TextureUi::new(target.clone(), 1.0, move || {
        let (node, _) = telar::new_leaf(
            LayoutStyle::new()
                .width(telar::SizeDimension::Percent(1.0))
                .height(telar::SizeDimension::Percent(1.0)),
        )?;
        Ok(Box::new(Marker {
            node,
            rect: Rect::new(0.0, 0.0, 160.0, 90.0),
            presses: built,
        }) as Box<dyn LayoutItem>)
    })
    .expect("texture UI");

    ui.render().expect("render");
    let pixels = read_back(&gpu, &target);
    let at = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    };

    let painted = at(80, 45);
    assert_eq!(
        &painted[..3],
        &[255, 0, 0],
        "the UI should have painted its own quarter of the target"
    );
    assert_eq!(
        at(240, 135),
        APP,
        "the rest of the target must still hold what the application drew — Telar composes into the \
         picture, it does not replace it"
    );

    // A press on the widget's far corner has to arrive in the texture's own coordinates, not the window's.
    ui.place_in(Rect::new(100.0, 50.0, 640.0, 360.0));
    let handled = ui.on_event(&Event::PointerPressed {
        x: 100.0 + 150.0 * 2.0,
        y: 50.0 + 80.0 * 2.0,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
    assert!(handled, "the press landed on the widget and it took it");
    assert_eq!(
        presses.borrow().as_slice(),
        &[(150.0, 80.0)],
        "the pointer must arrive where the user saw it, in the target's coordinates"
    );

    let bigger = app_texture(&gpu, 640, 360);
    ui.resize(bigger.clone(), 1.0);
    assert_eq!(ui.logical_size(), (640.0, 360.0));
    ui.render().expect("render after resize");
    let pixels = read_back(&gpu, &bigger);
    let corner = ((10 * 640 + 10) * 4) as usize;
    assert_eq!(
        &pixels[corner..corner + 3],
        &[255, 0, 0],
        "the UI should be composing into the new texture"
    );
}

// Two trees at two resolutions on one thread, each laying out against its own target and blind to the other's layout world, which is what the per-UI `Surface` buys.
#[test]
fn two_texture_uis_lay_out_against_their_own_targets() {
    let Ok(gpu) = telar::gpu::open() else {
        common::skip_without_gpu("the app-owned texture seam");
        return;
    };

    let sizes = Rc::new(RefCell::new(Vec::new()));
    let ui_of = |width: u32, height: u32| {
        let seen = sizes.clone();
        TextureUi::new(app_texture(&gpu, width, height), 1.0, move || {
            let (node, rect) = telar::new_leaf(
                LayoutStyle::new()
                    .width(telar::SizeDimension::Percent(1.0))
                    .height(telar::SizeDimension::Percent(1.0)),
            )?;
            seen.borrow_mut().push((node, rect));
            Ok(Box::new(Marker {
                node,
                rect: Rect::new(0.0, 0.0, 8.0, 8.0),
                presses: Rc::new(RefCell::new(Vec::new())),
            }) as Box<dyn LayoutItem>)
        })
        .expect("texture UI")
    };
    let _small = ui_of(320, 180);
    let mut large = ui_of(1280, 720);

    let boxes = || -> Vec<Rect> { sizes.borrow().iter().map(|(_, rect)| rect.get()).collect() };
    assert_eq!(
        (boxes()[0].width, boxes()[0].height),
        (320.0, 180.0),
        "the diegetic tree lays out against its own 320×180 target"
    );
    assert_eq!(
        (boxes()[1].width, boxes()[1].height),
        (1280.0, 720.0),
        "the chrome tree lays out against the window's size, in the same process and the same thread"
    );

    // Two worlds, not two views of one. Layout node ids are per-world and freely collide across them, so each tree's rect signal being unmoved by the other's resize is what independence looks like from outside.
    large.resize(app_texture(&gpu, 1920, 1080), 1.0);
    assert_eq!(
        (boxes()[1].width, boxes()[1].height),
        (1920.0, 1080.0),
        "the resized tree follows its new target"
    );
    assert_eq!(
        (boxes()[0].width, boxes()[0].height),
        (320.0, 180.0),
        "and the other one does not"
    );
}

// Draws at a deliberately half-pixel origin, where the smooth raster's blend and the pixel raster's on/off coverage differ and a linear atlas sample would smear either.
struct Label {
    node: NodeId,
    raster: telar::Raster,
}

impl Component for Label {
    fn view(&self) -> RenderNode {
        RenderNode::Primitive(telar::DrawCommand::Text {
            spans: None,
            text: std::sync::Arc::from("Hamburgefonstiv"),
            rect: Rect::new(4.5, 4.5, 200.0, 24.0),
            style: std::sync::Arc::new(
                telar::TextStyle::new(13.0, Color::WHITE).with_raster(self.raster),
            ),
        })
    }
}

impl LayoutItem for Label {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

// The glyph grid end to end: shaped, packed into the GPU atlas, sampled and composed. Every pixel is either the background or the text colour, which no stage between here and the shaper may reintroduce.
#[test]
fn pixel_raster_reaches_the_application_texture_without_a_blended_edge() {
    let Ok(gpu) = telar::gpu::open() else {
        common::skip_without_gpu("the app-owned texture seam");
        return;
    };

    let render_with = |raster| {
        let target = app_texture(&gpu, 240, 32);
        let mut ui = TextureUi::new(target.clone(), 1.0, move || {
            let (node, _) = telar::new_leaf(
                LayoutStyle::new()
                    .width(telar::SizeDimension::Percent(1.0))
                    .height(telar::SizeDimension::Percent(1.0)),
            )?;
            Ok(Box::new(Label { node, raster }) as Box<dyn LayoutItem>)
        })
        .expect("texture UI");
        ui.render().expect("render");
        read_back(&gpu, &target)
    };
    // Neither the app's blue nor the text's white: an edge the rasterizer blended.
    let blended = |pixels: &[u8]| {
        pixels
            .chunks_exact(4)
            .filter(|px| px[..3] != APP[..3] && px[..3] != [255, 255, 255])
            .count()
    };

    let smooth = render_with(telar::Raster::Smooth);
    if blended(&smooth) == 0 {
        eprintln!("skipping: no text was drawn (no fonts on this machine)");
        return;
    }
    let pixel = render_with(telar::Raster::Pixel);
    assert!(
        blended(&pixel) == 0,
        "the pixel raster left {} blended pixels in the application's texture",
        blended(&pixel)
    );
}
