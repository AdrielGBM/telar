use telar::{NodeId, RectStyle, Role};

use crate::{SpinnerProps, SpinnerTokens, spinner};

fn lay_out(node: NodeId) {
    telar::testing::lay_out(node, 100.0, 100.0);
}

#[test]
fn spinner_builds_with_default_size() {
    crate::test_support::fresh_layout_runtime();
    let widget = spinner(SpinnerProps::props().build());
    assert!(widget.is_ok(), "a spinner builds from its default size");
    lay_out(widget.unwrap().layout_node());
}

#[test]
fn spinner_builds_with_custom_size() {
    crate::test_support::fresh_layout_runtime();
    let widget = spinner(SpinnerProps::props().diameter(48.0).build()).unwrap();
    lay_out(widget.layout_node());
}

#[derive(Clone, Default)]
struct Plain {
    spinners: Option<SpinnerTokens>,
}

impl telar::ThemeTokens for Plain {
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(spinners) = &self.spinners {
            extensions.insert(spinners.clone());
        }
    }
}

fn ring(props: SpinnerProps) -> (telar::Rect, RectStyle) {
    crate::test_support::fresh_layout_runtime();
    telar::testing::scoped(|| {
        let widget = spinner(props).unwrap();
        lay_out(widget.layout_node());
        telar::ComponentList::new(widget)
    })
    .commands()
    .iter()
    .find_map(|command| match command {
        telar::DrawCommand::Rect { rect, style } => Some((*rect, **style)),
        _ => None,
    })
    .expect("the ring paints")
}

#[test]
fn an_unsized_spinner_stands_a_control_height_tall_and_follows_its_tokens() {
    telar::set_theme(Plain::default());
    let (rect, style) = ring(SpinnerProps::props().build());
    assert_eq!(
        rect.width,
        crate::metrics::control_height(telar::ControlSize::Regular)
    );
    assert_eq!(style.radius, telar::BorderRadius::all(rect.width / 2.0));

    telar::set_theme(Plain {
        spinners: Some(SpinnerTokens { size: Some(40.0) }),
    });
    let (rect, _) = ring(SpinnerProps::props().build());
    assert_eq!(rect.width, 40.0);
    let (rect, _) = ring(SpinnerProps::props().diameter(32.0).build());
    assert_eq!(rect.width, 32.0, "a diameter the caller names wins");
}

#[test]
fn a_spinner_stands_as_tall_as_a_control_of_its_size_and_scales_its_token() {
    use telar::ControlSize;
    for size in [ControlSize::Small, ControlSize::Regular, ControlSize::Large] {
        telar::set_theme(Plain::default());
        let (rect, _) = ring(SpinnerProps::props().size(size).build());
        assert_eq!(rect.width, crate::metrics::control_height(size), "{size:?}");

        telar::set_theme(Plain {
            spinners: Some(SpinnerTokens { size: Some(40.0) }),
        });
        let (rect, _) = ring(SpinnerProps::props().size(size).build());
        assert_eq!(rect.width, 40.0 * size.scale(), "{size:?}");
        let (rect, _) = ring(SpinnerProps::props().size(size).diameter(32.0).build());
        assert_eq!(
            rect.width, 32.0,
            "{size:?}: the diameter wins at every size"
        );
    }
}

/// A reader hears a spinner as a progress bar with no reading, which says the work is under way and not how far it has got, under the name its call gave it.
#[test]
fn a_reader_hears_a_named_progress_bar_with_no_reading() {
    use telar::Accessible;
    telar::set_theme(Plain::default());
    crate::test_support::fresh_layout_runtime();
    let tree = telar::testing::scoped(|| {
        let widget = spinner(SpinnerProps::props().build())
            .unwrap()
            .a11y_label(|| "Saving");
        lay_out(widget.layout_node());
        telar::ComponentList::new(widget)
    });
    let node = telar::inspect::snapshot(&tree.commands())
        .into_iter()
        .find(|node| node.role == Role::ProgressBar)
        .expect("the spinner is read");
    assert_eq!(node.name, "Saving");
    assert_eq!(node.value, None, "indeterminate");
    assert_eq!(node.id, None, "no keyboard stop");
}

#[test]
fn a_style_amendment_reshapes_the_ring() {
    telar::set_theme(Plain::default());
    let square: telar::SurfaceAmendment =
        std::rc::Rc::new(|s: RectStyle| s.with_radius(telar::BorderRadius::all(0.0)));
    let mut props = SpinnerProps::props().build();
    props.style = Some(square);
    let (_, style) = ring(props);
    assert_eq!(style.radius, telar::BorderRadius::all(0.0));
}
