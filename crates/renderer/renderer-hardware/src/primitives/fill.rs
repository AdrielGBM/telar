pub(crate) struct EncodedFill {
    pub fill_type: u32,
    pub fill_color: [f32; 4],
    pub grad_p0: [f32; 2],
    pub grad_p1: [f32; 2],
    pub grad_radius: f32,
    pub grad_stop_count: u32,
    pub grad_positions: [f32; 4],
    pub grad_colors: [[f32; 4]; 4],
}

pub(crate) fn encode_fill_style(fill: &renderer_core::Paint, matrix: [f32; 6]) -> EncodedFill {
    let ap = |x: f32, y: f32| -> [f32; 2] {
        let [a, b, c, d, e, f] = matrix;
        [a * x + c * y + e, b * x + d * y + f]
    };
    match fill {
        renderer_core::Paint::Solid(c) => EncodedFill {
            fill_type: 0,
            fill_color: c.to_array(),
            grad_p0: [0.0; 2],
            grad_p1: [0.0; 2],
            grad_radius: 0.0,
            grad_stop_count: 0,
            grad_positions: [0.0; 4],
            grad_colors: [[0.0; 4]; 4],
        },
        renderer_core::Paint::Gradient(g) => {
            let mut positions = [0.0f32; 4];
            let mut colors = [[0.0f32; 4]; 4];
            for (i, s) in g.stops.active().iter().enumerate() {
                positions[i] = s.position;
                colors[i] = s.color.to_array();
            }
            let stop_count = g.stops.active().len() as u32;
            match g.kind {
                renderer_core::GradientKind::Linear { start, end } => EncodedFill {
                    fill_type: 1,
                    fill_color: [0.0; 4],
                    grad_p0: ap(start.x, start.y),
                    grad_p1: ap(end.x, end.y),
                    grad_radius: 0.0,
                    grad_stop_count: stop_count,
                    grad_positions: positions,
                    grad_colors: colors,
                },
                renderer_core::GradientKind::Radial { center, radius } => EncodedFill {
                    fill_type: 2,
                    fill_color: [0.0; 4],
                    grad_p0: ap(center.x, center.y),
                    grad_p1: [0.0; 2],
                    grad_radius: radius,
                    grad_stop_count: stop_count,
                    grad_positions: positions,
                    grad_colors: colors,
                },
            }
        }
    }
}
