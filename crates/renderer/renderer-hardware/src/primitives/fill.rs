//! The fill encoding every instanced pipeline shares, solid and gradient alike.

/// Intermediate fill encoding parameterised over the gradient stop count: rect and text consume 4 slots, paths 8. Not a GPU type: its fields are copied into the `#[repr(C)]` instance structs. A gradient is sampled at `q = M·(p − grad_origin)`, where `grad_map` holds the rows of `M`: a linear ramp reads `q.x`, a radial one `|q|`, so either reaches its last stop at 1.
pub(crate) struct EncodedFill<const N: usize> {
    pub fill_type: u32,
    pub fill_color: [f32; 4],
    pub grad_origin: [f32; 2],
    pub grad_map: [f32; 4],
    pub grad_stop_count: u32,
    pub grad_positions: [f32; N],
    pub grad_colors: [[f32; 4]; N],
}

impl<const N: usize> EncodedFill<N> {
    /// The "no paint at all" encoding: solid, fully transparent. What an absent fill or stroke uploads.
    pub(crate) fn none() -> Self {
        Self::solid([0.0; 4])
    }

    fn solid(color: [f32; 4]) -> Self {
        Self {
            fill_type: 0,
            fill_color: color,
            grad_origin: [0.0; 2],
            grad_map: [0.0; 4],
            grad_stop_count: 0,
            grad_positions: [0.0; N],
            grad_colors: [[0.0; 4]; N],
        }
    }
}

pub(crate) fn encode_fill_style<const N: usize>(
    fill: &renderer_core::Paint,
    matrix: [f32; 6],
) -> EncodedFill<N> {
    let g = match fill {
        renderer_core::Paint::Solid(c) => return EncodedFill::solid(c.to_array()),
        renderer_core::Paint::Gradient(g) => g,
    };
    let mut positions = [0.0f32; N];
    let mut colors = [[0.0f32; 4]; N];
    let active = g.stops.active();
    // The shader holds only N gradient slots, so clamp to avoid an out-of-bounds write when core carries more.
    debug_assert!(
        active.len() <= N,
        "fill encoder supports at most {N} gradient stops"
    );
    for (i, s) in active.iter().take(N).enumerate() {
        positions[i] = s.position;
        colors[i] = s.color.to_array();
    }
    let (fill_type, grad_origin, grad_map) = match g
        .kind
        .transformed(geometry_core::Transform::from_array(matrix))
    {
        renderer_core::GradientKind::Linear { start, end } => {
            let (dx, dy) = (end.x - start.x, end.y - start.y);
            let len_sq = dx * dx + dy * dy;
            let map = if len_sq > 0.0001 {
                [dx / len_sq, dy / len_sq, 0.0, 0.0]
            } else {
                [0.0; 4]
            };
            (1, [start.x, start.y], map)
        }
        renderer_core::GradientKind::Radial { transform } => {
            let map = transform.invert().map_or([0.0; 4], |inverse| {
                [inverse.a, inverse.c, inverse.b, inverse.d]
            });
            (2, [transform.e, transform.f], map)
        }
    };
    EncodedFill {
        fill_type,
        fill_color: [0.0; 4],
        grad_origin,
        grad_map,
        grad_stop_count: active.len().min(N) as u32,
        grad_positions: positions,
        grad_colors: colors,
    }
}
