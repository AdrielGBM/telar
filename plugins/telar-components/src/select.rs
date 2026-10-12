//! [`select`]: a field that opens a list and writes the picked index back.

use std::rc::Rc;

#[cfg(test)]
use telar::Slots;
use telar::{
    Children, Color, LayoutError, LayoutItem, Props, Reactive, RwSignal, SurfaceAmendment, signal,
};

use crate::dropdown;
use crate::shared;
use crate::strings;
// What the test module below reads via `use super::*` to compute click points.
#[cfg(test)]
fn panel_width() -> f32 {
    dropdown::width(None)
}
#[cfg(test)]
use crate::shared::{panel_pad, row_height};
#[cfg(test)]
use telar::track_layout;

/// A select's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the trigger is laid out or painted. The rows of the list it opens follow [`ListTokens`](crate::ListTokens).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SelectTokens {
    /// Corner of the trigger. `None` is the theme's `radius_control`.
    pub radius: Option<f32>,
    /// Space between the trigger's edge and its value at the regular control size, scaled with the control size in force. `None` is [`metrics::control_pad_x`](crate::metrics::control_pad_x).
    pub pad_x: Option<f32>,
    /// Width of the trigger's frame. `None` is the theme's `border_width`.
    pub border_width: Option<f32>,
    /// Width of the trigger, unless it is stretched, and of the list it opens, at the regular control size and scaled with the control size in force. `None` is 13 ems of the document's text.
    pub width: Option<f32>,
    /// Space between the list and the trigger it drops from, at the regular control size and scaled with the control size in force. `None` is [`metrics::popup_offset`](crate::metrics::popup_offset).
    pub offset: Option<f32>,
}

impl SelectTokens {
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

    pub fn offset(&self) -> f32 {
        dropdown::offset(self.offset)
    }

    fn current_offset() -> f32 {
        Self::current().offset()
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

/// A dropdown bound to a signal: a trigger button showing the currently-selected option, and a click-opened anchored panel listing the choices. Picking one writes its index into `selected`, fires `on_select`, and closes. High-level sugar built on the overlay anchor + click-through primitives; lives in `telar-components`, not the kernel, so an app can drop it or ship its own.
///
/// Its choices are written as `item` children, the same pieces a `menu` is made of, so one can be disabled or carry an icon — which a list of strings could never say. What made that impossible for a select and not for a menu was the trigger: it has to name the current choice before the panel has ever been opened, and the rows only exist once it has. See `ListContext::declare`.
#[derive(Props)]
pub struct SelectProps {
    /// The bound selection index. `None` (the default) makes the select uncontrolled — it owns an internal signal so it still tracks a choice, just not one the caller can read.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<u32>>,
    /// The colour the trigger is framed in and the rows' washes are tinted with. `Color::TRANSPARENT` (the default) frames it in the theme accent and leaves the theme's washes as they are. A closure so a theme token re-reads on every render.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Fired with the picked index whenever a selection is made.
    #[props(some, default)]
    pub on_select: Option<Rc<dyn Fn(u32)>>,
    /// Take the width the row offers instead of the fixed trigger width — what a form field wants, where a fixed-width control beside full-width ones reads as a mistake. The panel opens at that width too.
    #[props(default = false)]
    pub stretch: bool,
    /// Amends the paint of the trigger — this component's **principal surface** — at rest, hovered and held open. See [`telar::SurfaceStyle`] for when a theme token or [`SelectTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A field that opens a list and writes the picked index back.
pub fn select(props: SelectProps, children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let rows = children;
    // `None` selection is uncontrolled: own an internal signal so the trigger still tracks a choice.
    let selected = props.selected.unwrap_or_else(|| signal(0u32));
    dropdown::dropdown(dropdown::Dropdown {
        placeholder: || strings::text(strings::SELECT),
        rows,
        color: props.color,
        on_pick: props.on_select,
        selected,
        stretch: props.stretch,
        geometry: SelectTokens::geometry,
        offset: SelectTokens::current_offset,
        style: props.style,
    })
}

#[cfg(test)]
#[path = "select_test.rs"]
mod tests;
