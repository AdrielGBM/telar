use super::*;

#[test]
fn triangle_absolute() {
    let built = parse_path_data("M0,0 L100,0 L50,80 Z").unwrap();
    assert_eq!(
        built.chain,
        "PathData::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(100.0, 0.0)).line_to(Point::new(50.0, 80.0)).close()"
    );
    assert_eq!(built.max_x, 100.0);
    assert_eq!(built.max_y, 80.0);
}

#[test]
fn implicit_lineto_after_moveto() {
    let built = parse_path_data("M0 0 10 0 10 10").unwrap();
    assert_eq!(
        built.chain,
        "PathData::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0)).line_to(Point::new(10.0, 10.0))"
    );
}

#[test]
fn relative_commands_resolve_to_absolute() {
    let built = parse_path_data("m10,10 l10,0 l0,10 z").unwrap();
    assert_eq!(
        built.chain,
        "PathData::new().move_to(Point::new(10.0, 10.0)).line_to(Point::new(20.0, 10.0)).line_to(Point::new(20.0, 20.0)).close()"
    );
}

#[test]
fn horizontal_and_vertical() {
    let built = parse_path_data("M0,0 H50 V50 h-50 v-50 Z").unwrap();
    assert_eq!(
        built.chain,
        "PathData::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(50.0, 0.0)).line_to(Point::new(50.0, 50.0)).line_to(Point::new(0.0, 50.0)).line_to(Point::new(0.0, 0.0)).close()"
    );
}

#[test]
fn cubic_and_quadratic() {
    let built = parse_path_data("M0,0 C10,0 20,10 20,20 Q30,30 40,20").unwrap();
    assert!(
        built.chain.contains(
            ".cubic_to(Point::new(10.0, 0.0), Point::new(20.0, 10.0), Point::new(20.0, 20.0))"
        ),
        "{}",
        built.chain
    );
    assert!(
        built
            .chain
            .contains(".quad_to(Point::new(30.0, 30.0), Point::new(40.0, 20.0))"),
        "{}",
        built.chain
    );
}

#[test]
fn negative_and_decimal_no_separator() {
    let built = parse_path_data("M1.5.5L-3-4").unwrap();
    assert_eq!(
        built.chain,
        "PathData::new().move_to(Point::new(1.5, 0.5)).line_to(Point::new(-3.0, -4.0))"
    );
}

#[test]
fn missing_moveto_errors() {
    assert!(
        parse_path_data("L10,10").is_err(),
        "a path has to start somewhere"
    );
}

#[test]
fn unsupported_command_errors() {
    assert!(
        parse_path_data("M0,0 S10,10 20,20").is_err(),
        "a smooth curve is not supported, and saying so beats emitting nothing"
    );
}

#[test]
fn a_dash_pattern_reads_spaces_and_commas() {
    assert_eq!(parse_dash_pattern("1 4").unwrap(), vec![1.0, 4.0]);
    assert_eq!(parse_dash_pattern("1, 4,2.5").unwrap(), vec![1.0, 4.0, 2.5]);
    assert_eq!(parse_dash_pattern("3").unwrap(), vec![3.0]);
}

#[test]
fn a_dash_pattern_that_draws_nothing_is_refused() {
    assert!(parse_dash_pattern("").is_err());
    assert!(parse_dash_pattern("0 0").is_err());
    assert!(parse_dash_pattern("1 -4").is_err());
    assert!(parse_dash_pattern("1 four").is_err());
    assert!(parse_dash_pattern("1 2 3 4 5 6 7 8 9").is_err());
    assert!(parse_dash_pattern("1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16").is_ok());
}

fn view_code(view: &str) -> String {
    crate::transpile_source(&format!("[view]\n{view}\n"), "demo", None, None)
        .unwrap()
        .rust_code
}

fn near(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
}

fn arc(
    from: (f32, f32),
    radii: (f32, f32),
    large_arc: bool,
    sweep: bool,
    to: (f32, f32),
) -> Vec<Segment> {
    EllipticalArc {
        from,
        radii,
        rotation: 0.0,
        large_arc,
        sweep,
        to,
    }
    .segments()
}

#[test]
fn a_quarter_arc_is_one_cubic_on_its_circle() {
    let segments = arc((10.0, 0.0), (10.0, 10.0), false, true, (0.0, 10.0));
    let [Segment::Cubic(c1, c2, end)] = segments.as_slice() else {
        panic!("a quarter turn is one curve: {segments:?}");
    };
    let handle = 10.0 * 4.0 / 3.0 * (std::f32::consts::PI / 8.0).tan();
    assert!(near(*c1, (10.0, handle)), "{c1:?}");
    assert!(near(*c2, (handle, 10.0)), "{c2:?}");
    assert_eq!(
        *end,
        (0.0, 10.0),
        "the arc ends exactly where it was told to"
    );
}

