use telar::{
    Accessible, BorderRadius, Color, ControlSize, Direction, LayoutItem, NodeId, RectStyle, Role,
    ShapeStyle, SurfaceAmendment, Tone, signal,
};

use crate::{ProgressProps, ProgressTokens, progress};

fn lay_out(node: NodeId) {
    telar::testing::lay_out(node, 300.0, 100.0);
}

#[test]
fn uncontrolled_progress_builds_with_default_value() {
    crate::test_support::fresh_layout_runtime();
    let result = progress(ProgressProps::props().build());
    assert!(
        result.is_ok(),
        "an uncontrolled progress builds from its default"
    );
}

#[test]
fn controlled_progress_builds_and_layouts() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(0.3f32);
    let widget = progress(ProgressProps::props().value(value).build()).unwrap();
    lay_out(widget.layout_node());
    assert_eq!(value.get(), 0.3);
}

#[test]
fn setting_value_after_build_does_not_panic() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(0.0f32);
    let widget = progress(ProgressProps::props().value(value).build()).unwrap();
    lay_out(widget.layout_node());
    value.set(0.75);
    value.set(1.5);
    assert_eq!(value.get(), 1.5);
}

/// The reason this prop is a closure. A reading derived from two services has no signal behind it, and a bar that insisted on one is a bar an application reimplements next to the catalogue.
#[test]
fn a_derived_reading_can_drive_the_bar() {
    telar::reset_runtime();
    let used = signal(3.0f32);
    let total = signal(4.0f32);
    let fraction = telar::derive_pair(used, total, |u, t| u / t);
    let _read = fraction;
    let bar = progress(ProgressProps::props().value(fraction).build());
    assert!(bar.is_ok(), "a derivation drives it");
    used.set(1.0);
    assert_eq!(fraction.get(), 0.25, "and keeps following its sources");
}

fn bar(props: ProgressProps) -> telar::testing::Scoped<telar::ComponentList> {
    crate::test_support::fresh_layout_runtime();
    telar::testing::scoped(|| {
        let widget = progress(props).unwrap();
        lay_out(widget.layout_node());
        telar::ComponentList::new(widget)
    })
}

/// A bar its call places at the full width of what holds it, as `width:100%` on the call does.
fn full_width_bar(props: ProgressProps) -> telar::testing::Scoped<telar::ComponentList> {
    crate::test_support::fresh_layout_runtime();
    telar::testing::scoped(|| {
        let widget = progress(props).unwrap();
        telar::place_item(widget.layout_node(), |style| {
            style.width(telar::SizeDimension::Percent(1.0))
        });
        lay_out(widget.layout_node());
        telar::ComponentList::new(widget)
    })
}

#[test]
fn the_bar_is_a_pill_until_the_tokens_name_a_radius() {
    telar::set_theme(crate::test_support::TokensTheme::<ProgressTokens> {
        role: 7.0,
        tokens: None,
    });
    let tree = bar(ProgressProps::props().build());
    assert_eq!(
        crate::test_support::first_painted(&tree).radius,
        BorderRadius::all(crate::metrics::gap_md(ControlSize::Regular) / 2.0),
        "fully rounded at whatever thickness it is laid out"
    );

    telar::set_theme(crate::test_support::TokensTheme {
        role: 7.0,
        tokens: Some(ProgressTokens {
            radius: Some(2.0),
            ..ProgressTokens::default()
        }),
    });
    assert_eq!(
        crate::test_support::first_painted(&tree).radius,
        BorderRadius::all(2.0)
    );
}

#[test]
fn the_tokens_lengths_are_the_bars_scaled_to_its_size() {
    telar::set_theme(crate::test_support::TokensTheme {
        role: 7.0,
        tokens: Some(ProgressTokens {
            width: Some(150.0),
            height: Some(6.0),
            ..ProgressTokens::default()
        }),
    });
    let track = |tree: &telar::ComponentList| {
        tree.commands()
            .iter()
            .find_map(|command| match command {
                telar::DrawCommand::Rect { rect, .. } => Some((rect.width, rect.height)),
                _ => None,
            })
            .expect("the track is drawn")
    };
    assert_eq!(track(&bar(ProgressProps::props().build())), (150.0, 6.0));
    let (width, height) = track(&bar(ProgressProps::props()
        .size(ControlSize::Small)
        .build()));
    assert_eq!(width, (150.0 * ControlSize::Small.text_scale()).round());
    assert!(
        (height - 6.0 * ControlSize::Small.scale()).abs() <= 0.5,
        "the tokens' lengths are the regular size's, scaled to the bar's, snapped to the pixel grid: {height}"
    );
}

