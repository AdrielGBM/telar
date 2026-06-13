use std::sync::Arc;

use rsx::{
    Color, Component, DrawCommand, Line, LineStyle, Point, Rect, RenderNode, TextPayload, TextStyle,
};

/// Returns a cached `Arc<str>` for a `&'static str`, allocating at most once per unique pointer per thread.
pub(crate) fn intern_static_str(s: &'static str) -> Arc<str> {
    use rustc_hash::FxHashMap;
    use std::cell::RefCell;
    thread_local! {
        static MAP: RefCell<FxHashMap<*const u8, Arc<str>>> = RefCell::new(FxHashMap::default());
    }
    MAP.with(|m| {
        m.borrow_mut()
            .entry(s.as_ptr())
            .or_insert_with(|| Arc::from(s))
            .clone()
    })
}

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

pub(crate) fn draw_section_header(
    children: &mut Vec<RenderNode>,
    w: f32,
    title: &'static str,
    border_color: Color,
    text_color: Color,
) {
    children.push(
        Line::new(
            || Point::new(0.0, 0.0),
            move || Point::new(w, 0.0),
            move || LineStyle::new(border_color, 1.0),
        )
        .view(),
    );
    children.push(RenderNode::Primitive(DrawCommand::Text(Arc::new(
        TextPayload {
            text: intern_static_str(title),
            rect: Rect {
                x: 0.0,
                y: 12.0,
                width: 200.0,
                height: 20.0,
            },
            style: TextStyle::new(12.0, text_color),
        },
    ))));
}
