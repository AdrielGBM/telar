use std::sync::Arc;

use geometry_core::{ObjectFit, Point};

use renderer_core::{Color, DrawCommand, GradientKind, Paint, PathData, PathStyle, PathVerb};

use super::SvgData;

// SvgData is shared across threads via Arc.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SvgData>();
};

fn only_path(cmds: &[DrawCommand]) -> (&PathData, &PathStyle) {
    match cmds.iter().find(|c| matches!(c, DrawCommand::Path { .. })) {
        Some(DrawCommand::Path { data, style }) => (data, style),
        _ => panic!("expected a Path command, got {cmds:?}"),
    }
}

#[test]
fn from_str_parses_and_reports_intrinsic_size() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="16"><rect width="24" height="16"/></svg>"##;
    let data = SvgData::from_str(svg).unwrap();
    let (w, h) = data.intrinsic_size();
    assert!((w - 24.0).abs() < 1e-3, "width {w}");
    assert!((h - 16.0).abs() < 1e-3, "height {h}");
}

#[test]
fn invalid_svg_returns_err() {
    assert!(SvgData::from_str("not an svg at all").is_err());
}

#[test]
fn solid_path_scales_and_centers() {
    // 10x10 viewBox, a filled rect covering the whole box, rendered into a 20x40 widget.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><rect x="0" y="0" width="10" height="10" fill="#ff0000"/></svg>"##;
    let data = SvgData::from_str(svg).unwrap();
    let cmds = data.commands_for(20.0, 40.0, None, None, ObjectFit::Contain);
    let (path, style) = only_path(&cmds);

    // Fit scale is min(20/10, 40/10) = 2, centered vertically: offset_y = (40 - 20)/2 = 10.
    let xs: Vec<Point> = path
        .verbs()
        .iter()
        .filter_map(|v| match v {
            PathVerb::MoveTo(p) | PathVerb::LineTo(p) => Some(*p),
            _ => None,
        })
        .collect();
    let min_x = xs.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let max_x = xs.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
    let min_y = xs.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let max_y = xs.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
    assert!((min_x - 0.0).abs() < 1e-2, "min_x {min_x}");
    assert!((max_x - 20.0).abs() < 1e-2, "max_x {max_x}");
    assert!((min_y - 10.0).abs() < 1e-2, "min_y {min_y}");
    assert!((max_y - 30.0).abs() < 1e-2, "max_y {max_y}");

    match style.fill {
        Some(Paint::Solid(c)) => {
            assert!((c.r - 1.0).abs() < 1e-3 && c.g.abs() < 1e-3 && c.b.abs() < 1e-3);
            assert!((c.a - 1.0).abs() < 1e-3);
        }
        other => panic!("expected solid fill, got {other:?}"),
    }
}

#[test]
fn group_opacity_emits_balanced_layer() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><g opacity="0.5"><rect width="10" height="10" fill="#00ff00"/></g></svg>"##;
    let data = SvgData::from_str(svg).unwrap();
    let cmds = data.commands_for(10.0, 10.0, None, None, ObjectFit::Contain);
    let pushes = cmds
        .iter()
        .filter(|c| matches!(c, DrawCommand::PushLayer { .. }))
        .count();
    let pops = cmds
        .iter()
        .filter(|c| matches!(c, DrawCommand::PopLayer))
        .count();
    assert_eq!(pushes, 1, "one PushLayer expected: {cmds:?}");
    assert_eq!(pops, 1, "one PopLayer expected: {cmds:?}");
    // PushLayer must precede PopLayer.
    let push_idx = cmds
        .iter()
        .position(|c| matches!(c, DrawCommand::PushLayer { .. }))
        .unwrap();
    let pop_idx = cmds
        .iter()
        .position(|c| matches!(c, DrawCommand::PopLayer))
        .unwrap();
    assert!(push_idx < pop_idx);
}

#[test]
fn linear_gradient_becomes_gradient_paint() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10">
        <defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="10" y2="0">
          <stop offset="0" stop-color="#000000"/><stop offset="1" stop-color="#ffffff"/>
        </linearGradient></defs>
        <rect width="10" height="10" fill="url(#g)"/></svg>"##;
    let data = SvgData::from_str(svg).unwrap();
    let cmds = data.commands_for(10.0, 10.0, None, None, ObjectFit::Contain);
    let (_, style) = only_path(&cmds);
    match style.fill {
        Some(Paint::Gradient(g)) => match g.kind {
            GradientKind::Linear { start, end } => {
                assert!((start.x - 0.0).abs() < 1e-2, "start.x {}", start.x);
                assert!((end.x - 10.0).abs() < 1e-2, "end.x {}", end.x);
            }
            other => panic!("expected linear gradient, got {other:?}"),
        },
        other => panic!("expected gradient fill, got {other:?}"),
    }
}

