use std::sync::Arc;

use geometry_core::{Point, Rect};

use crate::{
    BorderRadius, ImageData, ImageFilter, PathData, PathStyle, RectStyle, Stroke, TextStyle,
};

#[derive(Debug, Clone)]
pub enum DrawCommand {
    Rect {
        rect: Rect,
        style: Arc<RectStyle>,
    },
    Text {
        text: Arc<str>,
        rect: Rect,
        style: Arc<TextStyle>,
    },
    Image {
        data: Arc<ImageData>,
        rect: Rect,
        filter: ImageFilter,
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
                    rect: r1,
                    style: s1,
                },
                DrawCommand::Text {
                    text: t2,
                    rect: r2,
                    style: s2,
                },
            ) => *t1 == *t2 && r1 == r2 && s1 == s2,
            (
                DrawCommand::Image {
                    data: d1,
                    rect: r1,
                    filter: f1,
                },
                DrawCommand::Image {
                    data: d2,
                    rect: r2,
                    filter: f2,
                },
            ) => d1.id == d2.id && r1 == r2 && f1 == f2,
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
            _ => false,
        }
    }
}
