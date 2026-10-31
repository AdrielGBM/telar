use std::cell::Cell;
use std::rc::Rc;

use telar::{
    AvailableSpace, Component, ComponentList, DrawCommand, Event, PointerButton, PointerSource,
    compute_layout, new_container, track_layout,
};

use super::*;

/// A ghost button paints no surface, so its label is a word on the page like any other and takes the ink the page is written in. It is the variant a toolbar is made of, which is exactly where a region that declared its own colour would have had one word come out in the theme's instead.
#[test]
fn a_ghost_label_keeps_the_ink_of_the_region_around_it() {
    crate::test_support::fresh_layout_runtime();
    let btn = button(
        ButtonProps::props()
            .label("Undo")
            .variant(ButtonVariant::Ghost)
            .build(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(200.0).height(80.0),
        &[btn.layout_node()],
    )
    .unwrap();
    let declared = Color::rgba(0.9, 0.2, 0.1, 1.0);
    telar::declare(root, telar::Declared::default().with_color(declared));
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
            .color(Reactive::of(|| Color::rgba(0.2, 0.4, 0.9, 1.0)))
            .on_press(Rc::new(move || sink.set(true)))
            .build(),
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
/// Paint is a closure the renderer re-runs every frame, so a colour token switches for free. A metric is a number handed to the layout tree once, when the node is made — so until [`telar::style_follows`] the button kept the padding of the theme it was built under, and only a rebuild caught it up.
#[test]
fn switching_theme_re_spaces_the_button() {
    use telar::relayout_if_dirty;
    use telar::{ThemeTokens, set_theme};

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
    let btn = button(ButtonProps::props().build()).unwrap();
    let node = btn.layout_node();
    let rect = track_layout(node).unwrap();
    compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
    assert_eq!(
        rect.get().width,
        2.0 * 8.0 * 1.5,
        "a button starts at the padding of the theme it was built under"
    );

    set_theme(Spaced(24.0));
    relayout_if_dirty();
    assert_eq!(
        rect.get().width,
        2.0 * 24.0 * 1.5,
        "and follows the theme it is switched to, without being rebuilt"
    );
}

/// The control size is one ambient number that every component interprets through its own proportions — the alternative being a `size` prop on each of them and a table of what each of their parts measures at each size. So the check is that the button did *not* need to know: nothing about it mentions a size, and it still gets smaller.
#[test]
fn the_ambient_control_size_scales_a_control_that_never_asked_for_one() {
    use telar::relayout_if_dirty;
    use telar::{ControlSize, ThemeTokens, set_control_size, set_theme};

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
    let btn = button(ButtonProps::props().build()).unwrap();
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

#[test]
fn a_button_under_a_provider_paints_with_that_theme() {
    use telar::Color;
    use telar::{ThemeTokens, set_theme};

    #[derive(Clone)]
    struct Palette {
        primary: Color,
        on_primary: Color,
        spacing: f32,
    }
    impl ThemeTokens for Palette {
        fn primary(&self) -> Color {
            self.primary
        }
        fn on_primary(&self) -> Color {
            self.on_primary
        }
        fn ink(&self) -> Color {
            Color::BLACK
        }
        fn spacing(&self) -> f32 {
            self.spacing
        }
    }
    let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
    let blue = Color::rgba(0.0, 0.0, 1.0, 1.0);
    let white = Color::rgba(1.0, 1.0, 1.0, 1.0);
    let yellow = Color::rgba(1.0, 1.0, 0.0, 1.0);

    crate::test_support::fresh_layout_runtime();
    set_theme(Palette {
        primary: blue,
        on_primary: yellow,
        spacing: 8.0,
    });
    let themed = telar::provide_theme(
        Palette {
            primary: red,
            on_primary: white,
            spacing: 16.0,
        },
        || button(ButtonProps::props().label("A").build()),
    )
    .unwrap();
    let themed_rect = track_layout(themed.layout_node()).unwrap();
    let plain = button(ButtonProps::props().label("A").build()).unwrap();
    let plain_rect = track_layout(plain.layout_node()).unwrap();
    let row = telar::Container::new(
        LayoutStyle::new().flex_row().align_items(AlignItems::START),
        vec![Box::new(themed), plain],
    )
    .unwrap();
    compute_layout(
        row.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();

    let tree = ComponentList::new(row);
    let commands = tree.commands();
    let fills: Vec<Color> = commands
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Rect { style, .. } => style.fill.as_ref().map(|p| p.solid_color()),
            _ => None,
        })
        .collect();
    let inks: Vec<Color> = commands
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text { style, .. } => Some(style.color.solid_color()),
            _ => None,
        })
        .collect();
    assert_eq!(fills, vec![red, blue]);
    assert_eq!(inks, vec![white, yellow]);
    assert!(
        themed_rect.get().width > plain_rect.get().width,
        "the provider's spacing pads the button it covers"
    );
}