#[test]
fn filter_falls_back_to_raster_image() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10">
        <defs><filter id="b"><feGaussianBlur stdDeviation="1"/></filter></defs>
        <g filter="url(#b)"><rect width="10" height="10" fill="#ff0000"/></g></svg>"##;
    let data = SvgData::from_str(svg).unwrap();
    let cmds = data.commands_for(10.0, 10.0, None, None, ObjectFit::Contain);
    assert_eq!(
        cmds.len(),
        1,
        "fallback should be a single command: {cmds:?}"
    );
    match &cmds[0] {
        DrawCommand::Image { data, .. } => {
            // 10x10 fitted content at 2x density.
            assert_eq!(data.width, 20);
            assert_eq!(data.height, 20);
        }
        other => panic!("expected Image fallback, got {other:?}"),
    }
}

#[test]
fn tint_replaces_vector_paint() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><rect width="10" height="10" fill="#ff0000"/></svg>"##;
    let data = SvgData::from_str(svg).unwrap();
    let tint = Color::rgba(0.0, 0.0, 1.0, 1.0);
    let cmds = data.commands_for(10.0, 10.0, Some(tint), None, ObjectFit::Contain);
    let (_, style) = only_path(&cmds);
    match style.fill {
        Some(Paint::Solid(c)) => {
            assert!(c.b > 0.9 && c.r < 0.1 && c.g < 0.1, "tinted color {c:?}");
        }
        other => panic!("expected tinted solid fill, got {other:?}"),
    }
}

#[test]
fn stroke_override_replaces_every_stroke_width() {
    // A 1px stroke in a 10x10 viewBox rendered 1:1 (scale 1.0), so userspace widths map straight through.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><path d="M0 5 L10 5" stroke="#000000" stroke-width="1" fill="none"/></svg>"##;
    let data = SvgData::from_str(svg).unwrap();
    let width_of = |cmds: &[DrawCommand]| {
        only_path(cmds)
            .1
            .stroke
            .as_ref()
            .expect("stroked path")
            .width
    };

    // No override keeps the glyph's own width.
    let base = data.commands_for(10.0, 10.0, None, None, ObjectFit::Contain);
    assert!(
        (width_of(&base) - 1.0).abs() < 1e-3,
        "own width kept: {}",
        width_of(&base)
    );
    // The override forces the userspace width on every stroked path.
    let thick = data.commands_for(10.0, 10.0, None, Some(4.0), ObjectFit::Contain);
    assert!(
        (width_of(&thick) - 4.0).abs() < 1e-3,
        "override applied: {}",
        width_of(&thick)
    );
    // A different override is a distinct memo entry, not the cached one.
    assert!(!Arc::ptr_eq(&base, &thick));
}

#[test]
fn commands_for_is_memoized() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><rect width="10" height="10" fill="#ff0000"/></svg>"##;
    let data = SvgData::from_str(svg).unwrap();
    let a = data.commands_for(10.0, 10.0, None, None, ObjectFit::Contain);
    let b = data.commands_for(10.0, 10.0, None, None, ObjectFit::Contain);
    assert!(Arc::ptr_eq(&a, &b), "same args must return the same Arc");
    let c = data.commands_for(20.0, 20.0, None, None, ObjectFit::Contain);
    assert!(
        !Arc::ptr_eq(&a, &c),
        "different args must return a different Arc"
    );
}

#[test]
fn id_is_stable_across_instances() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><rect width="10" height="10"/></svg>"##;
    assert_eq!(
        SvgData::from_str(svg).unwrap().id(),
        SvgData::from_str(svg).unwrap().id()
    );
}

// Correctness guardrail: the baked path must reproduce the dynamic display list. Baking converts under an identity transform, so re-fitting `p * s + offset` at runtime must equal the dynamic `fit_ts.pre_concat(abs)` mapping. The math is bit-exact when the SVG's absolute transform is identity (no nested transforms), which every SVG below satisfies — so these assert EXACT `DrawCommand` equality, not an epsilon.
#[cfg(feature = "dynamic-svg")]
mod equivalence {
    use std::sync::Arc;

    use super::super::bake::bake;
    use super::{ObjectFit, SvgData};
    use renderer_core::{Color, DrawCommand};

    // Scales chosen so `sqrt(sx*sy) == s` exactly (integers / halves): the dynamic stroke width uses `uniform_scale = sqrt(det)` while the re-fit multiplies by the geometric-mean scale, and these agree bit-for-bit only when that square root is exact. This holds for every (size, fit) pair below — the aspect ratios are 1:1, 1:2 or 2:1, so `sx*sy` is a perfect square (1, 4, 9, 2.25) even under `fill`. Non-square aspect ratios (20x40, 40x20) force offset != 0 under contain/cover and stretch under fill.
    const SIZES: &[(f32, f32)] = &[
        (20.0, 20.0),
        (20.0, 40.0),
        (40.0, 20.0),
        (60.0, 60.0),
        (30.0, 30.0),
    ];

