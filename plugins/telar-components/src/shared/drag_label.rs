//! The name of what a [`DragSession`] carries, drawn beside the pointer over everything else on the surface.

use telar::{
    BorderRadius, Container, ControlSize, DragSession, LayoutError, LayoutItem, LayoutStyle,
    Overlay, ReactiveList, RectStyle, ShapeStyle, StyledContainer, Text, TextWrap, Transform,
    box_item, children, use_theme_tokens,
};

use crate::shared;

/// While `shown` reads true and the session carries something with a label, a chip with that label just past the pointer, in a click-through overlay so it never takes the pointer from what it is carried over.
///
/// Mounted only while shown, so a list of items that each hold one costs nothing while nothing is carried.
pub(crate) fn drag_label<P: Clone + 'static>(
    session: &DragSession<P>,
    shown: impl Fn() -> bool + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let session = session.clone();
    let list = ReactiveList::new(
        {
            let session = session.clone();
            move || vec![shown() && session.label().is_some()]
        },
        |showing: &bool| *showing,
        move |showing| {
            if !showing {
                return Ok(box_item(Container::new(
                    LayoutStyle::new().width(0.0).height(0.0),
                    vec![],
                )?));
            }
            chip(&session)
        },
        0.0,
    )?;
    Ok(box_item(list))
}

fn chip<P: Clone + 'static>(session: &DragSession<P>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let size = ControlSize::Regular;
    let text = {
        let session = session.clone();
        Text::declaring(
            move || {
                session
                    .label()
                    .map(|label| label.to_string())
                    .unwrap_or_default()
            },
            LayoutStyle::new(),
            move |inherited| {
                shared::control_text(size, inherited, 1.0)
                    .with_color(use_theme_tokens().inverse_ink())
                    .with_text_wrap(TextWrap::NoWrap)
            },
        )?
    };
    let chip_box = move || {
        LayoutStyle::row()
            .absolute()
            .inset_top(0.0)
            .inset_start(0.0)
            .padding_horizontal(shared::gap_md(size))
            .padding_vertical(shared::gap_xs(size))
    };
    let pointer = session.clone();
    let chip = StyledContainer::new(
        chip_box(),
        move |_| {
            RectStyle::default()
                .with_fill(use_theme_tokens().inverse_surface())
                .with_radius(BorderRadius::all(shared::radius_control()))
        },
        children![text],
    )?
    .styled_by(chip_box)
    .input_transparent()
    .with_transform(move |laid| {
        let (x, y) = pointer.pointer()?;
        let past = shared::gap_lg(size);
        Some(Transform::translate(x + past - laid.x, y + past - laid.y))
    });
    Ok(box_item(Overlay::click_through(
        LayoutStyle::new(),
        children![chip],
    )?))
}
