//! The fill encoding every instanced pipeline shares, solid and gradient alike.

/// Intermediate fill encoding parameterised over the gradient stop count: rect and text consume 4 slots, paths 8. Not a GPU type: its fields are copied into the `#[repr(C)]` instance structs.
pub(crate) struct EncodedFill<const N: usize> {
    pub fill_type: u32,
    pub fill_color: [f32; 4],
    pub grad_p0: [f32; 2],
    pub grad_p1: [f32; 2],
    pub grad_radius: f32,
    pub grad_stop_count: u32,
    pub grad_positions: [f32; N],
    pub grad_colors: [[f32; 4]; N],
}

impl<const N: usize> EncodedFill<N> {
    /// The "no paint at all" encoding: solid, fully transparent. What an absent fill or stroke uploads.
    pub(crate) fn none() -> Self {
        Self {
            fill_type: 0,
            fill_color: [0.0; 4],
            grad_p0: [0.0; 2],
            grad_p1: [0.0; 2],
            grad_radius: 0.0,
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
    let transform = geometry_core::Transform::from_array(matrix);
    let ap = |p: geometry_core::Point| -> [f32; 2] {
        let m = transform.apply(p);
        [m.x, m.y]
    };
    match fill {
        renderer_core::Paint::Solid(c) => EncodedFill {
            fill_type: 0,
            fill_color: c.to_array(),
            grad_p0: [0.0; 2],
            grad_p1: [0.0; 2],
            grad_radius: 0.0,
            grad_stop_count: 0,
            grad_positions: [0.0; N],
            grad_colors: [[0.0; 4]; N],
        },
        renderer_core::Paint::Gradient(g) => {
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
            let stop_count = (active.len().min(N)) as u32;
            match g.kind {
                renderer_core::GradientKind::Linear { start, end } => EncodedFill {
                    fill_type: 1,
                    fill_color: [0.0; 4],
                    grad_p0: ap(start),
                    grad_p1: ap(end),
                    grad_radius: 0.0,
                    grad_stop_count: stop_count,
                    grad_positions: positions,
                    grad_colors: colors,
                },
                renderer_core::GradientKind::Radial { center, radius } => EncodedFill {
                    fill_type: 2,
                    fill_color: [0.0; 4],
                    grad_p0: ap(center),
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
