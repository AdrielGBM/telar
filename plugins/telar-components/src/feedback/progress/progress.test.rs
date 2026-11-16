use telar::{
    Accessible, BorderRadius, Color, ControlSize, Direction, NodeId, RectStyle, Role, ShapeStyle,
    SurfaceAmendment, signal,
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
    let widget = progress(
        ProgressProps::props()
            .value(value)
            .width(200.0)
            .height(10.0)
            .build(),
    )
    .unwrap();
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
    let bar = progress(ProgressProps::props().value(fraction).stretch(true).build());
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

#[test]
fn the_bar_is_a_pill_until_the_tokens_name_a_radius() {
    telar::set_theme(crate::test_support::TokensTheme::<ProgressTokens> {
        role: 7.0,
        tokens: None,
    });
    let tree = bar(ProgressProps::props().width(120.0).height(10.0).build());
    assert_eq!(
        crate::test_support::first_painted(&tree).radius,
        BorderRadius::all(5.0),
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
fn an_unset_size_is_the_tokens_and_a_named_one_wins() {
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
    assert_eq!(
        track(&bar(ProgressProps::props().width(90.0).height(4.0).build())),
        (90.0, 4.0),
        "the bar's own size wins"
    );
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
        let widget = progress(ProgressProps::props().value(value).width(200.0).build())
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
