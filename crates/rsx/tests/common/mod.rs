//! Shared helpers for the headless integration tests: a no-op paths provider and a minimal real app that
//! fills its whole window with one solid color (recomputing layout on the WindowResized that AppHandler
//! synthesizes at mount).

use rsx::{
    App, AppPathsProvider, AvailableSpace, Color, Component, Event, EventResult, LayoutItem,
    LayoutStyle, NodeId, RectStyle, Rectangle, RenderNode, SizeDimension, compute_layout,
    mark_dirty, new_container, reset_layout_runtime,
};

/// A paths provider that reports nothing, so tests touch no real XDG directories (UserPrefs::load finds no
/// file and uses defaults).
pub struct NullPaths;

impl AppPathsProvider for NullPaths {
    fn config_dir(&self) -> Option<std::path::PathBuf> {
        None
    }
    fn data_dir(&self) -> Option<std::path::PathBuf> {
        None
    }
    fn cache_dir(&self) -> Option<std::path::PathBuf> {
        None
    }
}

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