#[test]
fn a_ghost_button_is_washed_while_the_pointer_rests_on_it() {
    use telar::testing::{centre, moved, route};

    crate::test_support::fresh_layout_runtime();
    let btn = button(
        ButtonProps::props()
            .label("Undo")
            .variant(ButtonVariant::Ghost)
            .build(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_row().width(200.0).height(80.0),
        &[btn.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(80.0),
    )
    .unwrap();
    telar::relayout_if_dirty();
    let mut tree = ComponentList::new(btn);
    let filled = |tree: &ComponentList| {
        tree.commands()
            .iter()
            .filter(|c| matches!(c, DrawCommand::Rect { style, .. } if style.fill.is_some()))
            .count()
    };
    assert_eq!(filled(&tree), 0);
    let (x, y) = centre(crate::test_support::drawn(telar::focus::Role::Button)[0]);
    route(&mut tree, &moved(x, y));
    assert_eq!(filled(&tree), 1);
}

#[derive(Clone)]
struct Rounded {
    control: f32,
    buttons: Option<ButtonTokens>,
}

impl telar::ThemeTokens for Rounded {
    fn radius_control(&self) -> f32 {
        self.control
    }
    fn border_width_strong(&self) -> f32 {
        3.0
    }
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(buttons) = &self.buttons {
            extensions.insert(buttons.clone());
        }
    }
}

fn laid_out(props: ButtonProps) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    let btn = button(props).unwrap();
    compute_layout(
        btn.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();
    ComponentList::new(btn)
}

fn box_style(tree: &ComponentList) -> RectStyle {
    tree.commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Rect { style, .. } => Some(**style),
            _ => None,
        })
        .expect("the button paints its box")
}

#[test]
fn the_corner_is_the_theme_s_control_role_until_the_button_tokens_name_one() {
    telar::set_theme(Rounded {
        control: 7.0,
        buttons: None,
    });
    let tree = laid_out(ButtonProps::props().label("Save").build());
    assert_eq!(box_style(&tree).radius, BorderRadius::all(7.0));

    telar::set_theme(Rounded {
        control: 7.0,
        buttons: Some(ButtonTokens {
            radius: Some(2.0),
            ..ButtonTokens::default()
        }),
    });
    assert_eq!(
        box_style(&tree).radius,
        BorderRadius::all(2.0),
        "the theme's own button radius wins, and the paint follows the switch"
    );

    telar::set_theme(Rounded {
        control: 12.0,
        buttons: Some(ButtonTokens::default()),
    });
    assert_eq!(
        box_style(&tree).radius,
        BorderRadius::all(12.0),
        "tokens left unset keep following the role"
    );
}

#[test]
fn an_outline_is_framed_at_the_theme_s_strong_border_width() {
    telar::set_theme(Rounded {
        control: 4.0,
        buttons: None,
    });
    let red = Color::rgba(0.8, 0.1, 0.1, 1.0);
    let tree = laid_out(
        ButtonProps::props()
            .label("Cancel")
            .variant(ButtonVariant::Outline)
            .color(red)
            .build(),
    );
    let style = box_style(&tree);
    assert_eq!(style.border, Some(Border::uniform(red, 3.0)));
    assert_eq!(style.fill, None, "an outline at rest is not filled");
}

#[test]
fn a_soft_button_is_a_wash_of_its_colour_at_the_theme_s_strength() {
    telar::set_theme(Rounded {
        control: 4.0,
        buttons: None,
    });
    let red = Color::rgba(0.8, 0.1, 0.1, 1.0);
    let tree = laid_out(
        ButtonProps::props()
            .label("Archive")
            .variant(ButtonVariant::Soft)
            .color(red)
            .build(),
    );
    let wash = telar::use_theme_tokens().highlight_med();
    assert_eq!(
        box_style(&tree).fill.map(|p| p.solid_color()),
        Some(red.with_alpha(wash.a))
    );
}

/// `@class` and the `style` prop amend the finished paint, so they reach the hovered box as well as the resting one.
#[test]
fn a_style_amendment_reshapes_every_state() {
    use telar::testing::{centre, moved, route};

    telar::set_theme(Rounded {
        control: 9.0,
        buttons: None,
    });
    let square: SurfaceAmendment = Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(0.0)));
    let mut tree = laid_out(
        ButtonProps::props()
            .label("Save")
            .variant(ButtonVariant::Ghost)
            .style(square)
            .build(),
    );
    assert_eq!(box_style(&tree).radius, BorderRadius::all(0.0));
    let (x, y) = centre(crate::test_support::drawn(telar::focus::Role::Button)[0]);
    route(&mut tree, &moved(x, y));
    let hovered = box_style(&tree);
    assert!(hovered.fill.is_some(), "the hover wash is drawn");
    assert_eq!(
        hovered.radius,
        BorderRadius::all(0.0),
        "on the amended shape"
    );
}

fn label_height_in(width: f32) -> f32 {
    crate::test_support::fresh_layout_runtime();
    let btn = button(ButtonProps::props().label("Get started now").build()).unwrap();
    compute_layout(
        btn.layout_node(),
        AvailableSpace::Definite(width),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    ComponentList::new(btn)
        .commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Text { rect, .. } => Some(rect.height),
            _ => None,
        })
        .expect("the button draws its label")
}

/// A button names one action, so a narrow space makes it overflow or its neighbours move, never break its label over two lines that the box has no vertical padding to hold.
#[test]
fn a_label_wider_than_its_space_stays_on_one_line() {
    assert_eq!(label_height_in(40.0), label_height_in(400.0));
}

/// A button sized by its label stands it in the middle of its face: the padding before the first glyph's advance is the padding after the last, to within the half pixel layout rounds its edges to.
#[test]
fn a_hugging_button_pads_its_label_evenly_on_both_sides() {
    crate::test_support::fresh_layout_runtime();
    let btn = button(
        ButtonProps::props()
            .label("Settings")
            .variant(ButtonVariant::Ghost)
            .build(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_row().width(400.0).height(80.0),
        &[btn.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(80.0),
    )
    .unwrap();
    let tree = ComponentList::new(btn);
    let face = crate::test_support::drawn(telar::focus::Role::Button)[0];
    let (before, after) = crate::test_support::run_insets(&tree, "Settings", face);
    let pad = crate::shared::control_pad_x();
    assert!((before - pad).abs() <= 0.5, "{before} {after}");
    assert!((after - pad).abs() <= 0.5, "{before} {after}");
}
