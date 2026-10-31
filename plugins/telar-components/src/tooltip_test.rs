use super::*;
use crate::test_support::find_text;
use telar::testing::moved;
use telar::{
    AvailableSpace, ComponentList, DrawCommand, LineHeight, compute_layout, new_container,
    relayout_if_dirty,
};

fn trigger() -> Children {
    Children::one(|| {
        let inner = Container::new(LayoutStyle::new().width(80.0).height(30.0), vec![])?;
        Ok(box_item(inner))
    })
}

/// A hint is sized from the text it is describing, not from the theme: a compact panel that says its region is 11px gets a bubble at that panel's scale, and `TooltipProps` has no size to correct it with.
#[test]
fn a_bubble_takes_the_size_the_region_around_it_declared() {
    crate::test_support::fresh_layout_runtime();
    let tooltip = tooltip(
        TooltipProps::props()
            .text("Move")
            .children(trigger())
            .build(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[tooltip.layout_node()],
    )
    .unwrap();
    telar::declare(root, telar::Declared::default().with_font_size(11.0));
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();

    let mut tree = ComponentList::new(tooltip);
    let _ = tree.commands();
    tree.on_event(&moved(40.0, 15.0));
    relayout_if_dirty();

    let size = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Text { text, style, .. } if text.as_ref() == "Move" => {
                Some(style.font_size)
            }
            _ => None,
        })
        .expect("the bubble drew its name");
    assert_eq!(size, 11.0 * BUBBLE_RATIO);
}

