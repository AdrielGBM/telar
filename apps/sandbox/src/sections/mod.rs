use std::sync::Arc;

use crate::theme::SandboxTheme;
use rsx::{
    Component, DrawCommand, Line, LineStyle, Point, Rect, RenderNode, TextPayload, TextStyle,
    use_theme,
};

/// Returns a per-call-site cached `Arc<str>` for a string literal, allocating at most once per thread.
#[macro_export]
macro_rules! static_rc_str {
    ($s:literal) => {{
        thread_local! {
            static V: std::sync::Arc<str> = std::sync::Arc::from($s as &str);
        }
        V.with(std::sync::Arc::clone)
    }};
}

mod cards;
mod colors;
mod gradients;
mod grid;
mod images_section;
mod layers;
mod lines;
mod paths;
mod shadows;
mod shapes;
mod theme_section;
mod transforms;
mod typography;

pub use cards::cards_section;
pub use colors::colors_section;
pub use gradients::gradients_section;
pub use grid::grid_section;
pub use images_section::images_section;
pub use layers::layers_section;
pub use lines::lines_section;
pub use paths::paths_section;
pub use shadows::shadows_section;
pub use shapes::shapes_section;
pub use theme_section::theme_section;
pub use transforms::transforms_section;
pub use typography::typography_section;

pub(crate) fn draw_section_header(children: &mut Vec<RenderNode>, w: f32, title: &'static str) {
    let muted = use_theme::<SandboxTheme>().muted;
    children.push(
        Line::new(
            || Point::new(0.0, 0.0),
            move || Point::new(w, 0.0),
            move || LineStyle::new(use_theme::<SandboxTheme>().card_border, 1.0),
        )
        .view(),
    );
    children.push(RenderNode::Primitive(DrawCommand::Text(Arc::new(
        TextPayload {
            text: Arc::from(title),
            rect: Rect {
                x: 0.0,
                y: 12.0,
                width: 200.0,
                height: 20.0,
            },
            style: TextStyle::new(12.0, muted),
        },
    ))));
}