/// An unset bar is the measure and the medium gap of its own size.
#[test]
fn the_bar_scales_with_its_size() {
    telar::set_theme(crate::test_support::TokensTheme::<ProgressTokens> {
        role: 7.0,
        tokens: None,
    });
    for size in [ControlSize::Small, ControlSize::Regular, ControlSize::Large] {
        let tree = bar(ProgressProps::props().size(size).build());
        let track = tree
            .commands()
            .iter()
            .find_map(|command| match command {
                telar::DrawCommand::Rect { rect, .. } => Some((rect.width, rect.height)),
                _ => None,
            })
            .expect("the track is drawn");
        assert_eq!(
            track,
            (
                crate::metrics::measure(size, 16.0),
                crate::metrics::gap_md(size)
            ),
            "{size:?}"
        );
    }
}

fn named_bar(value: f32) -> telar::testing::Scoped<telar::ComponentList> {
    crate::test_support::fresh_layout_runtime();
    telar::testing::scoped(|| {
        let widget = progress(ProgressProps::props().value(value).build())
            .unwrap()
            .a11y_label(|| "Upload");
        lay_out(widget.layout_node());
        telar::ComponentList::new(widget)
    })
}

/// A reader hears the bar as what it is, what it was called and how far it has filled, though the keyboard never stops at it.
#[test]
fn a_reader_hears_the_bar_its_name_and_how_far_it_has_filled() {
    let tree = named_bar(0.4);
    let node = telar::inspect::snapshot(&tree.commands())
        .into_iter()
        .find(|node| node.role == Role::ProgressBar)
        .expect("the bar is read");
    assert_eq!(node.name, "Upload");
    let reading = node
        .value
        .as_ref()
        .and_then(telar::Value::reading)
        .expect("the bar carries a number");
    assert!((reading.now() - 40.0).abs() < 1e-3, "{reading:?}");
    assert_eq!((reading.min(), reading.max()), (0.0, 100.0));
    assert_eq!(node.id, None, "no keyboard stop");
}

/// Which edges of the track the fill of a bar 40% full touches on a surface reading `direction`: its left, its right.
fn fill_touches(direction: Direction) -> (bool, bool) {
    telar::set_direction(direction);
    let tree = named_bar(0.4);
    let mut drawn = Vec::new();
    telar::for_each_with_matrix(&tree.commands(), |command, matrix| {
        if let telar::DrawCommand::Rect { rect, .. } = command {
            drawn.push(telar::transform_clip_rect(matrix, *rect));
        }
    });
    telar::set_direction(Direction::Ltr);
    let [track, fill] = drawn[..] else {
        panic!("the track and its fill are drawn: {drawn:?}");
    };
    let near = |a: f32, b: f32| (a - b).abs() < 0.5;
    (
        near(fill.x, track.x),
        near(fill.x + fill.width, track.x + track.width),
    )
}

#[test]
fn the_fill_starts_at_the_inline_start_of_the_track() {
    assert_eq!(
        fill_touches(Direction::Ltr),
        (true, false),
        "from the left on a left-to-right surface"
    );
    assert_eq!(
        fill_touches(Direction::Rtl),
        (false, true),
        "from the right on a right-to-left one"
    );
}

#[test]
fn a_style_amendment_restyles_the_track() {
    let ink = Color::rgb(0.3, 0.3, 0.9);
    let tree = bar(ProgressProps::props()
        .style(std::rc::Rc::new(move |paint: RectStyle| paint.with_fill(ink)) as SurfaceAmendment)
        .build());
    assert_eq!(
        crate::test_support::first_painted(&tree).fill,
        Some(ink.into())
    );
}

#[test]
fn a_bar_placed_at_the_full_width_is_as_wide_as_what_holds_it() {
    let tree = full_width_bar(ProgressProps::props().build());
    let width = tree.commands().iter().find_map(|command| match command {
        telar::DrawCommand::Rect { rect, .. } => Some(rect.width),
        _ => None,
    });
    assert_eq!(width, Some(300.0));
}

