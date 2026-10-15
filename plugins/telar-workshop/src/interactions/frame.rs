//! A frame a step left, drawn again: over the canvas in place of the live preview while the step is picked, and small beside a step that failed.

use std::rc::Rc;

use telar::preview::Frame;
use telar::{
    Accessible, AlignItems, Canvas, Children, Container, DrawCommand, LayoutError, LayoutItem,
    LayoutStyle, Reactive, ReactiveList, Rect, RectStyle, RenderNode, Role, ShapeStyle, Size,
    SizeDimension, StyledContainer, Text, Transform, box_item, children, composite_surface,
    use_theme_tokens,
};
use telar_components::{ButtonProps, button};
use telar_devtools::{WORKBENCH_GRID, use_workbench_tokens, workbench_card, workbench_mono};

use super::{Interactions, Shown};
use crate::state::WorkshopState;
use crate::strings::{self, BACK_TO_LIVE, SHOWING_STEP, STEP_FRAME};

const THUMBNAIL_HEIGHT: f32 = WORKBENCH_GRID * 12.0;
const THUMBNAIL_MAX_WIDTH: f32 = WORKBENCH_GRID * 30.0;

/// What covers the live canvas while a step is picked: the frame that step left, which takes no input, under a bar naming the step with the way back to live. Nothing otherwise.
pub(crate) fn frame_layer(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let watching = state.interactions().clone();
    let building = state.interactions().clone();
    let layer = ReactiveList::with_style(
        LayoutStyle::new().absolute_fill(),
        move || watching.shown_key().into_iter().collect(),
        |key: &(u64, usize)| *key,
        move |_| match building.shown() {
            Some(shown) => cover(&building, shown),
            None => Ok(box_item(Container::new(LayoutStyle::new(), Vec::new())?)),
        },
    )?;
    let layer = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        |_| RectStyle::default(),
        children![layer],
    )?
    .input_transparent();
    Ok(box_item(layer))
}

fn cover(interactions: &Interactions, shown: Shown) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let picture = picture(
        shown.frame.clone(),
        shown.index,
        LayoutStyle::new().absolute_fill(),
    )?;
    let cover = StyledContainer::new(
        LayoutStyle::new()
            .absolute_fill()
            .flex_column()
            .align_items(AlignItems::START)
            .padding_all(WORKBENCH_GRID),
        |_| RectStyle::default().with_fill(use_workbench_tokens().canvas_backdrop),
        vec![picture, bar(interactions, &shown)?],
    )?
    .input_opaque();
    Ok(box_item(cover))
}

/// Names the step whose frame is shown, red when it failed, beside the way back to live.
fn bar(interactions: &Interactions, shown: &Shown) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let failed = shown.step.failed();
    let naming = [
        ("step", (shown.index + 1).to_string()),
        ("total", shown.total.to_string()),
        ("action", shown.step.action.clone()),
    ];
    let label = Text::declaring(
        move || {
            let values: Vec<(&str, &str)> = naming
                .iter()
                .map(|(name, value)| (*name, value.as_str()))
                .collect();
            strings::text_with_values(SHOWING_STEP, &values)
        },
        LayoutStyle::new().min_width(0.0),
        move |text| {
            let text = workbench_mono(text);
            match failed {
                true => text.with_color(use_theme_tokens().error()),
                false => text,
            }
        },
    )?;
    let interactions = interactions.clone();
    let live = button(
        ButtonProps::props()
            .label(Reactive::of(|| strings::text(BACK_TO_LIVE)))
            .on_press(Rc::new(move || interactions.back_to_live()))
            .build(),
        Children::default(),
    )?;
    let bar = StyledContainer::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID)
            .padding_horizontal(WORKBENCH_GRID)
            .padding_vertical(WORKBENCH_GRID / 2.0)
            .max_width(SizeDimension::Percent(1.0)),
        |_| workbench_card().with_fill(use_workbench_tokens().panel_background),
        vec![box_item(label), live],
    )?
    .role(Role::Status);
    Ok(box_item(bar))
}

/// The frame step `index` left, small: as tall as a few rows, and no wider than its height lets it be at its own proportions. Pressing it does what `pick` does; the step's own line is the way there by keyboard.
pub(super) fn thumbnail(
    frame: Frame,
    index: usize,
    pick: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Size { width, height } = frame.size;
    let wide = match height > 0.0 {
        true => (THUMBNAIL_HEIGHT * width / height).min(THUMBNAIL_MAX_WIDTH),
        false => THUMBNAIL_HEIGHT,
    };
    let picture = picture(frame, index, LayoutStyle::new().absolute_fill())?;
    let framed = StyledContainer::new(
        LayoutStyle::new()
            .width(wide)
            .height(THUMBNAIL_HEIGHT)
            .flex_shrink(0.0),
        |_| {
            let tokens = use_workbench_tokens();
            workbench_card().with_fill(tokens.canvas_backdrop)
        },
        vec![picture],
    )?
    .on_press(pick);
    Ok(box_item(framed))
}

/// `frame` drawn whole in the box `style` lays out, announced as the picture it is rather than as the controls it shows.
fn picture(
    frame: Frame,
    index: usize,
    style: LayoutStyle,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let canvas = Canvas::new(LayoutStyle::new().absolute_fill(), move |rect| {
        drawn(&frame, rect)
    })?;
    let number = (index + 1).to_string();
    let picture = Container::new(style, children![canvas])?
        .role(Role::Drawing)
        .a11y_label(move || strings::text_with(STEP_FRAME, &number));
    Ok(box_item(picture))
}

/// `frame` scaled to fit `rect` whole, centred in it, and clipped to its own size.
///
/// The boxes and surfaces its commands name are left out: they belong to the canvas that drew them, and drawn again here they would tell a reader of controls that are only pictured.
pub(super) fn drawn(frame: &Frame, rect: Rect) -> RenderNode {
    let Size { width, height } = frame.size;
    if width <= 0.0 || height <= 0.0 {
        return RenderNode::Empty;
    }
    let scale = (rect.width / width).min(rect.height / height);
    let x = rect.x + (rect.width - width * scale) / 2.0;
    let y = rect.y + (rect.height - height * scale) / 2.0;
    let painted = frame
        .commands
        .iter()
        .filter(|command| !is_marker(command))
        .cloned()
        .map(RenderNode::Primitive);
    composite_surface(
        Transform::scale_around(scale, scale, 0.0, 0.0).then(Transform::translate(x, y)),
        frame.size,
        painted,
    )
}

fn is_marker(command: &DrawCommand) -> bool {
    matches!(
        command,
        DrawCommand::PushElement { .. }
            | DrawCommand::PopElement
            | DrawCommand::PushSurface { .. }
            | DrawCommand::PopSurface
    )
}