#[test]
fn a_half_arc_is_two_quarter_turns_through_the_sweep_side() {
    let clockwise = arc((0.0, 0.0), (5.0, 5.0), false, true, (10.0, 0.0));
    let [Segment::Cubic(_, _, mid), Segment::Cubic(_, _, end)] = clockwise.as_slice() else {
        panic!("half a turn is two curves: {clockwise:?}");
    };
    assert!(near(*mid, (5.0, -5.0)), "{mid:?}");
    assert_eq!(*end, (10.0, 0.0));
    let counter = arc((0.0, 0.0), (5.0, 5.0), false, false, (10.0, 0.0));
    let [Segment::Cubic(_, _, mid), _] = counter.as_slice() else {
        panic!("{counter:?}");
    };
    assert!(
        near(*mid, (5.0, 5.0)),
        "the other sweep bulges the other way: {mid:?}"
    );
}

#[test]
fn the_large_arc_flag_takes_the_long_way_round() {
    let segments = arc((10.0, 0.0), (10.0, 10.0), true, true, (0.0, 10.0));
    assert_eq!(segments.len(), 3, "three quarters of a turn: {segments:?}");
}

#[test]
fn radii_too_small_to_reach_are_scaled_until_they_do() {
    let small = arc((0.0, 0.0), (1.0, 1.0), false, true, (10.0, 0.0));
    let [Segment::Cubic(_, _, mid), _] = small.as_slice() else {
        panic!("{small:?}");
    };
    assert!(
        near(*mid, (5.0, -5.0)),
        "the smallest circle through both ends: {mid:?}"
    );
}

#[test]
fn a_zero_radius_arc_is_a_line_and_one_that_goes_nowhere_is_nothing() {
    assert_eq!(
        parse_path_data("M0,0 A0,5 0 0 1 10,0").unwrap().chain,
        "PathData::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0))"
    );
    assert_eq!(
        parse_path_data("M3,4 A5,5 0 0 1 3,4").unwrap().chain,
        "PathData::new().move_to(Point::new(3.0, 4.0))"
    );
}

#[test]
fn a_relative_arc_with_its_flags_run_together_reads_like_the_absolute_one() {
    assert_eq!(
        parse_path_data("M0,0 a5 5 0 0110 0").unwrap().chain,
        parse_path_data("M0,0 A5,5 0 0 1 10,0").unwrap().chain
    );
}

#[test]
fn an_arc_flag_that_is_not_a_digit_is_refused() {
    assert!(parse_path_data("M0,0 A5,5 0 2 1 10,0").is_err());
    assert!(parse_path_data("M0,0 A5,5 0 0").is_err());
}

#[test]
fn an_arc_reaches_its_bulge_in_the_extent() {
    let built = parse_path_data("M0,10 A10,10 0 0 0 20,10").unwrap();
    assert!((built.max_y - 20.0).abs() < 1e-3, "{}", built.max_y);
    assert_eq!(built.max_x, 20.0);
}

#[test]
fn a_view_box_is_four_numbers_with_an_area() {
    assert_eq!(parse_view_box("0 0 24 24").unwrap(), [0.0, 0.0, 24.0, 24.0]);
    assert_eq!(
        parse_view_box("-2,1.5, 20 10").unwrap(),
        [-2.0, 1.5, 20.0, 10.0]
    );
    assert!(parse_view_box("0 0 24").is_err());
    assert!(parse_view_box("0 0 24 twenty").is_err());
    assert!(parse_view_box("0 0 0 24").is_err());
    assert!(parse_view_box("0 0 24 -1").is_err());
}

#[test]
fn a_view_box_path_is_fitted_to_the_rect_it_is_laid_out_in() {
    let code =
        view_code("path d:\"M2,12 L22,12\" view_box:\"0 0 24 24\" stroke:#000000 stroke_width:2");
    assert!(
        code.contains("Canvas::declaring(") && code.contains(".width(24.0).height(24.0)"),
        "the box is the view box's size when nothing else gives one:\n{code}"
    );
    assert!(
        code.contains("__path_data.fitted(Rect::new(0.0, 0.0, 24.0, 24.0), __rect)"),
        "the data is scaled from the view box into the laid-out rect each paint:\n{code}"
    );
    assert!(!code.contains("Path::static_data"), "{code}");
    assert!(!code.contains("compile_error!"), "{code}");
}

#[test]
fn a_view_box_path_given_one_side_keeps_its_proportions() {
    let code = view_code("path d:\"M0,0 L20,10\" view_box:\"0 0 20 10\" stroke:#000000 width:40");
    assert!(
        code.contains(".width(40.0)") && code.contains(".aspect_ratio(2.0)"),
        "{code}"
    );
    let both = view_code(
        "path d:\"M0,0 L20,10\" view_box:\"0 0 20 10\" stroke:#000000 width:40 height:40",
    );
    assert!(!both.contains("aspect_ratio"), "{both}");
    assert!(!both.contains(".width(20.0)"), "{both}");
}

