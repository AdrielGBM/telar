//! [`menu`]: a button that opens a list of one-shot actions.

use std::rc::Rc;

#[cfg(test)]
use telar::Slots;
use telar::{Children, Color, LayoutError, LayoutItem, Props, Reactive, SurfaceAmendment};

use crate::dropdown;
use crate::shared;
// What the test module below reads via `use super::*` to compute click points.
#[cfg(test)]
fn panel_width() -> f32 {
    dropdown::width(None)
}
#[cfg(test)]
use crate::shared::{panel_pad, row_height};
#[cfg(test)]
use telar::track_layout;

/// A menu's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the trigger is laid out or painted. The rows of the list it opens follow [`ListTokens`](crate::ListTokens).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MenuTokens {
    /// Corner of the trigger. `None` is the theme's `radius_control`.
    pub radius: Option<f32>,
    /// Space between the trigger's edge and its label at the regular control size, scaled with the control size in force. `None` is [`metrics::control_pad_x`](crate::metrics::control_pad_x).
    pub pad_x: Option<f32>,
    /// Width of a bordered trigger's frame. `None` is the theme's `border_width`.
    pub border_width: Option<f32>,
    /// Width of the trigger, unless it is stretched or fitted, and of the list it opens, at the regular control size and scaled with the control size in force. `None` is 13 ems of the document's text.
    pub width: Option<f32>,
}

impl MenuTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_control)
    }

    pub fn pad_x(&self) -> f32 {
        self.pad_x
            .map_or_else(shared::control_pad_x, |pad| pad * telar::control_scale())
    }

    pub fn border_width(&self) -> f32 {
        self.border_width.unwrap_or_else(shared::border_width)
    }

    pub fn width(&self) -> f32 {
        dropdown::width(self.width)
    }

    fn geometry() -> dropdown::Geometry {
        let tokens = Self::current();
        dropdown::Geometry {
            radius: tokens.radius(),
            pad_x: tokens.pad_x(),
            border_width: tokens.border_width(),
            width: tokens.width(),
        }
    }
}

/// A click-triggered list of action items: a labelled trigger button that opens an anchored list; picking an item fires `on_select` with its index and closes. Unlike `select`, a menu holds no bound selection state — its items are one-shot actions. High-level sugar built on the overlay anchor + click-through primitives; lives in `telar-components`, not the kernel.
#[derive(Props)]
pub struct MenuProps {
    /// The trigger button's label.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Fired with the index of the chosen item when it is picked.
    #[props(some, default)]
    pub on_select: Option<Rc<dyn Fn(u32)>>,
    /// The colour a bordered trigger is framed in and the rows' washes are tinted with. `Color::TRANSPARENT` (the default) frames it in the theme accent and leaves the theme's washes as they are. A closure so a theme token re-reads on every render.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Take the width the row offers instead of the fixed trigger width — see [`crate::SelectProps::stretch`].
    #[props(default)]
    pub stretch: bool,
    /// Draw the trigger as a field, with the border a `select` carries. Off by default, because a menu is a *button* that happens to open a list, and a button wears no frame until it is pressed.
    ///
    /// A prop and not a decision settled inside the component, because both readings are legitimate: a menu standing alone in a header wants no frame, and one sitting in a row of fields wants to match them.
    #[props(default)]
    pub bordered: bool,
    /// Show the caret that says the trigger opens something. On by default. Without it the label stands centred in the trigger, as a button's does.
    #[props(default = true)]
    pub caret: bool,
    /// Size the trigger to its label instead of the fixed width a column of fields shares. Off by default; the list it opens keeps its own width.
    #[props(default)]
    pub fit: bool,
    /// Amends the paint of the trigger — this component's **principal surface**, the thing a caller means when they point at a menu — at rest, hovered and held open. See [`telar::SurfaceStyle`] for when a theme token or [`MenuTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

// A menu carries no bound selection, so its rows are one-shot actions: no index is written back and no row is highlighted.
/// A button that opens a list of one-shot actions.
pub fn menu(props: MenuProps, children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    dropdown::dropdown(dropdown::Dropdown {
        label: dropdown::TriggerLabel::Fixed(props.label),
        rows: children,
        color: props.color,
        on_pick: props.on_select,
        selected: None,
        stretch: props.stretch,
        bordered: props.bordered,
        caret: props.caret,
        fit: props.fit,
        geometry: MenuTokens::geometry,
        style: props.style,
    })
}

#[cfg(test)]
#[path = "menu_test.rs"]
mod tests;