    // Every fit re-fits through the same shared `fit_params`, so baked and dynamic must agree exactly for each.
    const FITS: &[ObjectFit] = &[ObjectFit::Contain, ObjectFit::Cover, ObjectFit::Fill];

    fn tints() -> [Option<Color>; 3] {
        [
            None,
            Some(Color::rgba(0.0, 0.0, 1.0, 0.5)),
            Some(Color::rgba(1.0, 0.5, 0.0, 1.0)),
        ]
    }

    fn dynamic(
        svg: &str,
        w: f32,
        h: f32,
        tint: Option<Color>,
        fit: ObjectFit,
    ) -> Arc<Vec<DrawCommand>> {
        SvgData::from_str(svg)
            .unwrap()
            .commands_for(w, h, tint, None, fit)
    }

    fn baked(
        svg: &str,
        w: f32,
        h: f32,
        tint: Option<Color>,
        fit: ObjectFit,
    ) -> Arc<Vec<DrawCommand>> {
        let (size, baked) = bake(svg).unwrap();
        SvgData::from_baked(size, baked).commands_for(w, h, tint, None, fit)
    }

    fn assert_vector_equivalent(svg: &str) {
        for &(w, h) in SIZES {
            for tint in tints() {
                for &fit in FITS {
                    let d = dynamic(svg, w, h, tint, fit);
                    let b = baked(svg, w, h, tint, fit);
                    assert_eq!(
                        *d, *b,
                        "baked != dynamic at ({w}x{h}) tint={tint:?} fit={fit:?}\n dynamic={d:#?}\n baked={b:#?}"
                    );
                }
            }
        }
    }

    // A rasterized fallback can only match structurally, never byte-for-byte: `DrawCommand::Image`'s PartialEq compares `ImageData::id`, a fresh per-instance counter, and the dynamic fallback rasterizes at 2x the fitted (display-dependent) size while the bake rasterizes at 2x intrinsic. What must agree is placement: the same single Image drawn into the same content rect with the same filter.
    fn assert_raster_structurally_equivalent(svg: &str) {
        for &(w, h) in SIZES {
            for tint in tints() {
                for &fit in FITS {
                    let d = dynamic(svg, w, h, tint, fit);
                    let b = baked(svg, w, h, tint, fit);
                    assert_eq!(d.len(), 1, "dynamic raster should be one command");
                    assert_eq!(b.len(), 1, "baked raster should be one command");
                    match (&d[0], &b[0]) {
                        (
                            DrawCommand::Image {
                                rect: rd,
                                filter: fd,
                                ..
                            },
                            DrawCommand::Image {
                                rect: rb,
                                filter: fb,
                                ..
                            },
                        ) => {
                            assert_eq!(
                                rd, rb,
                                "raster content rect mismatch at ({w}x{h}) fit={fit:?}"
                            );
                            assert_eq!(fd, fb, "raster filter mismatch at ({w}x{h}) fit={fit:?}");
                        }
                        other => panic!("expected an Image fallback from both, got {other:?}"),
                    }
                }
            }
        }
    }

    const FILLS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><rect x="0" y="0" width="10" height="10" fill="#ff0000"/></svg>"##;
    const STROKE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" width="20" height="20"><rect x="2" y="2" width="16" height="16" fill="none" stroke="#00ff00" stroke-width="2"/></svg>"##;
    const GRADIENT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="10" y2="0"><stop offset="0" stop-color="#000000"/><stop offset="1" stop-color="#ffffff"/></linearGradient></defs><rect width="10" height="10" fill="url(#g)"/></svg>"##;
    const OPACITY_GROUP: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><g opacity="0.5"><rect width="10" height="10" fill="#00ff00"/></g></svg>"##;
    const RASTER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><defs><filter id="b"><feGaussianBlur stdDeviation="1"/></filter></defs><g filter="url(#b)"><rect width="10" height="10" fill="#ff0000"/></g></svg>"##;

    #[test]
    fn fills_only_equivalent() {
        assert_vector_equivalent(FILLS);
    }

    #[test]
    fn stroke_equivalent() {
        assert_vector_equivalent(STROKE);
    }

    #[test]
    fn linear_gradient_equivalent() {
        assert_vector_equivalent(GRADIENT);
    }

    #[test]
    fn opacity_group_equivalent() {
        assert_vector_equivalent(OPACITY_GROUP);
    }

    #[test]
    fn raster_fallback_equivalent() {
        assert_raster_structurally_equivalent(RASTER);
    }
}
