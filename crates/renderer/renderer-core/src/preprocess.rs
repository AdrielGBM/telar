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
    if !commands
        .iter()
        .any(|cmd| matches!(cmd, DrawCommand::Rect(p) if fill_layer_alpha(&p.style).is_some()))
    {
        return None;
    }
    let mut result = Vec::with_capacity(commands.len() + 4);
    for cmd in commands {
        if let DrawCommand::Rect(p) = cmd {
            if let Some(alpha) = fill_layer_alpha(&p.style) {
                let mut opaque = (**p).clone();
                if let Some(Paint::Solid(c)) = opaque.style.fill {
                    opaque.style.fill = Some(Paint::Solid(Color { a: 1.0, ..c }));
                }
                result.push(DrawCommand::PushLayer {
                    opacity: alpha,
                    backdrop_blur: 0.0,
                    clip_radius: 0.0,
                });
                result.push(DrawCommand::Rect(Box::new(opaque)));
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
