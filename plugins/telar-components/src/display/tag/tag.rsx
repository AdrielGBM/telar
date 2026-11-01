[logic]
use std::rc::Rc;

use crate::look::Look;
use crate::shared::{self, GlyphProps, Mark, glyph};
use telar::{ControlSize, Tone, Variant};

/// A tag's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the tag paints and lays out, so a theme that moves the role still moves every tag it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TagTokens {
    /// Corner radius. `None` is the theme's `radius_item`, the corner of a key cap; a class with `radius:full` makes a pill of one tag.
    pub radius: Option<f32>,
    /// Space between the edge and the label at the regular size, scaled with the tag's own. `None` is [`gap_md`](crate::metrics::gap_md).
    pub pad_x: Option<f32>,
    /// Width of the outline variant's frame, and of the frame round a filter that is not selected. `None` is the theme's `border_width`.
    pub border_width: Option<f32>,
}

impl TagTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// The corner radius.
    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_item)
    }

    /// The space between the edge and the label of a tag of `size`.
    pub fn pad_x(&self, size: ControlSize) -> f32 {
        self.pad_x
            .map_or_else(|| shared::gap_md(size), |pad| pad * size.scale())
    }

    /// The width of a frame the tag draws.
    pub fn border_width(&self) -> f32 {
        self.border_width.unwrap_or_else(shared::border_width)
    }
}

/// A short label for a status ("Saved"), a category ("Beta"), a value the user removes ("Logo ×") or a filter they select, at a caption's share of the text around it.
///
/// What it does follows from the props it is handed: none of them is static text, neither a control nor a Tab stop; `selected` or `on_toggle` makes the tag a filter, a toggle button, as `checkbox` takes them; a control in `action` is the caller's to act on the tag with, at its end. A filter with an action is a toggle with that control beside it, inside one tag. A counter names itself through `a11y_label` on the call: `tag label:"3" a11y_label:"3 unread comments"`.
///
/// A filter's toggle answers the pointer in every variant: a filled tag steps its fill away from its ink, a soft or outline one deepens a wash of its colour, and a ghost one shows the theme's wash. A static tag never reacts to the pointer.
///
/// A ghost tag has no surface: its label, and what sits beside it, carry the tone alone, in an ink that reads on the surface under it; its toggle shows a surface only under the pointer.
pub struct Props {
    /// The words on the tag, which also name it to assistive technology.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// What the tag's colour says: a named tone, or a colour of the caller's (`tone:$theme.brand`, `tone:#ff8800`), from which every state and the ink are worked out alike. `transparent` is transparent.
    #[props(into, default)]
    pub tone: Tone,
    /// How large the tag is drawn: its height, text, padding and gaps, as [`ControlSize`] works them out. A control in `action` is sized where it is written: a small icon button is as tall as a regular tag, and as a small tag round an action.
    #[props(default)]
    pub size: ControlSize,
    /// How the tag carries its colour.
    #[props(default)]
    pub variant: Variant,
    /// What sits before the label, in the label's ink: an `icon`, a `spinner`. Inside the toggle when the tag is a filter.
    #[props(default)]
    pub leading: Children,
    /// What sits after the label, in the label's ink.
    #[props(default)]
    pub trailing: Children,
    /// A control the caller acts on the tag with, at its end: a remove button, an "Open in a new tab" one. It is outside the toggle when the tag is a filter, a Tab stop of its own; it brings its own name, behaviour and pointer target, which the tag keeps clear of the toggle beside it.
    #[props(default)]
    pub action: Children,
    /// Makes the tag a filter bound to this state, which a press flips: a toggle button, drawn in its variant and tone and ticked while on, and in a neutral frame while off. `None` with an `on_toggle` is a filter that keeps its own state, starting off.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<bool>>,
    /// Makes the tag a filter too, and fires with the new state on every press.
    #[props(some, default)]
    pub on_toggle: Option<Rc<dyn Fn(bool)>>,
    /// Amends the paint of the tag — its **principal surface** — whatever it does. See [`telar::SurfaceStyle`] for when a theme token or [`TagTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// How tall the tag's body stands at `size`: half a control and a spacing unit, so a tag moves half as far between sizes as a control does, and never below the smallest pointer target where it is a filter.
fn body_height(size: ControlSize, filter: bool) -> f32 {
    let height = (shared::control_height(size) / 2.0 + shared::spacing(ControlSize::Regular)).round();
    match filter {
        true => height.max(shared::MIN_TARGET),
        false => height,
    }
}

fn flip(selected: RwSignal<bool>, on_toggle: Option<&dyn Fn(bool)>) {
    let next = !selected.get();
    selected.set(next);
    if let Some(on_toggle) = on_toggle {
        on_toggle(next);
    }
}

let filter = (props.selected.is_some() || props.on_toggle.is_some())
    .then(|| props.selected.unwrap_or_else(|| signal(false)));
let size = props.size;
let on_toggle = props.on_toggle.clone();
let look = Look {
    tone: props.tone.clone(),
    variant: props.variant,
    selected: filter,
    interactive: filter.is_some(),
};

[view]
row align:stretch radius:(TagTokens::current().radius()) fill:(look.fill($text.ink)) stroke:(look.frame($text.ink)) stroke_width:(TagTokens::current().border_width()) font_size:(size.text_scale() * shared::CAPTION_RATIO)em text_wrap:nowrap surface:$props.style
    if let Some(selected) = filter
        row role:button toggled:selected.get() on_press:(move || flip(selected, on_toggle.as_deref())) cursor:pointer color:(look.ink($text.ink)) fill:transparent radius:(TagTokens::current().radius()) hover_style(fill:(look.hovered($text.ink)) radius:(TagTokens::current().radius())) active_style(fill:(look.pressed($text.ink)) radius:(TagTokens::current().radius())) align:center gap:(shared::gap_xs(size)) min_height:(body_height(size, true)) min_width:(shared::MIN_TARGET) pad_start:(TagTokens::current().pad_x(size)) pad_end:(TagTokens::current().pad_x(size))
            // A leading icon carries an inset round its ink and the drawn tick fills its box, so the tick takes that inset as padding to stand as far from the label.
            row shown:selected.get() pad_end:(shared::gap_xs(size))
                glyph shape:Mark::Check size:size
            children $props.leading
            text "{$props.label}"
            children $props.trailing
    else
        row align:center gap:(shared::gap_xs(size)) min_height:(body_height(size, false)) color:(look.ink($text.ink)) pad_start:(TagTokens::current().pad_x(size)) pad_end:(TagTokens::current().pad_x(size))
            children $props.leading
            text "{$props.label}"
            children $props.trailing
    // The ink is declared on each part rather than on the tag, so every part reads the ink of the place the tag sits in rather than the one it works out from it.
    row align:center color:(look.ink($text.ink))
        children $props.action
