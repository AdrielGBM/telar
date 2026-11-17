//! [`radio_group`]: a set of radio buttons choosing one of its options, or none yet.

use std::rc::Rc;

use telar::{
    Accessible, AlignItems, Border, Color, Container, Handler, JustifyContent, LayoutError,
    LayoutItem, LayoutStyle, Props, Reactive, RectStyle, RovingAxis, RovingGroup, RwSignal,
    ShapeStyle, StyledContainer, SurfaceAmendment, amend_surface, box_item, children, focus::Role,
    signal,
};

use crate::shared;
use telar::ControlSize;

/// A radio button's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the radio paints. The ring and its dot are pills at whatever size they are laid out.
#[derive(Clone, Debug, Default, PartialEq, telar::ComponentTokens)]
pub struct RadioTokens {
    /// Diameter of the ring at the regular size, scaled with the radio's own. `None` is [`metrics::icon_size`](crate::metrics::icon_size).
    #[token(or = shared::icon_size, scaled)]
    pub size: Option<f32>,
    /// Width of the ring. `None` is the theme's `border_width_strong`.
    #[token(or = shared::border_width_strong)]
    pub border_width: Option<f32>,
}

fn ring_style(size: ControlSize) -> LayoutStyle {
    let side = RadioTokens::current().size(size);
    LayoutStyle::row()
        .width(side)
        .height(side)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
}

/// The dot stands inside the ring by the step between the parts of one control, as a checkbox's mark does inside its box.
fn dot_style(size: ControlSize) -> LayoutStyle {
    let side = (RadioTokens::current().size(size) - shared::gap_sm(size) * 2.0).max(0.0);
    LayoutStyle::new().width(side).height(side)
}

fn column(size: ControlSize) -> LayoutStyle {
    LayoutStyle::column().gap(shared::gap_xs(size))
}

/// A column of radio buttons, one per option, choosing one of them into a bound `selected` index.
///
/// Each option is a ring beside its name that fills its centre dot while it is the chosen one; tapping the ring or the name chooses it. The set is one stop in the tab order, landing on the chosen option or the first while none is; the arrows move focus from option to option and choose the one they reach, wrapping at either end, with Left and Right following the writing direction, and Home and End go to the first and the last.
#[derive(Props)]
pub struct RadioGroupProps {
    /// What each option is called, in order.
    #[props(default)]
    pub items: Vec<&'static str>,
    /// Bound selection: the index of the chosen option, `None` inside the signal while none is. Leaving the prop out is uncontrolled: the group owns its own signal.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<Option<usize>>>,
    /// What a reader calls the set: "Shipping speed". `None` (the default) leaves it unnamed.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
    /// How large the radios are drawn: their rings, dots and names, the control height each stands at and the gap between them, as [`ControlSize`] works them out.
    #[props(default)]
    pub size: ControlSize,
    /// Accent (the chosen dot and its ring). `None` (the default) is the theme accent.
    #[props(some, into, default)]
    pub color: Option<Reactive<Color>>,
    /// Fires with the index a press or a key chose.
    #[props(some, default)]
    pub on_select: Option<Handler<fn(usize)>>,
    /// Amends the paint of every ring — the group's **principal surface** — chosen and not. See [`telar::SurfaceStyle`] for when a theme token or [`RadioTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A set of radio buttons choosing one of its options, or none yet.
pub fn radio_group(props: RadioGroupProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let RadioGroupProps {
        items,
        selected,
        label,
        size,
        color,
        on_select,
        style,
    } = props;
    let selected = selected.unwrap_or_else(|| signal(None));
    let choose = Rc::new(move |at: usize| {
        selected.set(Some(at));
        if let Some(cb) = &on_select {
            cb.run(at);
        }
    });
    let group = {
        let choose = Rc::clone(&choose);
        RovingGroup::new(
            RovingAxis::Both,
            move || selected.get(),
            move |at| choose(at),
        )
    };
    let options = items
        .into_iter()
        .enumerate()
        .map(|(at, name)| {
            let choose = Rc::clone(&choose);
            let ring = Ring {
                size,
                selected,
                at,
                color: color.clone(),
                style: style.clone(),
            };
            let radio = shared::labelled_control(
                size,
                ring.build()?,
                Reactive::Const(name.to_string()),
                Role::Radio,
                move || selected.get() == Some(at),
                move || choose(at),
            )?
            .roving(&group, at);
            Ok(box_item(radio))
        })
        .collect::<Result<Vec<_>, LayoutError>>()?;
    let set = Container::new(column(size), options)?
        .styled_by(move || column(size))
        .role(Role::RadioGroup);
    Ok(match label {
        Some(label) => box_item(set.a11y_label(move || label.get())),
        None => box_item(set),
    })
}

/// What one option's ring is drawn from.
struct Ring {
    size: ControlSize,
    selected: RwSignal<Option<usize>>,
    at: usize,
    color: Option<Reactive<Color>>,
    style: Option<SurfaceAmendment>,
}

impl Ring {
    /// The ring, accented with its dot filled while its option is the chosen one.
    fn build(self) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let Ring {
            size,
            selected,
            at,
            color,
            style,
        } = self;
        let chosen = move || selected.get() == Some(at);
        let dot_color = color.clone();
        let dot = StyledContainer::new(
            dot_style(size),
            move |r| {
                let fill = match chosen() {
                    true => shared::resolve(&dot_color, shared::accent),
                    false => Color::TRANSPARENT,
                };
                RectStyle::default()
                    .with_fill(fill)
                    .with_radius(shared::pill(r))
            },
            vec![],
        )?
        .styled_by(move || dot_style(size));
        let ring = StyledContainer::new(
            ring_style(size),
            move |r| {
                let stroke = match chosen() {
                    true => shared::resolve(&color, shared::accent),
                    false => shared::control_border(),
                };
                let rest = RectStyle::default()
                    .with_fill(shared::surface())
                    .with_border(Border::uniform(
                        stroke,
                        RadioTokens::current().border_width(),
                    ))
                    .with_radius(shared::pill(r));
                amend_surface(rest, &style)
            },
            children![dot],
        )?
        .styled_by(move || ring_style(size));
        Ok(box_item(ring))
    }
}

#[cfg(test)]
#[path = "radio_group.test.rs"]
mod tests;