#[test]
fn a_view_box_that_cannot_be_one_is_a_compile_error() {
    let code = view_code("path d:\"M0,0 L1,1\" view_box:\"0 0 24\" stroke:#000000");
    assert!(
        code.contains("compile_error!(\"rsx: `view_box` is four numbers"),
        "{code}"
    );
}

#[test]
fn a_path_without_a_view_box_is_drawn_in_its_own_pixels() {
    let code = view_code("path d:\"M0,0 L10,8\" stroke:#000000 stroke_width:2");
    assert!(
        code.contains("Path::static_data(") && code.contains(".width(10.0).height(8.0)"),
        "{code}"
    );
    assert!(!code.contains("fitted("), "{code}");
}

#[test]
fn a_path_sized_by_its_class_keeps_that_size() {
    let src = "[style]\n@icon\n    width:16\n    height:16\n[view]\npath @icon d:\"M0,0 L24,24\" view_box:\"0 0 24 24\" stroke:#000000\n";
    let code = crate::transpile_source(src, "demo", None, None)
        .unwrap()
        .rust_code;
    assert!(!code.contains(".width(24.0)"), "{code}");
    assert!(!code.contains("aspect_ratio"), "{code}");
}

#[test]
fn stroke_cap_shapes_the_ends_of_the_stroke() {
    let code = view_code("path d:\"M0,0 L10,0\" stroke:#000000 stroke_width:2 stroke_cap:round");
    assert!(code.contains(", 2.0).with_cap(LineCap::Round))"), "{code}");
    let dashed = view_code(
        "path d:\"M0,0 L10,0\" stroke:#000000 stroke_width:2 stroke_dash:\"1 4\" stroke_cap:square",
    );
    assert!(
        dashed.contains(".with_dash(&[1.0, 4.0], 0.0).with_cap(LineCap::Square))"),
        "{dashed}"
    );
}

#[test]
fn stroke_cap_names_only_a_cap_and_needs_a_stroke() {
    let misspelled = view_code("path d:\"M0,0 L10,0\" stroke:#000000 stroke_cap:rounded");
    assert!(
        misspelled.contains("compile_error!(\"`stroke_cap:rounded` is not a value of `stroke_cap`"),
        "{misspelled}"
    );
    assert!(!misspelled.contains("with_cap"), "{misspelled}");
    let strokeless = view_code("path d:\"M0,0 L10,0\" fill:#000000 stroke_cap:round");
    assert!(
        strokeless.contains("compile_error!(\"rsx: `stroke_cap` shapes the ends of the `stroke`"),
        "{strokeless}"
    );
}

#[test]
fn a_stroke_with_no_width_takes_the_theme_strong_border_width() {
    let code = crate::transpile_source(
        "[view]\npath d:\"M0,0 L10,0\" stroke:#000000\n",
        "demo",
        Some("app::AppTheme"),
        None,
    )
    .unwrap()
    .rust_code;
    assert!(
        code.contains(", ::telar::use_theme_tokens().border_width_strong())"),
        "{code}"
    );
    let library = library_code("[view]\npath d:\"M0,0 L10,0\" stroke:#000000\n");
    assert!(
        library.contains(", ::telar::use_theme_tokens().border_width_strong())"),
        "{library}"
    );
}

#[test]
fn a_path_paints_in_the_ink_of_the_text_it_is_drawn_in() {
    let code = view_code("path d:\"M0,0 L10,0\" stroke:$text.ink stroke_width:2");
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(
        code.contains("Canvas::declaring(")
            && code.contains("move |__rect: Rect, __text_style: TextStyle|"),
        "reading its text draws the path where the inherited text style is handed over:\n{code}"
    );
    assert!(
        code.contains("let __text = Reactive::Const(BoxText::drawn_in(&__text_style));"),
        "{code}"
    );
    assert!(
        code.contains("Stroke::new(__text.get().ink, 2.0)"),
        "{code}"
    );
    assert!(
        code.contains("RenderNode::path(__path_data.clone(), "),
        "{code}"
    );
}

#[test]
fn a_path_reads_its_text_only_where_it_paints() {
    let code = view_code("path d:\"M0,0 L10,0\" stroke:#000000 width:($text.em * 2.0)");
    assert!(
        code.contains("compile_error!(\"`width:($text.em * 2.0)` on `path`: `$text` is"),
        "{code}"
    );
    let text = view_code("text \"x\" color:$text.ink");
    assert!(
        text.contains("compile_error!(\"`color:$text.ink` on `text`"),
        "{text}"
    );
}

fn library_code(source: &str) -> String {
    let document = telar_parser::parse(source).expect("the fixture parses");
    crate::codegen::transpile(crate::codegen::TranspileInput {
        document: &document,
        source,
        component_name: "kit",
        theme_type: None,
        assets: None,
        prelude: &[],
        library: true,
        hot_reload: false,
        rsx_path: None,
        imports: &crate::style_import::ResolvedImports::default(),
    })
    .expect("the fixture transpiles")
    .rust_code
}