#[test]
fn hover_shows_bubble_and_leave_hides_it() {
    crate::test_support::fresh_layout_runtime();
    let tooltip = tooltip(
        TooltipProps::props()
            .text("Helpful hint")
            .children(trigger())
            .build(),
    )
    .unwrap();

    // A parent-less root computed against the window registers the overlay host the bubble anchors into.
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[tooltip.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(tooltip);
    let _ = tree.commands();

    assert!(
        !find_text(&tree.commands(), "Helpful hint"),
        "leaving hides the bubble again"
    );

    tree.on_event(&moved(40.0, 15.0));
    relayout_if_dirty();
    assert!(
        find_text(&tree.commands(), "Helpful hint"),
        "bubble shows on hover"
    );

    tree.on_event(&moved(9999.0, 9999.0));
    relayout_if_dirty();
    assert!(
        !find_text(&tree.commands(), "Helpful hint"),
        "bubble hidden on leave"
    );
}

/// The shape every hint in an application takes: the name, its binding pushed to the far edge, and a sentence under both. All three have to reach the bubble, because the reason the shortcut is a prop of its own is that folding it into the name loses exactly this arrangement.
#[test]
fn a_hint_shows_its_name_its_shortcut_and_its_description() {
    crate::test_support::fresh_layout_runtime();
    let tooltip = tooltip(
        TooltipProps::props()
            .text("Move")
            .shortcut("G")
            .description("Drag the selection along the ground")
            .children(trigger())
            .build(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[tooltip.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(tooltip);
    tree.on_event(&moved(40.0, 15.0));
    relayout_if_dirty();

    let cmds = tree.commands();
    assert!(find_text(&cmds, "Move"), "the name");
    assert!(find_text(&cmds, "G"), "its binding");
    assert!(
        find_text(&cmds, "Drag the selection along the ground"),
        "and what it does"
    );
}

/// The description line is set with room to breathe, because it is the only part of a bubble that wraps.
///
/// At the default 1.2 the two lines of a long hint sit almost on top of each other, and it reads as the *text* being squashed rather than as the leading being short — which is why it only showed up on the hints long enough to need a second line, and looked like a placement bug rather than a type one. The looser leading is set on this line and on no other: the name and the shortcut never wrap.
#[test]
fn the_description_line_is_set_with_room_to_wrap_into() {
    crate::test_support::fresh_layout_runtime();
    let tooltip = tooltip(
        TooltipProps::props()
            .text("Setup")
            .description("Name regions and say what it is made of")
            .children(trigger())
            .build(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[tooltip.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(tooltip);
    tree.on_event(&moved(40.0, 15.0));
    relayout_if_dirty();

    // Asserted on the style, not the drawn height: whether this sentence needs a second line depends on the system font, so measuring it would test the CI runner's font.
    let leading_of = |needle: &str| {
        tree.commands()
            .iter()
            .find_map(|c| match c {
                DrawCommand::Text { text, style, .. } if text.starts_with(needle) => {
                    Some(style.line_height)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("`{needle}` is drawn"))
    };
    assert_eq!(
        leading_of("Name regions"),
        LineHeight::Times(DESCRIPTION_LEADING)
    );
    assert_eq!(
        leading_of("Setup"),
        LineHeight::Natural,
        "the name never wraps"
    );
}

/// A bubble is as wide as what it says, up to its cap. It was taking the cap whatever it had to say, because the layer its chip sits in was a `LayoutStyle::new()` — a CSS **block**, where a child fills its containing block and `align_items` means nothing. A two-word hint in a 240px box does not read as a hint, and nothing about it looks like a layout mode being wrong.
#[test]
fn a_bubble_is_as_wide_as_what_it_says() {
    crate::test_support::fresh_layout_runtime();
    let tooltip = tooltip(
        TooltipProps::props()
            .text("Object")
            .shortcut("1")
            .description("Pick whole bodies")
            .children(trigger())
            .build(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[tooltip.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(tooltip);
    tree.on_event(&moved(40.0, 15.0));
    relayout_if_dirty();

    let widest = tree
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text { rect, .. } => Some(rect.width),
            _ => None,
        })
        .fold(0.0f32, f32::max);
    assert!(
        widest > 0.0 && widest < TooltipTokens::current().max_width() * 0.75,
        "the bubble hugged its text instead of taking its cap, got {widest}"
    );
}

/// A hint with nothing but a name is the common case, and it must not pay for the parts it left out with a taller bubble than it has words for.
#[test]
fn an_absent_shortcut_and_description_take_no_room() {
    crate::test_support::fresh_layout_runtime();
    let height_of = |props: TooltipProps| {
        let mut tree = telar::testing::scoped(|| {
            let tooltip = tooltip(TooltipProps {
                children: trigger(),
                ..props
            })
            .unwrap();
            let root = new_container(
                LayoutStyle::new().flex_column().width(400.0).height(400.0),
                &[tooltip.layout_node()],
            )
            .unwrap();
            compute_layout(
                root,
                AvailableSpace::Definite(400.0),
                AvailableSpace::Definite(400.0),
            )
            .unwrap();
            ComponentList::new(tooltip)
        });
        tree.on_event(&moved(40.0, 15.0));
        relayout_if_dirty();
        tree.commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Text { rect, .. } => Some(rect.height),
                _ => None,
            })
            .sum::<f32>()
    };
    let bare = height_of(TooltipProps::props().text("Move").build());
    crate::test_support::fresh_layout_runtime();
    let full = height_of(
        TooltipProps::props()
            .text("Move")
            .description("Drag the selection")
            .build(),
    );
    assert!(bare > 0.0, "the name is drawn either way");
    assert!(
        full > bare,
        "a described hint is taller than a bare one: {bare} vs {full}"
    );
}

#[test]
fn builds_without_hover() {
    crate::test_support::fresh_layout_runtime();
    let tooltip = tooltip(TooltipProps::props().children(trigger()).build()).unwrap();
    let tree = ComponentList::new(tooltip);
    assert!(
        !find_text(&tree.commands(), "Helpful hint"),
        "a tooltip draws nothing until it is hovered"
    );
}

#[derive(Clone)]
struct Themed {
    tooltips: Option<TooltipTokens>,
}

impl telar::ThemeTokens for Themed {
    fn ink(&self) -> Color {
        Color::rgba(0.1, 0.12, 0.2, 1.0)
    }
    fn surface(&self) -> Color {
        Color::rgba(0.97, 0.96, 0.94, 1.0)
    }
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(tooltips) = self.tooltips {
            extensions.insert(tooltips);
        }
    }
}

/// A hovered tooltip saying "Move", with `color` and `style` given.
fn hovered(
    color: Color,
    style: Option<telar::SurfaceAmendment>,
) -> telar::testing::Scoped<ComponentList> {
    crate::test_support::fresh_layout_runtime();
    let props = TooltipProps::props().text("Move").color(color);
    let props = match style {
        Some(style) => props.style(style),
        None => props,
    };
    let mut tree = telar::testing::scoped(|| {
        let tooltip = tooltip(props.children(trigger()).build()).unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[tooltip.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        ComponentList::new(tooltip)
    });
    let _ = tree.commands();
    tree.on_event(&moved(40.0, 15.0));
    relayout_if_dirty();
    tree
}

fn bubble(tree: &ComponentList) -> (RectStyle, Color) {
    let commands = tree.commands();
    let chip = commands
        .iter()
        .find_map(|c| match c {
            DrawCommand::Rect { style, .. } if style.fill.is_some() => Some(**style),
            _ => None,
        })
        .expect("the bubble is painted");
    let ink = commands
        .iter()
        .find_map(|c| match c {
            DrawCommand::Text { text, style, .. } if text.as_ref() == "Move" => {
                Some(style.color.solid_color())
            }
            _ => None,
        })
        .expect("the bubble says its name");
    (chip, ink)
}

/// The bubble was a fixed near-black chip under a fixed near-white ink, in a light application and a dark one alike. It is the theme's inverse surface under its inverse ink now, rounded like every other floating surface.
#[test]
fn a_bubble_is_the_theme_s_inverse_surface_under_its_inverse_ink() {
    use telar::{BorderRadius, Paint, ThemeTokens};

    let theme = Themed { tooltips: None };
    telar::set_theme(theme.clone());
    let tree = hovered(Color::TRANSPARENT, None);
    let (chip, ink) = bubble(&tree);
    assert_eq!(chip.fill, Some(Paint::Solid(theme.inverse_surface())));
    assert_eq!(chip.radius, BorderRadius::all(theme.radius_surface()));
    assert_eq!(ink, theme.inverse_ink());
}

/// A bubble given a colour of its own is inked in whichever ink reads on that colour.
#[test]
fn a_bubble_in_a_colour_of_its_own_is_inked_to_read_on_it() {
    telar::set_theme(Themed { tooltips: None });
    let pale = Color::rgba(0.95, 0.9, 0.6, 1.0);
    let tree = hovered(pale, None);
    let (_, ink) = bubble(&tree);
    assert_eq!(ink, crate::shared::ink_on(pale));
}

#[test]
fn the_bubble_s_corner_follows_the_tooltip_tokens_and_its_caller_s_amendment() {
    use telar::{BorderRadius, Paint};

    telar::set_theme(Themed {
        tooltips: Some(TooltipTokens {
            radius: Some(1.0),
            ..TooltipTokens::default()
        }),
    });
    let tree = hovered(Color::TRANSPARENT, None);
    assert_eq!(bubble(&tree).0.radius, BorderRadius::all(1.0));
    drop(tree);

    let green = Color::rgba(0.1, 0.6, 0.2, 1.0);
    let tree = hovered(
        Color::TRANSPARENT,
        Some(std::rc::Rc::new(move |style: RectStyle| {
            style.with_fill(green)
        })),
    );
    assert_eq!(bubble(&tree).0.fill, Some(Paint::Solid(green)));
}

#[derive(Clone)]
struct Scheme {
    dark: bool,
}

impl telar::ThemeTokens for Scheme {
    fn ink(&self) -> Color {
        if self.dark {
            Color::rgba(0.93, 0.93, 0.95, 1.0)
        } else {
            Color::rgba(0.09, 0.1, 0.18, 1.0)
        }
    }
    fn surface(&self) -> Color {
        if self.dark {
            Color::rgba(0.16, 0.16, 0.19, 1.0)
        } else {
            Color::rgba(1.0, 1.0, 1.0, 1.0)
        }
    }
}

/// The shortcut and the sentence rank below the name by fading, and fading may not take them under 4.5:1 on the bubble: on a dark theme's light bubble a fixed strength did.
#[test]
fn every_line_of_a_bubble_reads_on_it_in_light_and_in_dark() {
    use telar::ThemeTokens;

    for dark in [false, true] {
        let theme = Scheme { dark };
        telar::set_theme(theme.clone());
        crate::test_support::fresh_layout_runtime();
        let tooltip = tooltip(
            TooltipProps::props()
                .text("Find")
                .shortcut("Ctrl+K")
                .description("Search every command")
                .children(trigger())
                .build(),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[tooltip.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(tooltip);
        let _ = tree.commands();
        tree.on_event(&moved(40.0, 15.0));
        relayout_if_dirty();
        let fill = theme.inverse_surface();
        for line in ["Find", "Ctrl+K", "Search every command"] {
            let ink = tree
                .commands()
                .iter()
                .find_map(|c| match c {
                    DrawCommand::Text { text, style, .. } if text.as_ref() == line => {
                        Some(style.color.solid_color())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{line:?} is drawn"));
            assert!(
                fill.contrast_ratio(ink) >= 4.5,
                "{line:?} on the {} bubble: {}",
                if dark {
                    "dark theme's"
                } else {
                    "light theme's"
                },
                fill.contrast_ratio(ink)
            );
        }
    }
}
