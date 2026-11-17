//! [`select`]: a field that opens a list and writes the picked index back.

use telar::{
    Children, Color, Handler, LayoutError, LayoutItem, Props, Reactive, RwSignal, SurfaceAmendment,
    signal,
};

use crate::shared;
use crate::shared::dropdown;
use crate::strings;
use telar::ControlSize;
// What the test module below reads via `use super::*` to compute click points.
#[cfg(test)]
fn panel_width() -> f32 {
    dropdown::width(ControlSize::Regular, None)
}
#[cfg(test)]
use crate::shared::{panel_pad, row_height};
#[cfg(test)]
use telar::track_layout;

/// A select's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the trigger is laid out or painted. The rows of the list it opens follow [`ListTokens`](crate::ListTokens).
#[derive(Clone, Copy, Debug, Default, PartialEq, telar::ComponentTokens)]
pub struct SelectTokens {
    /// Corner of the trigger. `None` is the theme's `radius_control`.
    #[token(or = shared::radius_control)]
    pub radius: Option<f32>,
    /// Space between the trigger's edge and its value at the regular size, scaled with the select's own. `None` is [`metrics::control_pad_x`](crate::metrics::control_pad_x).
    #[token(or = shared::control_pad_x, scaled)]
    pub pad_x: Option<f32>,
    /// Width of the trigger's frame. `None` is the theme's `border_width`.
    #[token(or = shared::border_width)]
    pub border_width: Option<f32>,
    /// The width the trigger takes where what holds it leaves the room, and the least the list it opens takes, at the regular size and scaled with the select's own: the trigger is narrower where the room is less, and as its call places it (`width:100%`). `None` is 13 ems of the document's text.
    pub width: Option<f32>,
    /// Space between the list and the trigger it drops from, at the regular size and scaled with the select's own. `None` is [`metrics::popup_offset`](crate::metrics::popup_offset).
    pub offset: Option<f32>,
}

impl SelectTokens {
    /// The width the trigger of a select of `size` takes where it has the room, and the least the list it opens takes.
    pub fn width(&self, size: ControlSize) -> f32 {
        dropdown::width(size, self.width)
    }

    /// The space between the list and the trigger of a select of `size`.
    pub fn offset(&self, size: ControlSize) -> f32 {
        dropdown::offset(size, self.offset)
    }

    fn current_offset(size: ControlSize) -> f32 {
        Self::current().offset(size)
    }

    fn geometry(size: ControlSize) -> dropdown::Geometry {
        let tokens = Self::current();
        dropdown::Geometry {
            radius: tokens.radius(),
            pad_x: tokens.pad_x(size),
            border_width: tokens.border_width(),
            width: tokens.width(size),
        }
    }
}

/// A dropdown bound to a signal: a trigger button showing the currently-selected option, and a click-opened anchored panel listing the choices. Picking one writes its index into `selected`, fires `on_select`, and closes. High-level sugar built on the overlay anchor + click-through primitives; lives in `telar-components`, not the kernel, so an app can drop it or ship its own.
///
/// Its choices are written as `item` children, the same pieces a `menu` is made of, so one can be disabled or carry an icon — which a list of strings could never say. What made that impossible for a select and not for a menu was the trigger: it has to name the current choice before the panel has ever been opened, and the rows only exist once it has. See `ListContext::declare`.
#[derive(Props)]
pub struct SelectProps {
    /// The bound selection index, `None` inside the signal while nothing is chosen and the trigger shows its placeholder. Leaving the prop out makes the select uncontrolled — it owns an internal signal so it still tracks a choice, just not one the caller can read.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<Option<usize>>>,
    /// How large the field and its list are drawn: the field's height, padding, value and caret, and the list's padding, width and offset, as [`ControlSize`] works them out. The choices are sized where they are written.
    #[props(default)]
    pub size: ControlSize,
    /// The colour the trigger is framed in and the rows' washes are tinted with. `None` (the default) frames it in the theme's `control_border` and leaves the theme's washes as they are. A closure so a theme token re-reads on every render.
    #[props(some, into, default)]
    pub color: Option<Reactive<Color>>,
    /// Fired with the picked index whenever a selection is made.
    #[props(some, into, default)]
    pub on_select: Option<Handler<fn(usize)>>,
    /// Amends the paint of the trigger — this component's **principal surface** — at rest, hovered and held open. See [`telar::SurfaceStyle`] for when a theme token or [`SelectTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
    /// The choices, as [`item`](crate::item)s.
    #[props(default)]
    pub children: Children,
}

/// A field that opens a list and writes the picked index back.
pub fn select(props: SelectProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SelectProps {
        selected,
        size,
        color,
        on_select,
        style,
        children,
    } = props;
    // `None` selection is uncontrolled: own an internal signal so the trigger still tracks a choice.
    let selected = selected.unwrap_or_else(|| signal(None));
    dropdown::dropdown(dropdown::Dropdown {
        placeholder: || strings::text(strings::SELECT),
        size,
        rows: children,
        color,
        on_pick: on_select,
        selected,
        geometry: SelectTokens::geometry,
        offset: SelectTokens::current_offset,
        style,
    })
}

#[cfg(test)]
#[path = "select.test.rs"]
mod tests;
