use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::style::Scale;
use crate::{Color, DrawCommand, Paint, PathData, PathStyle, RectStyle, TextStyle};

fn fill_layer_alpha(style: &RectStyle) -> Option<f32> {
    // Skip when a shadow is present: shadow.color.a controls shadow opacity independently and would be incorrectly scaled inside a fill-alpha layer.
    if style.radius.is_zero() || style.shadow.is_some() {
        return None;
    }
    match style.fill {
        Some(Paint::Solid(c)) if c.a > 0.0 && c.a < 1.0 => Some(c.a),
        _ => None,
    }
}

/// Cheap, allocation-free predicate for whether `expand_fill_layers` would rewrite any Rect into a
/// synthetic `PushLayer` opacity triple. F1 damage tracking must treat these hidden layers like real
/// `PushLayer`s (their composite is not confined to the dirty rect), so it queries this first.
pub fn would_expand_fill_layers(commands: &[DrawCommand]) -> bool {
    commands.iter().any(|cmd| match cmd {
        DrawCommand::Rect { style, .. } => fill_layer_alpha(style).is_some(),
        _ => false,
    })
}

pub fn expand_fill_layers(commands: &[DrawCommand]) -> Option<Vec<DrawCommand>> {
    if !would_expand_fill_layers(commands) {
        return None;
    }
    let mut result = Vec::with_capacity(commands.len() + 4);
    for cmd in commands {
        if let DrawCommand::Rect { rect, style } = cmd {
            if let Some(alpha) = fill_layer_alpha(style) {
                let mut opaque = **style;
                if let Some(Paint::Solid(c)) = opaque.fill {
                    opaque.fill = Some(Paint::Solid(Color { a: 1.0, ..c }));
                }
                result.push(DrawCommand::PushLayer {
                    opacity: alpha,
                    backdrop_blur: 0.0,
                });
                result.push(DrawCommand::Rect {
                    rect: *rect,
                    style: Arc::new(opaque),
                });
                result.push(DrawCommand::PopLayer);
                continue;
            }
        }
        result.push(cmd.clone());
    }
    Some(result)
}

pub fn blur_sigma(blur_radius: f32) -> f32 {
    blur_radius / 2.0
}

pub fn blur_padding(sigma: f32) -> i32 {
    (sigma * 3.0).ceil() as i32 + 1
}

pub fn scale_commands(commands: &[DrawCommand], sf: f32) -> Option<Vec<DrawCommand>> {
    if (sf - 1.0).abs() < f32::EPSILON {
        return None;
    }
    Some(commands.iter().map(|cmd| scale_command(cmd, sf)).collect())
}

fn scale_path_data(data: &crate::PathData, sf: f32) -> crate::PathData {
    let mut out = crate::PathData::new();
    for verb in data.verbs() {
        out = match verb {
            crate::PathVerb::MoveTo(p) => out.move_to(p.scale(sf)),
            crate::PathVerb::LineTo(p) => out.line_to(p.scale(sf)),
            crate::PathVerb::QuadTo { ctrl, to } => out.quad_to(ctrl.scale(sf), to.scale(sf)),
            crate::PathVerb::CubicTo { ctrl1, ctrl2, to } => {
                out.cubic_to(ctrl1.scale(sf), ctrl2.scale(sf), to.scale(sf))
            }
            crate::PathVerb::Close => out.close(),
        };
    }
    out
}

fn scale_command(cmd: &DrawCommand, sf: f32) -> DrawCommand {
    match cmd {
        DrawCommand::Rect { rect, style } => DrawCommand::Rect {
            rect: rect.scale(sf),
            style: Arc::new((**style).scale(sf)),
        },
        DrawCommand::Text { text, rect, style } => DrawCommand::Text {
            text: text.clone(),
            rect: rect.scale(sf),
            style: Arc::new((**style).scale(sf)),
        },
        DrawCommand::Image { data, rect, filter } => DrawCommand::Image {
            data: data.clone(),
            rect: rect.scale(sf),
            filter: *filter,
        },
        DrawCommand::Line { p1, p2, style } => DrawCommand::Line {
            p1: p1.scale(sf),
            p2: p2.scale(sf),
            style: (*style).scale(sf),
        },
        DrawCommand::Path { data, style } => DrawCommand::Path {
            data: Arc::new(scale_path_data(data, sf)),
            style: Arc::new((**style).scale(sf)),
        },
        DrawCommand::PushClip { rect, radius } => DrawCommand::PushClip {
            rect: rect.scale(sf),
            radius: (*radius).scale(sf),
        },
        DrawCommand::PopClip => DrawCommand::PopClip,
        DrawCommand::PushMatrix { matrix } => DrawCommand::PushMatrix {
            // Only the translation components (e, f at 4-5) are scaled; the linear part stays since sf*(a*x + c*y + e) = a*(sf*x) + c*(sf*y) + sf*e.
            matrix: [
                matrix[0],
                matrix[1],
                matrix[2],
                matrix[3],
                matrix[4] * sf,
                matrix[5] * sf,
            ],
        },
        DrawCommand::PopMatrix => DrawCommand::PopMatrix,
        DrawCommand::PushLayer {
            opacity,
            backdrop_blur,
        } => DrawCommand::PushLayer {
            opacity: *opacity,
            backdrop_blur: backdrop_blur * sf,
        },
        DrawCommand::PopLayer => DrawCommand::PopLayer,
    }
}

