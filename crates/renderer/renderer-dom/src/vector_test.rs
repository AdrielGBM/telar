use super::*;
use renderer_core::{Color, FillRule, LineCap, Shadow, ShapeStyle};

fn drawing() -> Drawing {
    Drawing::new(7)
}

#[test]
fn an_empty_drawing_has_nothing_to_show() {
    assert_eq!(drawing().finish(), "");
}

#[test]
fn a_path_becomes_its_verbs() {
    let data = PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(10.0, 0.0))
        .quad_to(Point::new(15.0, 5.0), Point::new(10.0, 10.0))
        .cubic_to(
            Point::new(8.0, 12.0),
            Point::new(2.0, 12.0),
            Point::new(0.0, 10.0),
        )
        .close();
    let mut d = drawing();
    d.path(&data, &PathStyle::default().with_fill(Color::BLACK));
    let out = d.finish();
    assert!(
        out.contains("d=\"M0 0L10 0Q15 5 10 10C8 12 2 12 0 10Z\""),
        "{out}"
    );
    assert!(out.contains("fill=\"#000000\""), "{out}");
}

#[test]
fn a_path_with_no_fill_is_not_filled_black() {
    let data = PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(10.0, 10.0));
    let mut d = drawing();
    d.path(
        &data,
        &PathStyle::default().with_stroke(Stroke::new(Color::BLACK, 2.0).with_cap(LineCap::Round)),
    );
    let out = d.finish();
    assert!(out.contains("fill=\"none\""), "{out}");
    assert!(out.contains("stroke-width=\"2\""), "{out}");
    assert!(out.contains("stroke-linecap=\"round\""), "{out}");
}

#[test]
fn an_even_odd_fill_says_so() {
    let data = PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(4.0, 0.0))
        .close();
    let mut d = drawing();
    d.path(
        &data,
        &PathStyle::default()
            .with_fill(Color::WHITE)
            .with_fill_rule(FillRule::EvenOdd),
    );
    let svg = d.finish();
    assert!(svg.contains("fill-rule=\"evenodd\""), "{svg}");
}

#[test]
fn a_gradient_becomes_a_definition_the_shape_points_at() {
    let mut d = drawing();
    d.path(
        &PathData::new()
            .move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(4.0, 4.0)),
        &PathStyle::default().with_fill(Paint::Gradient(Gradient::linear(
            Point::new(0.0, 0.0),
            Point::new(0.0, 20.0),
            &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
        ))),
    );
    let out = d.finish();
    assert!(
        out.starts_with("<defs><linearGradient id=\"t7-1\""),
        "{out}"
    );
    assert!(out.contains("gradientUnits=\"userSpaceOnUse\""), "{out}");
    assert!(out.contains("y2=\"20\""), "{out}");
    assert!(out.contains("fill=\"url(#t7-1)\""), "{out}");
}

#[test]
fn a_uniform_radius_is_a_rect_and_a_mixed_one_is_a_path() {
    let mut d = drawing();
    d.rect(
        Rect::new(0.0, 0.0, 20.0, 10.0),
        &RectStyle::default()
            .with_fill(Color::BLACK)
            .with_radius(BorderRadius::all(3.0)),
    );
    assert!(
        d.finish()
            .contains("<rect x=\"0\" y=\"0\" width=\"20\" height=\"10\" rx=\"3\""),
        "a uniform radius stays a rect"
    );

    let mut d = drawing();
    d.rect(
        Rect::new(0.0, 0.0, 20.0, 10.0),
        &RectStyle::default()
            .with_fill(Color::BLACK)
            .with_radius(BorderRadius {
                top_left: 4.0,
                top_right: 0.0,
                bottom_right: 4.0,
                bottom_left: 0.0,
            }),
    );
    let out = d.finish();
    assert!(out.starts_with("<path d=\"M4 0"), "{out}");
    assert!(out.contains("A4 4 0 0 1"), "{out}");
}

#[test]
fn a_border_is_drawn_inside_the_box_a_stroke_would_straddle() {
    let mut d = drawing();
    d.rect(
        Rect::new(0.0, 0.0, 20.0, 20.0),
        &RectStyle::default().with_border(renderer_core::Border::uniform(Color::BLACK, 4.0)),
    );
    let out = d.finish();
    assert!(
        out.contains("x=\"2\" y=\"2\" width=\"16\" height=\"16\""),
        "{out}"
    );
    assert!(out.contains("stroke-width=\"4\""), "{out}");
}

#[test]
fn a_shadow_becomes_a_filter() {
    let mut d = drawing();
    d.rect(
        Rect::new(0.0, 0.0, 10.0, 10.0),
        &RectStyle::default()
            .with_fill(Color::BLACK)
            .with_shadow(Shadow {
                offset_x: 0.0,
                offset_y: 2.0,
                blur_radius: 8.0,
                spread: 0.0,
                color: Color::BLACK,
            }),
    );
    let out = d.finish();
    assert!(
        out.contains("<feDropShadow dx=\"0\" dy=\"2\" stdDeviation=\"4\""),
        "{out}"
    );
    assert!(out.contains("filter=\"url(#t7-1)\""), "{out}");
}

