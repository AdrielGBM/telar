//! Shared helpers for the headless integration tests: a no-op paths provider and a minimal real app that
//! fills its whole window with one solid color (recomputing layout on the WindowResized that AppHandler
//! synthesizes at mount).

// Each test binary compiles this module whole but uses only the helpers it needs.
#![allow(dead_code)]

use telar::{
    App, AvailableSpace, Color, Component, Event, EventResult, LayoutItem, LayoutStyle, NodeId,
    RectStyle, Rectangle, RenderNode, SizeDimension, compute_layout, mark_dirty, new_container,
    reset_layout_runtime,
};

struct FillRoot {
    root: NodeId,
    rect: Rectangle,
}

impl FillRoot {
    fn new(color: Color) -> Self {
        let rect = Rectangle::new(
            LayoutStyle::new()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            move || RectStyle::filled(color, 0.0),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            &[rect.layout_node()],
        )
        .unwrap();
        Self { root, rect }
    }
}

impl Component for FillRoot {
    fn view(&self) -> RenderNode {
        self.rect.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::WindowResized { width, height } = event {
            mark_dirty(self.root).ok();
            compute_layout(
                self.root,
                AvailableSpace::Definite(*width as f32),
                AvailableSpace::Definite(*height as f32),
            )
            .ok();
            return EventResult::Handled;
        }
        EventResult::Ignored
    }
}

/// A real rsx app whose window is entirely one solid color.
pub struct FillApp {
    pub color: Color,
}

impl App for FillApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        Box::new(FillRoot::new(self.color))
    }

    fn clear_color(&self) -> Option<Color> {
        Some(Color::rgba(0.0, 0.0, 0.0, 1.0))
    }
}

/// A fill app that can be told to panic during build, to exercise the multi-surface panic quarantine
/// (T-4.2): a surface whose build panics must unmount without tumbling the other surfaces.
pub struct MaybePanicApp {
    pub color: Color,
    pub panic_on_build: bool,
}

impl App for MaybePanicApp {
    fn root(&self) -> Box<dyn Component> {
        assert!(
            !self.panic_on_build,
            "MaybePanicApp: intentional build panic"
        );
        reset_layout_runtime();
        Box::new(FillRoot::new(self.color))
    }

    fn clear_color(&self) -> Option<Color> {
        Some(Color::rgba(0.0, 0.0, 0.0, 1.0))
    }
}

/// Assert the center pixel of a `w`×`h` premultiplied-RGBA8 buffer matches `expected` (R, G, B) within a small
/// tolerance, and is not the black clear color.
pub fn assert_center_rgb(pixels: &[u8], w: u32, h: u32, expected: [u8; 3], label: &str) {
    assert_eq!(
        pixels.len(),
        (w * h * 4) as usize,
        "{label}: read-back must be width*height*4"
    );
    let center = (((h / 2) * w + (w / 2)) * 4) as usize;
    let px = &pixels[center..center + 4];
    for c in 0..3 {
        assert!(
            (px[c] as i32 - expected[c] as i32).abs() <= 4,
            "{label} channel {c}: got {} expected ~{} (pixel {px:?})",
            px[c],
            expected[c],
        );
    }
    assert_ne!(
        &px[0..3],
        &[0u8, 0, 0],
        "{label}: fill did not paint over clear"
    );
}

/// Reports that a GPU test is skipping for want of an adapter — and fails instead when `TELAR_REQUIRE_GPU`
/// is set.
///
/// CI sets it on the leg that installs lavapipe, so a suite that quietly stopped covering the GPU reads as
/// red there rather than as passing tests that never ran. Everywhere else the absence is a real answer and
/// the test skips.
pub fn skip_without_gpu(what: &str) {
    // An empty value counts as unset: a workflow that picks the variable per matrix leg still defines it as
    // `""` on the legs that do not want it, and reading that as "required" would fail every skip.
    let required = std::env::var("TELAR_REQUIRE_GPU").is_ok_and(|v| !v.is_empty());
    assert!(
        !required,
        "{what}: TELAR_REQUIRE_GPU is set, so an adapter was expected"
    );
    eprintln!("skipping {what}: no GPU adapter available");
}
