use crate::style_pool::FRAME_STYLE_POOL;
use crate::{Color, DrawCommand, Paint, RectStyle};

// Returns Some(fill_alpha) when the rect should be rendered via an intermediate
// layer to avoid the AA-fringe artifact on semi-transparent rounded rects.
fn fill_layer_alpha(style: &RectStyle) -> Option<f32> {
    // Skip when shadow is present: shadow.color.a controls shadow opacity
    // independently and would be incorrectly scaled inside a fill-alpha layer.
    if style.radius.is_zero() || style.shadow.is_some() {
        return None;
    }
    match style.fill {
        Some(Paint::Solid(c)) if c.a > 0.0 && c.a < 1.0 => Some(c.a),
        _ => None,
    }
}

// Expands each semi-transparent solid-fill rounded rect into PushLayer{opacity}
// + opaque Rect + PopLayer, so AA coverage and fill transparency are composited
// separately.
pub fn expand_fill_layers(commands: &[DrawCommand]) -> Option<Vec<DrawCommand>> {
    let needs_expand = commands.iter().any(|cmd| match cmd {
        DrawCommand::Rect { style, .. } => {
            let s = *FRAME_STYLE_POOL.lock().unwrap().get_rect(*style);
            fill_layer_alpha(&s).is_some()
        }
        _ => false,
    });
    if !needs_expand {
        return None;
    }
    let mut result = Vec::with_capacity(commands.len() + 4);
    for cmd in commands {
        if let DrawCommand::Rect { rect, style } = cmd {
            let resolved = *FRAME_STYLE_POOL.lock().unwrap().get_rect(*style);
            if let Some(alpha) = fill_layer_alpha(&resolved) {
                let mut opaque = resolved;
                if let Some(Paint::Solid(c)) = opaque.fill {
                    opaque.fill = Some(Paint::Solid(Color { a: 1.0, ..c }));
                }
                let opaque_handle = FRAME_STYLE_POOL.lock().unwrap().intern_rect(opaque);
                result.push(DrawCommand::PushLayer {
                    opacity: alpha,
                    backdrop_blur: 0.0,
                });
                result.push(DrawCommand::Rect {
                    rect: *rect,
                    style: opaque_handle,
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

fn sbr(br: crate::BorderRadius, sf: f32) -> crate::BorderRadius {
    crate::BorderRadius {
        top_left: br.top_left * sf,
        top_right: br.top_right * sf,
        bottom_right: br.bottom_right * sf,
        bottom_left: br.bottom_left * sf,
    }
}

fn sshadow(s: crate::style::Shadow, sf: f32) -> crate::style::Shadow {
    crate::style::Shadow {
        offset_x: s.offset_x * sf,
        offset_y: s.offset_y * sf,
        blur_radius: s.blur_radius * sf,
        spread: s.spread * sf,
        color: s.color,
    }
}

fn spaint(p: crate::Paint, sf: f32) -> crate::Paint {
    match p {
        crate::Paint::Solid(_) => p,
        crate::Paint::Gradient(g) => crate::Paint::Gradient(crate::style::Gradient {
            kind: match g.kind {
                crate::style::GradientKind::Linear { start, end } => {
                    crate::style::GradientKind::Linear {
                        start: sp(start, sf),
                        end: sp(end, sf),
                    }
                }
                crate::style::GradientKind::Radial { center, radius } => {
                    crate::style::GradientKind::Radial {
                        center: sp(center, sf),
                        radius: radius * sf,
                    }
                }
            },
            stops: g.stops,
        }),
    }
}

fn sstroke(s: crate::style::Stroke, sf: f32) -> crate::style::Stroke {
    crate::style::Stroke {
        paint: spaint(s.paint, sf),
        width: s.width * sf,
        cap: s.cap,
        join: s.join,
    }
}

fn sline_style(s: crate::LineStyle, sf: f32) -> crate::LineStyle {
    crate::LineStyle {
        paint: spaint(s.paint, sf),
        width: s.width * sf,
        cap: s.cap,
        join: s.join,
    }
}

fn srect_style(s: crate::RectStyle, sf: f32) -> crate::RectStyle {
    crate::RectStyle {
        fill: s.fill.map(|p| spaint(p, sf)),
        stroke: s.stroke.map(|st| sstroke(st, sf)),
        shadow: s.shadow.map(|sh| sshadow(sh, sf)),
        radius: sbr(s.radius, sf),
    }
}

fn spath_style(s: crate::PathStyle, sf: f32) -> crate::PathStyle {
    crate::PathStyle {
        fill: s.fill.map(|p| spaint(p, sf)),
        stroke: s.stroke.map(|st| sstroke(st, sf)),
        shadow: s.shadow.map(|sh| sshadow(sh, sf)),
        fill_rule: s.fill_rule,
    }
}

fn stext_style(s: crate::TextStyle, sf: f32) -> crate::TextStyle {
    crate::TextStyle {
        font_size: s.font_size * sf,
        paint: spaint(s.paint, sf),
        shadow: s.shadow.map(|sh| sshadow(sh, sf)),
    }
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
        DrawCommand::Rect { rect, style } => {
            let resolved = *FRAME_STYLE_POOL.lock().unwrap().get_rect(*style);
            let scaled = srect_style(resolved, sf);
            let handle = FRAME_STYLE_POOL.lock().unwrap().intern_rect(scaled);
            DrawCommand::Rect {
                rect: sr(*rect, sf),
                style: handle,
            }
        }
        DrawCommand::Text { text, rect, style } => {
            let resolved = *FRAME_STYLE_POOL.lock().unwrap().get_text(*style);
            let scaled = stext_style(resolved, sf);
            let handle = FRAME_STYLE_POOL.lock().unwrap().intern_text(scaled);
            DrawCommand::Text {
                text: text.clone(),
                rect: sr(*rect, sf),
                style: handle,
            }
        }
        DrawCommand::Image { data, rect, filter } => DrawCommand::Image {
            data: data.clone(),
            rect: sr(*rect, sf),
            filter: *filter,
        },
        DrawCommand::Line { p1, p2, style } => DrawCommand::Line {
            p1: sp(*p1, sf),
            p2: sp(*p2, sf),
            style: sline_style(*style, sf),
        },
        DrawCommand::Path { data, style } => {
            let resolved = *FRAME_STYLE_POOL.lock().unwrap().get_path(*style);
            let scaled = spath_style(resolved, sf);
            let handle = FRAME_STYLE_POOL.lock().unwrap().intern_path(scaled);
            DrawCommand::Path {
                data: std::sync::Arc::new(spath_data(data, sf)),
                style: handle,
            }
        }
        DrawCommand::PushClip { rect, radius } => DrawCommand::PushClip {
            rect: sr(*rect, sf),
            radius: sbr(*radius, sf),
        },
        DrawCommand::PopClip => DrawCommand::PopClip,
        DrawCommand::PushMatrix { matrix } => DrawCommand::PushMatrix {
            // Only the translation components (e, f at indices 4-5) are scaled to physical pixels.
            // The linear part (a,b,c,d) is unchanged: sf*(a*x + c*y + e) = a*(sf*x) + c*(sf*y) + sf*e.
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
