use std::sync::Arc;

use crate::style::Scale;
use crate::{Color, DrawCommand, Paint, RectStyle};

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

pub fn expand_fill_layers(commands: &[DrawCommand]) -> Option<Vec<DrawCommand>> {
    let needs_expand = commands.iter().any(|cmd| match cmd {
        DrawCommand::Rect { style, .. } => fill_layer_alpha(style).is_some(),
        _ => false,
    });
    if !needs_expand {
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

fn sp(p: geometry_core::Point, sf: f32) -> geometry_core::Point {
    geometry_core::Point::new(p.x * sf, p.y * sf)
}

fn sr(r: geometry_core::Rect, sf: f32) -> geometry_core::Rect {
    geometry_core::Rect::new(r.x * sf, r.y * sf, r.width * sf, r.height * sf)
}

fn spath_data(data: &crate::PathData, sf: f32) -> crate::PathData {
    let mut out = crate::PathData::new();
    for verb in data.verbs() {
        out = match verb {
            crate::PathVerb::MoveTo(p) => out.move_to(sp(*p, sf)),
            crate::PathVerb::LineTo(p) => out.line_to(sp(*p, sf)),
            crate::PathVerb::QuadTo { ctrl, to } => out.quad_to(sp(*ctrl, sf), sp(*to, sf)),
            crate::PathVerb::CubicTo { ctrl1, ctrl2, to } => {
                out.cubic_to(sp(*ctrl1, sf), sp(*ctrl2, sf), sp(*to, sf))
            }
            crate::PathVerb::Close => out.close(),
        };
    }
    out
}

fn scale_command(cmd: &DrawCommand, sf: f32) -> DrawCommand {
    match cmd {
        DrawCommand::Rect { rect, style } => DrawCommand::Rect {
            rect: sr(*rect, sf),
            style: Arc::new((**style).scale(sf)),
        },
        DrawCommand::Text { text, rect, style } => DrawCommand::Text {
            text: text.clone(),
            rect: sr(*rect, sf),
            style: Arc::new((**style).scale(sf)),
        },
        DrawCommand::Image { data, rect, filter } => DrawCommand::Image {
            data: data.clone(),
            rect: sr(*rect, sf),
            filter: *filter,
        },
        DrawCommand::Line { p1, p2, style } => DrawCommand::Line {
            p1: sp(*p1, sf),
            p2: sp(*p2, sf),
            style: (*style).scale(sf),
        },
        DrawCommand::Path { data, style } => DrawCommand::Path {
            data: Arc::new(spath_data(data, sf)),
            style: Arc::new((**style).scale(sf)),
        },
        DrawCommand::PushClip { rect, radius } => DrawCommand::PushClip {
            rect: sr(*rect, sf),
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
        #[cfg(target_os = "android")]
        DrawCommand::AndroidHardwareBufferImage { .. } => cmd.clone(),
    }
}
