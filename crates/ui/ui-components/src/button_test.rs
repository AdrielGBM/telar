use std::cell::Cell;
use std::rc::Rc;

use layout_core::AvailableSpace;
use platform_core::{Event, PointerButton, PointerSource};
use renderer_core::DrawCommand;
use ui_core::{Component, ComponentList, compute_layout, new_container, track_layout};

use super::*;

/// A ghost button paints no surface, so its label is a word on the page like any other and takes the ink the page is written in. It is the variant a toolbar is made of, which is exactly where a region that declared its own colour would have had one word come out in the theme's instead.
#[test]
fn a_ghost_label_keeps_the_ink_of_the_region_around_it() {
    crate::test_support::fresh_layout_runtime();
    let btn = button(
        ButtonProps::props().label("Undo").ghost(true).build(),
        Children::default(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(200.0).height(80.0),
        &[btn.layout_node()],
    )
    .unwrap();
    let declared = Color::rgba(0.9, 0.2, 0.1, 1.0);
    ui_core::declare(
        root,
        renderer_core::Declared::default().with_color(declared),
    );
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(80.0),
    )
    .unwrap();

    let tree = ComponentList::new(btn);
    let ink = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Text { text, style, .. } if text.as_ref() == "Undo" => {
                Some(style.color.solid_color())
            }
            _ => None,
        })
        .expect("the button drew its label");
    assert_eq!(ink, declared);
}

#[test]
fn tap_fires_on_press() {
    let flag = Rc::new(Cell::new(false));
    let sink = flag.clone();
    crate::test_support::fresh_layout_runtime();
    let mut btn = button(
        ButtonProps::props()
            .label("OK")
            .fill(Reactive::of(|| Color::rgba(0.2, 0.4, 0.9, 1.0)))
            .on_press(Rc::new(move || sink.set(true)))
            .build(),
        Children::default(),
    )
    .unwrap();
    let node = btn.layout_node();
    let rect = track_layout(node).unwrap();
    compute_layout(
        node,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(80.0),
    )
    .unwrap();

    let r = rect.get();
    let (cx, cy) = ((r.x + r.width / 2.0) as f64, (r.y + r.height / 2.0) as f64);
    btn.on_event(&Event::PointerPressed {
        x: cx,
        y: cy,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
    assert!(!flag.get(), "press alone must not fire on_press");
    btn.on_event(&Event::PointerReleased {
        x: cx,
        y: cy,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
    assert!(flag.get(), "a tap fires on_press");
}

/// A theme switch has to re-*space*, not only re-colour.
///
/// Paint is a closure the renderer re-runs every frame, so a colour token switches for free. A metric is a number handed to the layout tree once, when the node is made — so until [`ui_core::style_follows`] the button kept the padding of the theme it was built under, and only a rebuild caught it up.
#[test]
fn switching_theme_re_spaces_the_button() {
    use theme_core::{ThemeTokens, set_theme};
    use ui_core::relayout_if_dirty;

    #[derive(Clone)]
    struct Spaced(f32);
    impl ThemeTokens for Spaced {
        fn spacing(&self) -> f32 {
            self.0
        }
    }

    crate::test_support::fresh_layout_runtime();
    set_theme(Spaced(8.0));
    // Measured with no label, so the width is the padding alone: what a font system makes of the text is a different question from whether the box followed the theme.
    let btn = button(ButtonProps::props().build(), Children::default()).unwrap();
    let node = btn.layout_node();
    let rect = track_layout(node).unwrap();
    compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
    assert_eq!(
        rect.get().width,
        2.0 * 8.0 * 1.75,
        "a button starts at the padding of the theme it was built under"
    );

    set_theme(Spaced(24.0));
    relayout_if_dirty();
    assert_eq!(
        rect.get().width,
        2.0 * 24.0 * 1.75,
        "and follows the theme it is switched to, without being rebuilt"
    );
}

/// The control size is one ambient number that every component interprets through its own proportions — the alternative being a `size` prop on each of them and a table of what each of their parts measures at each size. So the check is that the button did *not* need to know: nothing about it mentions a size, and it still gets smaller.
#[test]
fn the_ambient_control_size_scales_a_control_that_never_asked_for_one() {
    use theme_core::{ControlSize, ThemeTokens, set_control_size, set_theme};
    use ui_core::relayout_if_dirty;

    #[derive(Clone)]
    struct Plain;
    impl ThemeTokens for Plain {
        fn spacing(&self) -> f32 {
            8.0
        }
    }

    crate::test_support::fresh_layout_runtime();
    set_theme(Plain);
    set_control_size(ControlSize::Regular);
    let btn = button(ButtonProps::props().build(), Children::default()).unwrap();
    let node = btn.layout_node();
    let rect = track_layout(node).unwrap();
    compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
    let regular = rect.get().width;

    set_control_size(ControlSize::Mini);
    relayout_if_dirty();
    assert_eq!(
        rect.get().width,
        regular * ControlSize::Mini.scale(),
        "a denser control size carries through the button's own padding ratio"
    );
    set_control_size(ControlSize::Regular);
}
