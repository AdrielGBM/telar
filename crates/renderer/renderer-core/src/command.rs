//! [`DrawCommand`]: the flat instruction stream every backend consumes, and the equality that decides whether a frame changed.

use std::sync::Arc;

use geometry_core::{Point, Rect};

use crate::{
    BorderRadius, Element, ImageData, PathData, PathStyle, Raster, RectStyle, Span, Stroke,
    TextStyle,
};

#[derive(Debug, Clone)]
/// One instruction in a frame: something painted, or a change to the clip, matrix or layer stack.
pub enum DrawCommand {
    Rect {
        rect: Rect,
        style: Arc<RectStyle>,
    },
    /// A paragraph, uniform or mixed. `style` is the paragraph's; `spans` are the byte ranges that differ from it — `None`, overwhelmingly the common case, is text that does not.
    ///
    /// One command rather than two because mixed text is not a different kind of thing from text: the shaper builds one span or many through the same call, and every layer above it was carrying a parallel function for a distinction that stopped at the boundary. Keeping the text whole is also what lets a clamp re-shape it with an ellipsis, which per-run text could never be cut across.
    Text {
        text: Arc<str>,
        spans: Option<Arc<[Span]>>,
        rect: Rect,
        style: Arc<TextStyle>,
    },
    Image {
        data: Arc<ImageData>,
        rect: Rect,
        /// How this picture's samples meet the pixel grid — the same property a glyph takes.
        raster: Raster,
    },
    Line {
        p1: Point,
        p2: Point,
        style: Stroke,
    },
    Path {
        data: Arc<PathData>,
        style: Arc<PathStyle>,
    },
    PushClip {
        rect: Rect,
        radius: BorderRadius,
    },
    PopClip,
    PushMatrix {
        matrix: [f32; 6],
    },
    PopMatrix,
    PushLayer {
        opacity: f32,
        backdrop_blur: f32,
    },
    PopLayer,
    /// Opens the box `id` names, for a backend whose output is a document rather than pixels.
    ///
    /// A marker, like [`PushClip`](Self::PushClip): everything until the matching [`PopElement`](Self::PopElement) belongs to this box. A rasteriser skips both and draws exactly what it drew before — the commands between them are already positioned. What a document backend gets is the structure the flattening would otherwise have thrown away, and the identity that lets it move an element instead of rebuilding it.
    PushElement {
        element: Arc<Element>,
    },
    PopElement,
}

impl PartialEq for DrawCommand {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                DrawCommand::Rect {
                    rect: r1,
                    style: s1,
                },
                DrawCommand::Rect {
                    rect: r2,
                    style: s2,
                },
            ) => r1 == r2 && s1 == s2,
            (
                DrawCommand::Text {
                    text: t1,
                    spans: p1,
                    rect: r1,
                    style: s1,
                },
                DrawCommand::Text {
                    text: t2,
                    spans: p2,
                    rect: r2,
                    style: s2,
                },
                // Pointer-equal is the cheap case; comparing ranges keeps a rebuilt-but-identical paragraph equal.
            ) => *t1 == *t2 && spans_eq(p1, p2) && r1 == r2 && s1 == s2,
            (
                DrawCommand::Image {
                    data: d1,
                    rect: rect1,
                    raster: raster1,
                },
                DrawCommand::Image {
                    data: d2,
                    rect: rect2,
                    raster: raster2,
                },
            ) => d1.id == d2.id && rect1 == rect2 && raster1 == raster2,
            (
                DrawCommand::Line {
                    p1: p1a,
                    p2: p2a,
                    style: s1,
                },
                DrawCommand::Line {
                    p1: p1b,
                    p2: p2b,
                    style: s2,
                },
            ) => p1a == p1b && p2a == p2b && s1 == s2,
            (
                DrawCommand::Path {
                    data: d1,
                    style: s1,
                },
                DrawCommand::Path {
                    data: d2,
                    style: s2,
                },
                // Pointer-equal is the cheap common case (same Arc reused); fall back to comparing geometry so a structurally-identical path rebuilt across frames still counts as unchanged (keeps scroll-blit / dirty-rect alive past path rebuilds).
            ) => (Arc::ptr_eq(d1, d2) || d1 == d2) && s1 == s2,
            (
                DrawCommand::PushClip {
                    rect: r1,
                    radius: br1,
                },
                DrawCommand::PushClip {
                    rect: r2,
                    radius: br2,
                },
            ) => r1 == r2 && br1 == br2,
            (DrawCommand::PopClip, DrawCommand::PopClip) => true,
            (DrawCommand::PushMatrix { matrix: m1 }, DrawCommand::PushMatrix { matrix: m2 }) => {
                m1 == m2
            }
            (DrawCommand::PopMatrix, DrawCommand::PopMatrix) => true,
            (
                DrawCommand::PushLayer {
                    opacity: o1,
                    backdrop_blur: b1,
                },
                DrawCommand::PushLayer {
                    opacity: o2,
                    backdrop_blur: b2,
                },
            ) => o1 == o2 && b1 == b2,
            (DrawCommand::PopLayer, DrawCommand::PopLayer) => true,
            (
                DrawCommand::PushElement { element: e1 },
                DrawCommand::PushElement { element: e2 },
            ) => Arc::ptr_eq(e1, e2) || e1 == e2,
            (DrawCommand::PopElement, DrawCommand::PopElement) => true,
            _ => false,
        }
    }
}

fn spans_eq(a: &Option<Arc<[Span]>>, b: &Option<Arc<[Span]>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a == b,
        _ => false,
    }
}
