//! Underlines and text cases on the GPU, checked against the software backend drawing the same frame. Skips when no GPU adapter is available, unless `TELAR_REQUIRE_GPU` says one was expected.

#[path = "test_common.rs"]
mod common;

use std::sync::Arc;

use geometry_core::Rect;
use platform_headless::HeadlessWindow;
use renderer_core::{Color, DrawCommand, RenderBackend, TextCase, TextStyle};
use renderer_software::{SoftwareRenderer, SoftwareRendererConfig};
use renderer_text::TextShaperConfig;
use telar_renderer_hardware::HardwareRenderer;

const WIDTH: u32 = 200;
const HEIGHT: u32 = 48;

fn frame(text: &str, style: TextStyle) -> Vec<DrawCommand> {
    vec![DrawCommand::Text {
        spans: None,
        text: Arc::from(text),
        rect: Rect::new(10.0, 8.0, 180.0, 30.0),
        style: Arc::new(style),
    }]
}

fn gpu(commands: &[DrawCommand], what: &str) -> Option<Vec<u8>> {
    let mut renderer = match pollster::block_on(HardwareRenderer::<HeadlessWindow>::new_headless(
        WIDTH,
        HEIGHT,
        None,
        false,
        TextShaperConfig::default(),
    )) {
        Ok(renderer) => renderer,
        Err(e) => {
            common::skip_without_gpu(what, e);
            return None;
        }
    };
    renderer
        .begin_frame(WIDTH, HEIGHT, 1.0, 1)
        .expect("begin_frame");
    renderer
        .render_frame(commands, Some(Color::BLACK))
        .expect("render_frame");
    Some(renderer.read_rgba().expect("read_rgba"))
}

fn cpu(commands: &[DrawCommand]) -> Vec<u8> {
    let mut renderer: SoftwareRenderer<HeadlessWindow, HeadlessWindow> =
        SoftwareRenderer::new_headless(WIDTH, HEIGHT, SoftwareRendererConfig::default());
    renderer
        .begin_frame(WIDTH, HEIGHT, 1.0, 0)
        .expect("begin_frame");
    renderer
        .render_frame(commands, Some(Color::BLACK))
        .expect("render_frame");
    renderer.read_rgba().expect("read_rgba").to_vec()
}

/// The rows in which at least `min` pixels are the underline's green and nothing else.
fn green_rows(rgba: &[u8], min: usize) -> Vec<usize> {
    rgba.chunks(WIDTH as usize * 4)
        .enumerate()
        .filter(|(_, row)| {
            row.chunks(4)
                .filter(|px| px[1] > 200 && px[0] < 40 && px[2] < 40)
                .count()
                >= min
        })
        .map(|(y, _)| y)
        .collect()
}

fn inked(rgba: &[u8]) -> usize {
    rgba.chunks(4).filter(|px| px[0] > 100).count()
}

#[test]
fn the_gpu_draws_an_underline_where_the_software_backend_does() {
    let underlined = TextStyle::new(16.0, Color::WHITE)
        .with_underline(true)
        .with_underline_offset(4.0)
        .with_underline_thickness(2.0)
        .with_underline_color(Color::from_rgb_u8(0, 255, 0));
    let commands = frame("underline", underlined);
    let Some(gpu) = gpu(&commands, "underline") else {
        return;
    };
    let on_gpu = green_rows(&gpu, 40);
    assert_eq!(on_gpu.len(), 2, "two rows thick: {on_gpu:?}");
    assert_eq!(on_gpu, green_rows(&cpu(&commands), 40));
}

#[test]
fn the_gpu_draws_a_cased_text_as_the_string_it_shows() {
    let style = TextStyle::new(16.0, Color::WHITE);
    let Some(cased) = gpu(
        &frame("menu", style.clone().with_text_case(TextCase::Upper)),
        "text case",
    ) else {
        return;
    };
    let shown = gpu(&frame("MENU", style.clone()), "text case").expect("an adapter was found");
    let written = gpu(&frame("menu", style), "text case").expect("an adapter was found");
    assert_eq!(cased, shown);
    assert_ne!(inked(&cased), inked(&written));
}