/// Reusable scratch for scaling draw commands to physical pixels on the software path. Holds the
/// output buffer plus per-frame caches keyed by the source `Arc` pointer, so a style shared by many
/// commands (the common case for a UI tree) is scaled and heap-allocated once per frame rather than
/// once per command.
#[derive(Default)]
pub struct ScaleScratch {
    storage: Vec<DrawCommand>,
    rect_styles: FxHashMap<usize, Arc<RectStyle>>,
    text_styles: FxHashMap<usize, Arc<TextStyle>>,
    path_styles: FxHashMap<usize, Arc<PathStyle>>,
    path_data: FxHashMap<usize, Arc<PathData>>,
}

impl ScaleScratch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Scales every command in `commands` by `sf` into the reusable internal buffer and returns it.
    /// The pointer-keyed caches are cleared on entry, so a recycled allocator address can never yield
    /// a stale hit (ABA-safe); cache reuse only spans commands within this single call.
    pub fn scale_into(&mut self, commands: &[DrawCommand], sf: f32) -> &[DrawCommand] {
        // Destructure so the output buffer and the caches are borrowed as disjoint fields in the loop.
        let Self {
            storage,
            rect_styles,
            text_styles,
            path_styles,
            path_data,
        } = self;
        storage.clear();
        rect_styles.clear();
        text_styles.clear();
        path_styles.clear();
        path_data.clear();
        storage.reserve(commands.len());
        for cmd in commands {
            storage.push(scale_command_cached(
                cmd,
                sf,
                rect_styles,
                text_styles,
                path_styles,
                path_data,
            ));
        }
        storage
    }
}

#[inline]
fn scaled_style_arc<T: Scale + Copy>(
    cache: &mut FxHashMap<usize, Arc<T>>,
    style: &Arc<T>,
    sf: f32,
) -> Arc<T> {
    let key = Arc::as_ptr(style) as usize;
    cache
        .entry(key)
        .or_insert_with(|| Arc::new((**style).scale(sf)))
        .clone()
}

#[inline]
fn scaled_path_arc(
    cache: &mut FxHashMap<usize, Arc<PathData>>,
    data: &Arc<PathData>,
    sf: f32,
) -> Arc<PathData> {
    let key = Arc::as_ptr(data) as usize;
    cache
        .entry(key)
        .or_insert_with(|| Arc::new(scale_path_data(data, sf)))
        .clone()
}

fn scale_command_cached(
    cmd: &DrawCommand,
    sf: f32,
    rect_styles: &mut FxHashMap<usize, Arc<RectStyle>>,
    text_styles: &mut FxHashMap<usize, Arc<TextStyle>>,
    path_styles: &mut FxHashMap<usize, Arc<PathStyle>>,
    path_data: &mut FxHashMap<usize, Arc<PathData>>,
) -> DrawCommand {
    match cmd {
        DrawCommand::Rect { rect, style } => DrawCommand::Rect {
            rect: rect.scale(sf),
            style: scaled_style_arc(rect_styles, style, sf),
        },
        DrawCommand::Text { text, rect, style } => DrawCommand::Text {
            text: text.clone(),
            rect: rect.scale(sf),
            style: scaled_style_arc(text_styles, style, sf),
        },
        DrawCommand::Path { data, style } => DrawCommand::Path {
            data: scaled_path_arc(path_data, data, sf),
            style: scaled_style_arc(path_styles, style, sf),
        },
        // The remaining variants either allocate nothing per command (Line/clip/matrix/layer) or only bump an Arc refcount (Image), so the uncached path is already cheap.
        other => scale_command(other, sf),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BorderRadius, RectStyle};
    use geometry_core::Rect;

    fn rect_cmd(style: &Arc<RectStyle>, x: f32) -> DrawCommand {
        DrawCommand::Rect {
            rect: Rect::new(x, 0.0, 10.0, 10.0),
            style: style.clone(),
        }
    }

    #[test]
    fn scale_into_matches_scale_commands_and_shares_arcs() {
        let style = Arc::new(RectStyle::default().with_radius(BorderRadius::all(4.0)));
        let cmds = vec![
            rect_cmd(&style, 0.0),
            rect_cmd(&style, 20.0),
            rect_cmd(&style, 40.0),
        ];
        let sf = 3.0;

        let expected = scale_commands(&cmds, sf).unwrap();
        let mut scratch = ScaleScratch::new();
        let got = scratch.scale_into(&cmds, sf);
        assert_eq!(got.len(), expected.len());
        for (g, e) in got.iter().zip(expected.iter()) {
            assert!(g == e);
        }

        // The three commands shared one input style Arc, so the scaled output Arcs are shared too (one Arc::new instead of three).
        let style_of = |c: &DrawCommand| match c {
            DrawCommand::Rect { style, .. } => style.clone(),
            _ => unreachable!(),
        };
        let a0 = style_of(&got[0]);
        let a1 = style_of(&got[1]);
        let a2 = style_of(&got[2]);
        assert!(Arc::ptr_eq(&a0, &a1));
        assert!(Arc::ptr_eq(&a1, &a2));
        assert_eq!(a0.radius.top_left, 12.0);
    }

    #[test]
    fn scale_into_reuses_buffer_across_frames() {
        let style = Arc::new(RectStyle::default());
        let cmds = vec![rect_cmd(&style, 0.0), rect_cmd(&style, 10.0)];
        let mut scratch = ScaleScratch::new();
        let _ = scratch.scale_into(&cmds, 2.0);
        let cap = scratch.storage.capacity();
        assert!(cap >= cmds.len());
        let _ = scratch.scale_into(&cmds, 2.0);
        // Buffer capacity persists between frames: no per-frame Vec reallocation for the same command count.
        assert_eq!(scratch.storage.capacity(), cap);
    }
}
