use telar::{NodeId, signal};

use super::*;

fn lay_out(node: NodeId) {
    telar::testing::lay_out(node, 300.0, 100.0);
}

#[test]
fn uncontrolled_progress_builds_with_default_value() {
    crate::test_support::fresh_layout_runtime();
    let result = progress(ProgressProps::props().build(), Children::default());
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
        Children::default(),
    )
    .unwrap();
    lay_out(widget.layout_node());
    assert_eq!(value.get(), 0.3);
}

#[test]
fn setting_value_after_build_does_not_panic() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(0.0f32);
    let widget = progress(
        ProgressProps::props().value(value).build(),
        Children::default(),
    )
    .unwrap();
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
    let bar = progress(
        ProgressProps::props().value(fraction).stretch(true).build(),
        Children::default(),
    );
    assert!(bar.is_ok(), "a derivation drives it");
    used.set(1.0);
    assert_eq!(fraction.get(), 0.25, "and keeps following its sources");
}

fn bar(props: ProgressProps) -> telar::testing::Scoped<telar::ComponentList> {
    crate::test_support::fresh_layout_runtime();
    telar::testing::scoped(|| {
        let widget = progress(props, Children::default()).unwrap();
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