#[test]
fn a_tone_colours_the_fill_whole_and_the_track_with_a_wash_of_it() {
    let fills = |tone: Tone| {
        bar(ProgressProps::props().tone(tone).build())
            .commands()
            .iter()
            .filter_map(|command| match command {
                telar::DrawCommand::Rect { style, .. } => style.fill.map(|fill| fill.solid_color()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let theme = telar::use_theme_tokens();
    for (tone, hue) in [
        (Tone::Danger, theme.error()),
        (Tone::default(), theme.primary()),
    ] {
        let [track, fill] = fills(tone)[..] else {
            panic!("the track and its fill are drawn");
        };
        assert_eq!(fill, hue);
        assert_eq!(track.with_alpha(hue.a), hue, "the same colour");
        assert!(track.a < hue.a, "let through: {track:?}");
    }
}

fn rects(tree: &telar::ComponentList) -> Vec<(telar::Rect, RectStyle)> {
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            telar::DrawCommand::Rect { rect, style } => Some((*rect, **style)),
            _ => None,
        })
        .collect()
}

/// The rect a clip cuts to, and its corner.
type Cut = (telar::Rect, BorderRadius);

/// Every box `tree` draws, with the cut it is drawn under, innermost, if any.
fn rects_and_cuts(tree: &telar::ComponentList) -> Vec<(telar::Rect, RectStyle, Option<Cut>)> {
    let mut cuts = Vec::new();
    let mut drawn = Vec::new();
    for command in tree.commands().iter() {
        match command {
            telar::DrawCommand::PushClip { rect, radius } => cuts.push((*rect, *radius)),
            telar::DrawCommand::PopClip => {
                cuts.pop();
            }
            telar::DrawCommand::Rect { rect, style } => {
                drawn.push((*rect, **style, cuts.last().copied()));
            }
            _ => {}
        }
    }
    drawn
}

#[test]
fn the_fill_is_a_rounded_bar_of_its_own_as_wide_as_the_reading_and_cut_by_the_track_s_corner() {
    let tree = full_width_bar(ProgressProps::props().value(0.25).build());
    let [
        (track, track_style, _),
        (fill, fill_style, Some((cut, corner))),
    ] = rects_and_cuts(&tree)[..]
    else {
        panic!("the track and its fill are drawn, the fill under a cut");
    };
    assert_eq!((track.width, fill.width), (300.0, 75.0));
    assert_eq!(fill.height, track.height);
    assert_eq!(cut, track, "the track cuts what it holds");
    assert_eq!(corner, track_style.radius, "along the corner it paints");
    assert!(
        (fill_style.radius.top_right - track_style.radius.top_right).abs() < 0.01,
        "its leading end is round as the track's ends are: {:?}",
        fill_style.radius
    );
}

/// The track cuts its fill, so whatever reshapes the track — the theme's tokens, a caller's amendment — reshapes the fill's outer ends with it, and a caller restyling the corner says so once.
#[test]
fn reshaping_the_track_reshapes_the_cut_of_its_fill() {
    let fill_cut = |tree: &telar::ComponentList| match rects_and_cuts(tree)[..] {
        [_, (_, _, Some((_, corner)))] => corner,
        _ => panic!("the fill is drawn under the track's cut"),
    };
    telar::set_theme(crate::test_support::TokensTheme {
        role: 7.0,
        tokens: Some(ProgressTokens {
            radius: Some(2.0),
            ..ProgressTokens::default()
        }),
    });
    assert_eq!(
        fill_cut(&bar(ProgressProps::props().value(0.5).build())),
        BorderRadius::all(2.0)
    );

    let tree = bar(ProgressProps::props()
        .value(0.5)
        .style(
            std::rc::Rc::new(|paint: RectStyle| paint.with_radius(BorderRadius::all(1.0)))
                as SurfaceAmendment,
        )
        .build());
    assert_eq!(fill_cut(&tree), BorderRadius::all(1.0));
}

#[test]
fn a_fill_style_restyles_the_fill_alone() {
    let ink = Color::rgb(0.3, 0.3, 0.9);
    let untouched = rects(&bar(ProgressProps::props().value(0.5).build()))[0]
        .1
        .fill;
    let tree = bar(ProgressProps::props()
        .value(0.5)
        .fill_style(
            std::rc::Rc::new(move |paint: RectStyle| paint.with_fill(ink)) as SurfaceAmendment,
        )
        .build());
    let [(_, track), (_, fill)] = rects(&tree)[..] else {
        panic!("the track and its fill are drawn");
    };
    assert_eq!(fill.fill, Some(ink.into()));
    assert_eq!(track.fill, untouched);
}

/// The width the tokens give a bar is the one it takes where there is room: in a column narrower than that, it is the column's width rather than past its edge.
#[test]
fn a_bar_in_a_narrower_column_is_no_wider_than_it() {
    crate::test_support::fresh_layout_runtime();
    let tree = telar::testing::scoped(|| {
        let widget = progress(ProgressProps::props().build()).unwrap();
        let column =
            telar::Container::new(telar::LayoutStyle::column().width(80.0), vec![widget]).unwrap();
        lay_out(column.layout_node());
        telar::ComponentList::new(column)
    });
    let [(track, _), _] = rects(&tree)[..] else {
        panic!("the track and its fill are drawn");
    };
    assert_eq!(track.width, 80.0);
}