#[test]
fn text_carries_the_css_a_box_would_have_given_it() {
    let mut d = drawing();
    d.text(
        "fill & stroke",
        Rect::new(2.0, 4.0, 100.0, 14.0),
        &TextStyle::new(11.0, Color::BLACK),
    );
    let out = d.finish();
    assert!(
        out.contains("<foreignObject x=\"2\" y=\"4\" width=\"100\" height=\"14\">"),
        "{out}"
    );
    assert!(
        out.contains("xmlns=\"http://www.w3.org/1999/xhtml\""),
        "{out}"
    );
    assert!(out.contains("font-size:11px"), "{out}");
    assert!(out.contains(">fill &amp; stroke<"), "{out}");
}

#[test]
fn groups_close_in_the_order_they_opened() {
    let mut d = drawing();
    d.open_layer(0.5);
    d.open_matrix([2.0, 0.0, 0.0, 2.0, 4.0, 4.0]);
    d.line(
        Point::new(0.0, 0.0),
        Point::new(4.0, 4.0),
        &Stroke::new(Color::BLACK, 1.0),
    );
    d.close_group();
    let out = d.finish();
    assert_eq!(
        out,
        "<g opacity=\"0.5\"><g transform=\"matrix(2,0,0,2,4,4)\"><line x1=\"0\" y1=\"0\" x2=\"4\" y2=\"4\" stroke=\"#000000\" stroke-width=\"1\"/></g></g>"
    );
}

#[test]
fn a_pop_with_nothing_open_closes_nothing() {
    let mut d = drawing();
    d.close_group();
    d.line(
        Point::new(0.0, 0.0),
        Point::new(1.0, 1.0),
        &Stroke::new(Color::BLACK, 1.0),
    );
    let svg = d.finish();
    assert!(
        !svg.contains("</g>"),
        "a pop with nothing open must close nothing: {svg}"
    );
}

#[test]
fn a_clip_is_a_definition_the_group_points_at() {
    let mut d = drawing();
    d.open_clip(Rect::new(0.0, 0.0, 30.0, 30.0), BorderRadius::all(6.0));
    d.image(
        "data:image/png;base64,AAA",
        Rect::new(0.0, 0.0, 30.0, 30.0),
        Raster::Pixel,
    );
    let out = d.finish();
    assert!(out.contains("<clipPath id=\"t7-1\"><rect x=\"0\" y=\"0\" width=\"30\" height=\"30\" rx=\"6\"/></clipPath>"), "{out}");
    assert!(out.contains("<g clip-path=\"url(#t7-1)\">"), "{out}");
    assert!(out.contains("image-rendering:pixelated"), "{out}");
    assert!(out.ends_with("</g>"), "{out}");
}

#[test]
fn a_collapsed_rect_draws_nothing() {
    let mut d = drawing();
    d.rect(
        Rect::new(0.0, 0.0, 0.0, 10.0),
        &RectStyle::default().with_fill(Color::BLACK),
    );
    assert_eq!(d.finish(), "");
}

/// The ring the rasterising backends fill, as a picture that can be laid over the box: outer outline, inner outline, even-odd. Drawn in the box's own corner, and the gradient moved to meet it — measured where the widget drew it, the colours would have started off the left edge of the picture.
#[test]
fn a_gradient_frame_is_a_ring_drawn_in_the_box_s_own_corner() {
    let svg = frame_svg(
        Rect::new(300.0, 500.0, 100.0, 40.0),
        BorderRadius::all(8.0),
        [2.0; 4],
        &Gradient::linear(
            Point::new(300.0, 500.0),
            Point::new(400.0, 500.0),
            &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
        ),
    );
    assert!(svg.contains("width=\"100\" height=\"40\""), "{svg}");
    assert!(svg.contains("fill-rule=\"evenodd\""), "{svg}");
    assert!(
        svg.contains("x1=\"0\" y1=\"0\" x2=\"100\" y2=\"0\""),
        "{svg}"
    );
    assert!(svg.contains("d=\"M8 0"), "{svg}");
    assert!(svg.contains("M8 2"), "{svg}");
    assert!(svg.contains("A6 6 0 0 1"), "{svg}");
}

/// A frame thicker than the box it frames has no interior to punch out, and the outline alone is the whole of it — a second subpath of nothing would have cut a hole under the even-odd rule.
#[test]
fn a_frame_that_swallows_its_box_is_the_outline_alone() {
    let svg = frame_svg(
        Rect::new(0.0, 0.0, 6.0, 6.0),
        BorderRadius::zero(),
        [4.0; 4],
        &Gradient::linear(
            Point::new(0.0, 0.0),
            Point::new(6.0, 0.0),
            &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
        ),
    );
    assert_eq!(svg.matches('M').count(), 1, "{svg}");
}
