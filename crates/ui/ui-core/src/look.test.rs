use layout_core::{AvailableSpace, LayoutStyle, NodeId};
use platform_core::{Event, PointerButton, PointerSource};
use reactive_core::{RwSignal, signal};
use renderer_core::{Border, Color, Declared, DrawCommand, Paint, RectStyle, ShapeStyle};
use ui_tree::{Component, RenderNode};

use super::*;
use crate::context::{compute_layout, new_container, reset_layout_runtime};
use crate::inherit::{declare, inherited_text_style};
use crate::{BoxText, LayoutItem, StyledContainer};

/// A look worked out from the ink around it, as a tone of the place a component sits in is: filled with that ink, framed and pressed in steps of it.
#[derive(Clone)]
struct OfTheInk;

impl Look for OfTheInk {
    fn fill(&self, around: Color) -> Color {
        around.with_alpha(0.2)
    }

    fn frame(&self, around: Color) -> Color {
        around.with_alpha(0.5)
    }

    fn ink(&self, around: Color) -> Color {
        around.with_alpha(0.9)
    }

    fn hovered(&self, around: Color) -> Color {
        around.with_alpha(0.3)
    }

    fn pressed(&self, around: Color) -> Color {
        around.with_alpha(0.4)
    }
}

/// A look that only fills, leaving everything else to the defaults.
#[derive(Clone)]
struct FillOnly;

impl Look for FillOnly {
    fn fill(&self, _around: Color) -> Color {
        Color::rgb(0.2, 0.4, 0.6)
    }
}

const FRAME_WIDTH: f32 = 2.0;

/// A box painted from `look` in the ink of the text around it, as `look:` paints one that answers the pointer: its rest, hover and pressed paint and the ink it declares all read that ink.
fn looked(look: impl Look + Clone + 'static, text: RwSignal<BoxText>) -> StyledContainer {
    let rest = look.clone();
    let hover = look.clone();
    let press = look.clone();
    let paint = move |fill: Color, frame: Color| {
        RectStyle::default()
            .with_fill(fill)
            .with_border(Border::new(Paint::Solid(frame), [FRAME_WIDTH; 4]))
    };
    StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        move |_| {
            let around = text.get().ink;
            paint(rest.fill(around), rest.frame(around))
        },
        vec![],
    )
    .unwrap()
    .hover_style(move |_| {
        let around = text.get().ink;
        paint(hover.hovered(around), hover.frame(around))
    })
    .active_style(move |_| {
        let around = text.get().ink;
        paint(press.pressed(around), press.frame(around))
    })
    .following_text(text)
    .declaring(move || Declared::default().with_color(look.ink(text.get().ink)))
}

/// `look` painting a box placed in a place whose ink is `around`, and that place.
fn placed(look: impl Look + Clone + 'static, around: Color) -> (StyledContainer, NodeId) {
    reset_layout_runtime();
    let text = signal(BoxText::initial());
    let card = looked(look, text);
    let place = new_container(
        LayoutStyle::new().width(200.0).height(100.0),
        &[card.layout_node()],
    )
    .unwrap();
    declare(place, Declared::default().with_color(around));
    compute_layout(
        place,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    (card, place)
}

/// The fill and the frame the box draws now.
fn painted(card: &StyledContainer) -> (Color, Color) {
    fn walk(node: &RenderNode) -> Option<(Color, Color)> {
        match node {
            RenderNode::Primitive(DrawCommand::Rect { style, .. }) => {
                let fill = match style.fill {
                    Some(Paint::Solid(fill)) => fill,
                    _ => return None,
                };
                let frame = match style.border.as_ref().map(|border| &border.paint) {
                    Some(Paint::Solid(frame)) => *frame,
                    _ => Color::TRANSPARENT,
                };
                Some((fill, frame))
            }
            RenderNode::Group { children } | RenderNode::Element { children, .. } => {
                children.iter().find_map(walk)
            }
            _ => None,
        }
    }
    walk(&card.view()).expect("the box paints its rect")
}

fn hover(card: &mut StyledContainer) {
    card.on_event(&Event::PointerMoved {
        x: 100.0,
        y: 50.0,
        source: PointerSource::Mouse,
    });
}

fn press(card: &mut StyledContainer) {
    card.on_event(&Event::PointerPressed {
        x: 100.0,
        y: 50.0,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
}

#[test]
fn at_rest_a_box_takes_the_look_s_fill_and_frame_and_holds_its_ink() {
    let around = Color::rgb(0.8, 0.1, 0.1);
    let (card, _) = placed(OfTheInk, around);
    assert_eq!(
        painted(&card),
        (OfTheInk.fill(around), OfTheInk.frame(around))
    );
    assert_eq!(
        inherited_text_style(card.layout_node()).color.solid_color(),
        OfTheInk.ink(around),
        "what the box holds is drawn in the look's ink"
    );
}

#[test]
fn under_the_pointer_and_pressed_the_box_takes_the_look_s_states() {
    let around = Color::rgb(0.1, 0.6, 0.2);
    let (mut card, _) = placed(OfTheInk, around);
    hover(&mut card);
    assert_eq!(
        painted(&card),
        (OfTheInk.hovered(around), OfTheInk.frame(around))
    );
    press(&mut card);
    assert_eq!(
        painted(&card),
        (OfTheInk.pressed(around), OfTheInk.frame(around))
    );
}

#[test]
fn the_box_follows_the_ink_of_the_place_it_sits_in() {
    let (mut card, place) = placed(OfTheInk, Color::rgb(0.8, 0.1, 0.1));
    let moved = Color::rgb(0.1, 0.1, 0.8);
    declare(place, Declared::default().with_color(moved));
    assert_eq!(
        painted(&card),
        (OfTheInk.fill(moved), OfTheInk.frame(moved))
    );
    assert_eq!(
        inherited_text_style(card.layout_node()).color.solid_color(),
        OfTheInk.ink(moved)
    );
    hover(&mut card);
    assert_eq!(painted(&card).0, OfTheInk.hovered(moved));
}

#[test]
fn a_look_that_only_fills_keeps_its_fill_under_the_pointer_frames_nothing_and_keeps_the_ink() {
    let around = Color::rgb(0.3, 0.3, 0.3);
    let (mut card, _) = placed(FillOnly, around);
    let rest = (FillOnly.fill(around), Color::TRANSPARENT);
    assert_eq!(painted(&card), rest);
    assert_eq!(
        inherited_text_style(card.layout_node()).color.solid_color(),
        around
    );
    hover(&mut card);
    assert_eq!(painted(&card), rest, "no change under the pointer");
    press(&mut card);
    assert_eq!(painted(&card), rest, "no change pressed");
}
