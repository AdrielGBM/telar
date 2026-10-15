//! [`accordion`]: a header that expands and collapses the body beneath it.

use telar::{
    AlignItems, BorderRadius, Children, Clip, ClippedItem, Color, Component, Container, Effect,
    Event, EventResult, JustifyContent, LayoutError, LayoutItem, LayoutStyle, NodeId, Props,
    Reactive, RectStyle, RenderNode, RwSignal, ShapeStyle, StyledContainer, SurfaceAmendment, Text,
    amend_surface, box_item, children, effect, focus::Role, mark_dirty, set_display, signal,
};

#[cfg(test)]
use telar::Slots;

use crate::shared;

/// An accordion's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the header paints.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccordionTokens {
    /// Space between the header's edge and its title at the regular control size, scaled with the control size in force. `None` is [`metrics::control_pad_x`](crate::metrics::control_pad_x).
    pub pad_x: Option<f32>,
    /// Corner radius of the header's wash. `None` is the theme's `radius_control`.
    pub radius: Option<f32>,
}

impl AccordionTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn pad_x(&self) -> f32 {
        self.pad_x
            .map_or_else(shared::control_pad_x, |pad| pad * telar::control_scale())
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_control)
    }
}

/// A header is a row of a list, a control height with the step that parts one row's wash from the next, so a stack of accordions reads like one.
fn header_box() -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::SPACE_BETWEEN)
        .gap(shared::gap_md())
        .padding_horizontal(AccordionTokens::current().pad_x())
        .min_height(shared::row_height())
}

/// Which of its paints the header is showing.
#[derive(Clone, Copy)]
enum State {
    Rest,
    Hovered,
    Pressed,
}

fn header_surface(state: State) -> RectStyle {
    let rect =
        RectStyle::default().with_radius(BorderRadius::all(AccordionTokens::current().radius()));
    match state {
        State::Rest => rect,
        State::Hovered => rect.with_fill(shared::hover_wash()),
        State::Pressed => rect.with_fill(shared::pressed_wash()),
    }
}

/// A single collapsible section: a clickable header (title + caret) that toggles an `open` bool, and the slot children below it, present only while open. This is high-level sugar over the primitives; lives in `telar-components`, not the kernel, so an app can drop it or ship its own.
///
/// The body is built ONCE from the slot children (they arrive as pre-built widgets and cannot be rebuilt once consumed, like `modal`/`drawer`'s), then shown/hidden by toggling `display:none` on its layout node — the same mechanism the sandbox uses to switch doc sections — instead of a `ReactiveList` rebuild. Unlike `modal`/`drawer`'s `Overlay`, this stays IN FLOW: collapsing the body's node to a zero rect pushes following siblings back up, and expanding it pushes them down again.
#[derive(Props)]
pub struct AccordionProps {
    /// Header label.
    #[props(into, default)]
    pub title: Reactive<String>,
    /// Bound open/closed state. `None` (the default) is uncontrolled — the widget owns its own `signal(false)`.
    #[props(some, into, default)]
    pub open: Option<RwSignal<bool>>,
    /// Accent (the caret). `Color::TRANSPARENT` (the default) means "unset": falls back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Amends the paint of the header — the accordion's **principal surface** — at rest, hovered and pressed. See [`telar::SurfaceStyle`] for when a theme token or [`AccordionTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A header that expands and collapses the body nested inside it.
pub fn accordion(
    props: AccordionProps,
    children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut slots = children.build()?;
    let AccordionProps {
        title,
        open,
        color,
        style,
    } = props;
    let open = open.unwrap_or_else(|| signal(false));
    let body_children = slots.take_default();

    let caret_open = open;
    let caret_color = color.clone();
    let caret = shared::mark(
        move || match caret_open.get() {
            true => shared::Mark::ChevronDown,
            false => shared::Mark::ChevronForward,
        },
        move |_| shared::resolve(&caret_color, shared::accent).into(),
    )?;

    let title_widget = Text::declaring(
        move || title.get(),
        LayoutStyle::new(),
        |t| shared::control_text(t, 1.0),
    )?;

    let announced_open = open;
    let toggle_open = open;
    let paint = std::rc::Rc::new(move |state: State| amend_surface(header_surface(state), &style));
    let (rest, hover, press) = (paint.clone(), paint.clone(), paint);
    let header = StyledContainer::new(
        header_box(),
        move |_r| rest(State::Rest),
        vec![box_item(title_widget), caret],
    )?
    .styled_by(header_box)
    .hover_style(move |_r| hover(State::Hovered))
    .active_style(move |_r| press(State::Pressed))
    .control(Role::Disclosure)
    .toggled(move || announced_open.get())
    .on_press(move || toggle_open.update(|o| *o = !*o));

    // Built once from the slot children, and clipped to its own rect so a collapsed body draws nothing even if a child paints at fixed coordinates.
    let body = Container::column(body_children)?;
    let body_node = body.layout_node();
    let clipped_body = ClippedItem::new(box_item(body), Clip::both());

    let root = Container::column(children![header, clipped_body])?;

    let display_open = open;
    let _effect = effect(move || {
        set_display(body_node, display_open.get());
        let _ = mark_dirty(body_node);
    });

    Ok(box_item(Accordion { root, _effect }))
}

/// Wraps the header+body column together with the display-toggling effect so it stays alive for the widget's lifetime — an `Effect` deregisters itself on drop, same as `ReactiveList`'s own `_effect` field.
struct Accordion {
    root: Container,
    _effect: Effect,
}

impl LayoutItem for Accordion {
    fn layout_node(&self) -> NodeId {
        self.root.layout_node()
    }
}

impl Component for Accordion {
    fn view(&self) -> RenderNode {
        self.root.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.root.on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        "Accordion"
    }
}

#[cfg(test)]
#[path = "accordion_test.rs"]
mod tests;
