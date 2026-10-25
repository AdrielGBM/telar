use telar::NodeId;

use super::*;

fn lay_out(node: NodeId) {
    telar::testing::lay_out(node, 100.0, 100.0);
}

#[test]
fn spinner_builds_with_default_size() {
    crate::test_support::fresh_layout_runtime();
    let widget = spinner(SpinnerProps::props().build(), Children::default());
    assert!(widget.is_ok(), "a spinner builds from its default size");
    lay_out(widget.unwrap().layout_node());
}

#[test]
fn spinner_builds_with_custom_size() {
    crate::test_support::fresh_layout_runtime();
    let widget = spinner(
        SpinnerProps::props().size(48.0).build(),
        Children::default(),
    )
    .unwrap();
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
        let widget = spinner(props, Children::default()).unwrap();
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
    assert_eq!(rect.width, crate::metrics::control_height());
    assert_eq!(style.radius, telar::BorderRadius::all(rect.width / 2.0));

    telar::set_theme(Plain {
        spinners: Some(SpinnerTokens { size: Some(40.0) }),
    });
    let (rect, _) = ring(SpinnerProps::props().build());
    assert_eq!(rect.width, 40.0);
    let (rect, _) = ring(SpinnerProps::props().size(32.0).build());
    assert_eq!(rect.width, 32.0, "a size the caller names wins");
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
