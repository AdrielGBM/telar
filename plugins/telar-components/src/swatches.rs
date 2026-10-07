//! [`swatches`]: a row of colour swatches selecting one of them, or none.

use std::rc::Rc;

use telar::{
    Accessible, Border, BorderRadius, Children, Color, Container, Cursor, Key, LayoutError,
    LayoutItem, LayoutStyle, NamedKey, Props, RectStyle, RwSignal, ShapeStyle, StyledContainer,
    box_item, focus::Role, signal,
};

use crate::shared;

const RING_WIDTH: f32 = 2.0;
const HAIRLINE_WIDTH: f32 = 1.0;

fn gap() -> f32 {
    shared::spacing() * 0.5
}
fn row() -> LayoutStyle {
    LayoutStyle::new().flex_row().flex_wrap().gap(gap())
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
    /// Each swatch's diameter. Default 20.
    #[props(default = 20.0)]
    pub size: f32,
    /// Fires with the index a press or a key selected.
    #[props(some, default)]
    pub on_select: Option<Rc<dyn Fn(u32)>>,
}

/// A row of colour swatches selecting one of them, or none.
pub fn swatches(
    props: SwatchesProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SwatchesProps {
        colors,
        names,
        selected,
        size,
        on_select,
    } = props;
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

        let swatch = StyledContainer::new(
            LayoutStyle::new().width(size).height(size),
            move |_r| swatch_rect(color, size, paint_selected.get() == Some(idx)),
            vec![],
        )?
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

    Ok(box_item(Container::new(row(), cells)?.styled_by(row)))
}

fn choose(selected: RwSignal<Option<u32>>, idx: u32, on_select: Option<&dyn Fn(u32)>) {
    if selected.peek() != Some(idx) {
        selected.set(Some(idx));
    }
    if let Some(cb) = on_select {
        cb(idx);
    }
}

fn swatch_rect(color: Color, size: f32, chosen: bool) -> RectStyle {
    let border = if chosen {
        Border::uniform(shared::ink_on(shared::surface()), RING_WIDTH)
    } else {
        Border::uniform(shared::border(), HAIRLINE_WIDTH)
    };
    RectStyle::default()
        .with_fill(color)
        .with_border(border)
        .with_radius(BorderRadius::all(size / 2.0))
}

#[cfg(test)]
#[path = "swatches_test.rs"]
mod tests;
