//! [`swatches`]: a row of colour swatches selecting one of them, or none.

use std::rc::Rc;

use telar::{
    Accessible, AlignItems, Border, Color, Container, Cursor, JustifyContent, Key, LayoutError,
    LayoutItem, LayoutStyle, NamedKey, Props, RectStyle, RwSignal, ShapeStyle, StyledContainer,
    SurfaceAmendment, amend_surface, box_item, children, focus::Role, signal,
};

use crate::shared;
use telar::ControlSize;

/// A swatch row's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the swatches paint.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SwatchesTokens {
    /// Diameter of a swatch at the regular size, scaled with the row's own. `None` is [`metrics::icon_size`](crate::metrics::icon_size) and [`metrics::gap_sm`](crate::metrics::gap_sm).
    pub size: Option<f32>,
    /// Width of the frame round the chosen swatch. `None` is the theme's `border_width_strong`.
    pub ring: Option<f32>,
}

impl SwatchesTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// The diameter of a swatch in a row of `size`.
    pub fn size(&self, size: ControlSize) -> f32 {
        self.size.map_or_else(
            || shared::icon_size(size) + shared::gap_sm(size),
            |diameter| diameter * size.scale(),
        )
    }

    pub fn ring(&self) -> f32 {
        self.ring.unwrap_or_else(shared::border_width_strong)
    }
}

fn hit_size(diameter: f32) -> f32 {
    diameter.max(shared::MIN_TARGET)
}

/// The swatches stand `gap_sm` apart where they are drawn, so the air their hit areas already take is subtracted from it.
fn row(size: ControlSize, diameter: f32) -> LayoutStyle {
    let spacing = (shared::gap_sm(size) - (hit_size(diameter) - diameter)).max(0.0);
    LayoutStyle::row().flex_wrap().gap(spacing)
}

fn disc(diameter: f32) -> LayoutStyle {
    LayoutStyle::new().width(diameter).height(diameter)
}

fn cell(diameter: f32) -> LayoutStyle {
    let hit = hit_size(diameter);
    LayoutStyle::new()
        .width(hit)
        .height(hit)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
}

/// A row of colour swatches selecting one of them, or none: the picker for a value drawn from a fixed palette (a theme's accents, a set of tokens).
#[derive(Props)]
pub struct SwatchesProps {
    /// The colours offered, in order.
    #[props(default)]
    pub colors: Vec<Color>,
    /// What each swatch is called, for assistive technology; one per colour (missing names read as empty).
    #[props(default)]
    pub names: Vec<String>,
    /// Bound selection: the index of the chosen colour, `None` for none of them. `None` for the prop itself (the default) is uncontrolled.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<Option<u32>>>,
    /// How large the row is drawn: its swatches and the gap between them, as [`ControlSize`] works them out.
    #[props(default)]
    pub size: ControlSize,
    /// Each swatch's drawn diameter in px. `0.0` (the default) means "unset": the [`SwatchesTokens`] size at the row's size. What takes the press around it is never smaller than 24 px, so a small swatch stays easy to hit.
    #[props(default)]
    pub diameter: f32,
    /// Fires with the index a press or a key selected.
    #[props(some, default)]
    pub on_select: Option<Rc<dyn Fn(u32)>>,
    /// Amends the paint of every swatch — the row's **principal surface** — chosen or not. See [`telar::SurfaceStyle`] for when a theme token or [`SwatchesTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A row of colour swatches selecting one of them, or none.
pub fn swatches(props: SwatchesProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SwatchesProps {
        colors,
        names,
        selected,
        size,
        diameter,
        on_select,
        style,
    } = props;
    let diameter = move || {
        if diameter > 0.0 {
            diameter
        } else {
            SwatchesTokens::current().size(size)
        }
    };
    let selected = selected.unwrap_or_else(|| signal(None::<u32>));
    let count = colors.len() as u32;
    let mut names = names.into_iter();
    let mut cells: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(colors.len());

    for (i, color) in colors.into_iter().enumerate() {
        let idx = i as u32;
        let name = names.next().unwrap_or_default();
        let paint_selected = selected;
        let announced = selected;
        let press_selected = selected;
        let key_selected = selected;
        let press_cb = on_select.clone();
        let key_cb = on_select.clone();
        let style = style.clone();

        let face = StyledContainer::new(
            disc(diameter()),
            move |r| {
                let rest = swatch_rect(color, paint_selected.get() == Some(idx))
                    .with_radius(shared::pill(r));
                amend_surface(rest, &style)
            },
            vec![],
        )?
        .styled_by(move || disc(diameter()));
        let swatch =
            StyledContainer::new(cell(diameter()), |_r| RectStyle::default(), children![face])?
                .styled_by(move || cell(diameter()))
                .a11y_label(move || name.clone())
                .control(Role::Radio)
                .toggled(move || announced.get() == Some(idx))
                .cursor(Cursor::Pointer)
                .on_press(move || choose(press_selected, idx, press_cb.as_deref()))
                .on_focused_key(move |key: &Key| -> bool {
                    let Key::Named(named) = key else {
                        return false;
                    };
                    let current = key_selected.peek().unwrap_or(idx);
                    let target = match named {
                        NamedKey::ArrowRight | NamedKey::ArrowDown => (current + 1).min(count - 1),
                        NamedKey::ArrowLeft | NamedKey::ArrowUp => current.saturating_sub(1),
                        NamedKey::Home => 0,
                        NamedKey::End => count - 1,
                        _ => return false,
                    };
                    choose(key_selected, target, key_cb.as_deref());
                    true
                });
        cells.push(box_item(swatch));
    }

    Ok(box_item(
        Container::new(row(size, diameter()), cells)?.styled_by(move || row(size, diameter())),
    ))
}

fn choose(selected: RwSignal<Option<u32>>, idx: u32, on_select: Option<&dyn Fn(u32)>) {
    selected.set_if_changed(Some(idx));
    if let Some(cb) = on_select {
        cb(idx);
    }
}

fn swatch_rect(color: Color, chosen: bool) -> RectStyle {
    let border = if chosen {
        Border::uniform(
            shared::ink_on(shared::surface()),
            SwatchesTokens::current().ring(),
        )
    } else {
        Border::uniform(shared::border(), shared::border_width())
    };
    RectStyle::default().with_fill(color).with_border(border)
}

#[cfg(test)]
#[path = "swatches.test.rs"]
mod tests;
