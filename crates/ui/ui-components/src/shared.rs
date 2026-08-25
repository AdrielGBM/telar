//! Internal helpers shared across the widget catalogue: the reactive-colour resolver, the named theme reads
//! every component paints from, and the labelled-control row scaffold. Not part of the public API
//! (`pub(crate)`).

use std::rc::Rc;

use layout_core::{AlignItems, LayoutError, LayoutStyle};
use reactive_core::RwSignal;
use renderer_core::{Color, RectStyle, TextStyle};
use theme_core::use_theme_tokens;
use ui_core::focus::Role;
use ui_core::{Container, LayoutItem, StyledContainer, Text, box_item, style_follows};

/// A reactive colour prop re-erased to a shareable handle: a `Box<dyn Fn>` isn't `Clone`, but widgets must hand the same colour closure to several style closures, so they re-erase to this `Rc`.
pub(crate) type ReactiveColor = Rc<dyn Fn() -> Color>;

/// A reactive human-text prop re-erased to a shareable handle: a `Box<dyn Fn>` isn't `Clone`, but a widget that reuses the same label/title in several places (or rebuilds it on each mount) re-erases to this `Rc`.
pub(crate) type ReactiveText = Rc<dyn Fn() -> String>;

/// An amendment to the paint a component worked out for its **principal surface** — the one a caller means
/// when they point at the control: a button's box, a menu's trigger, a tooltip's bubble.
///
/// It takes the finished [`RectStyle`] and hands back another, rather than being a `radius` or a `fill` prop,
/// and that is the whole point. A component resolves its surface *per state* — hovered, pressed, bordered,
/// themed — so a prop naming one property would have to be threaded through every one of those branches and
/// would still only cover the property it named. Amending the result composes with the states instead of
/// competing with them: `|s| s.with_radius(BorderRadius::all(2.0))` re-rounds the hovered style too, and
/// nothing about the component's own logic has to know it happened.
///
/// This is the per-instance half of styling. The other two halves already exist and are not this: **theme
/// tokens** for what a whole application should agree on (how round anything is), and **props** for what
/// changes the shape rather than the paint (whether a menu wears a field's border). Reaching for this to say
/// something every menu should say is how a design system comes apart one call site at a time.
pub(crate) type SurfaceStyle = Option<Rc<dyn Fn(RectStyle) -> RectStyle>>;

/// Applies a caller's amendment, if there is one.
pub(crate) fn amend(style: RectStyle, over: &SurfaceStyle) -> RectStyle {
    match over {
        Some(f) => f(style),
        None => style,
    }
}

/// The catalogue's text ink. Call it INSIDE a style closure so widget text recolours when the theme switches
/// (e.g. dark mode).
pub(crate) fn ink() -> Color {
    use_theme_tokens().ink()
}
/// Readable ink for a label sitting on `fill`: whichever of the theme's `ink` and `on_primary` contrasts
/// with it more.
///
/// A hard-coded white was right only while a filled control was assumed to carry a saturated accent. A
/// neutral palette — the greys a shadcn-style theme builds on, say — makes `primary` a near-white in dark
/// mode, and the label disappeared into its own button. Reading both ends of the theme and picking by luminance keeps a caller
/// free to pass any colour at all — which the `fill:` prop already lets them do.
pub(crate) fn ink_on(fill: Color) -> Color {
    let dark = ink();
    let light = use_theme_tokens().on_primary();
    if contrast(fill, dark) >= contrast(fill, light) {
        dark
    } else {
        light
    }
}

/// Difference in perceived luminance, the cheap stand-in for a full WCAG contrast ratio: enough to choose
/// between two candidate inks, and it needs no gamma round-trip.
fn contrast(a: Color, b: Color) -> f32 {
    (luminance(a) - luminance(b)).abs()
}

fn luminance(c: Color) -> f32 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

/// The panel surface a floating thing sits on.
pub(crate) fn surface() -> Color {
    use_theme_tokens().surface()
}
/// The quiet surface behind a chip or a tag.
pub(crate) fn surface_alt() -> Color {
    use_theme_tokens().surface_alt()
}
/// The hairline border and divider tone.
pub(crate) fn border() -> Color {
    use_theme_tokens().border()
}
pub(crate) fn accent() -> Color {
    use_theme_tokens().primary()
}
/// Readable ink for a label sitting on [`accent`].
pub(crate) fn on_accent() -> Color {
    use_theme_tokens().on_primary()
}
/// Secondary text: captions, placeholders, and the rail a slider or a spinner runs along.
pub(crate) fn muted() -> Color {
    use_theme_tokens().muted()
}

/// Base corner radius from the theme. A component that wants a different shape multiplies this rather than
/// declaring its own constant, so one theme number still moves it.
pub(crate) fn radius() -> f32 {
    use_theme_tokens().radius()
}
/// The steps either side of [`radius`], for the shapes that are not a card: a chip or a row rounds less, a
/// panel or a bubble sits between. Every one of these was a literal in the component that drew it, which
/// meant a theme could move its base radius and watch half the catalogue ignore it.
pub(crate) fn radius_sm() -> f32 {
    use_theme_tokens().radius_sm()
}
pub(crate) fn radius_md() -> f32 {
    use_theme_tokens().radius_md()
}
/// Base spacing unit from the theme, and what a component derives its own padding from.
///
/// Scaled by the ambient [`ControlSize`](theme_core::ControlSize) — which is the whole of how a control gets
/// smaller here: each component keeps its own proportions and the unit underneath them moves. See
/// `theme-core`'s `density` module for why that is one number rather than a size matrix per component.
pub(crate) fn spacing() -> f32 {
    use_theme_tokens().spacing() * theme_core::control_scale()
}
/// Base body text size from the theme, scaled by the ambient control size.
///
/// A component scales this by a ratio of its own — `font_size() * 1.4` for a heading, `* 0.85` for a caption —
/// instead of asking for a named role. Naming the roles here would decide for every application which roles it
/// is allowed to have, and a theme's own vocabulary belongs to its own type.
///
/// For the *text* of a control, prefer [`control_text`]: this reads the theme directly and so cannot see a
/// container that declared a size for the region the control sits in.
pub(crate) fn font_size() -> f32 {
    use_theme_tokens().font_size() * theme_core::control_scale()
}

/// A single line's box height, as a multiple of the text in it.
pub(crate) const LINE_LEADING: f32 = 1.4;
/// A caption's share of the text it labels.
pub(crate) const CAPTION_RATIO: f32 = 0.85;
/// A title's share of the body text around it.
pub(crate) const HEADING_RATIO: f32 = 1.4;

/// The text a control draws: whatever the tree above it says, at the ambient control density, at this
/// control's own ratio to the body size around it.
///
/// The ratio is how a component says "a caption" or "a close glyph" — a proportion of the text beside it
/// rather than a number of its own. Taking the inherited style as the starting point is the whole of B9's
/// fix: a field's label and a plain label next to it can no longer be two sizes, because there is one size
/// and each control names its distance from it.
pub(crate) fn control_text(inherited: TextStyle, ratio: f32) -> TextStyle {
    let size = inherited.font_size * theme_core::control_scale() * ratio;
    inherited.with_font_size(size)
}

/// [`control_text`]'s size on its own, for the box a control sizes to hold that text.
///
/// Takes the node rather than the style because a box is sized outside the closure that styles its text —
/// and reading the context here is what makes the box follow a declaration the text is already following.
pub(crate) fn control_text_size(node: ui_core::NodeId, ratio: f32) -> f32 {
    control_text(ui_core::inherited_text_style(node), ratio).font_size
}

/// Default standalone icon size from the theme, scaled by the ambient control size.
pub(crate) fn icon_size() -> f32 {
    use_theme_tokens().icon_size() * theme_core::control_scale()
}

/// Resolve a reactive colour: `color()` unless it is `Color::TRANSPARENT` (the "unset" sentinel), else `fallback()`.
/// `color()` is evaluated once. Collapses the per-widget accent/track/surface/bubble resolvers into one shape.
pub(crate) fn resolve(color: &dyn Fn() -> Color, fallback: impl FnOnce() -> Color) -> Color {
    let c = color();
    if c == Color::TRANSPARENT {
        fallback()
    } else {
        c
    }
}

/// The open/close state a scrim overlay is driven by: an explicitly bound `open` signal, else the shared
/// state of the `id` it is named by, else `None` (unbound — it can never open).
///
/// An explicit signal wins on purpose. Given both, the two would be independent states racing each other, and
/// the one written next to the widget is the one the author most likely meant.
pub(crate) fn resolve_open(
    open: Option<RwSignal<bool>>,
    id: &'static str,
) -> Option<RwSignal<bool>> {
    open.or_else(|| (!id.is_empty()).then(|| ui_core::overlay_state(id)))
}

/// A control (checkbox box / radio ring / toggle pill) plus an optional label, laid out as one gap-10 row that
/// is itself the tap target: a tap runs `on_press`. Shared by checkbox, radio and toggle.
/// Writes an `impl Default` for a `Props` struct from a field list, naming each field's *kind* instead of
/// repeating its value.
///
/// The catalogue's 22 `Default` impls were 226 lines in which the same four expressions appeared 41 times
/// between them — `Box::new(String::new)` nineteen times, `Box::new(|| Color::TRANSPARENT)` twenty-one. They
/// cannot be `#[derive(Default)]`: a `Box<dyn Fn() -> String>` has no `Default`, and the field types stay
/// exactly as they are because `scan_props_struct` detects them by their literal type text, so a newtype would
/// move the `.rsx` authoring surface.
///
/// Kinds: `text` (an empty reactive string), `color` (unset — `TRANSPARENT`, the catalogue's "fall back to the
/// theme" sentinel), `flag` (a reactive `false`), `action` (a no-op callback), and `= <expr>` for a field whose
/// default carries information. A struct where most fields are the last kind is better written by hand.
macro_rules! props_default {
    ($ty:ident { $($field:ident : $kind:tt),* $(,)? }) => {
        impl Default for $ty {
            fn default() -> Self {
                Self { $($field: props_default!(@value $kind)),* }
            }
        }
    };
    (@value text) => { Box::new(String::new) };
    (@value color) => { Box::new(|| renderer_core::Color::TRANSPARENT) };
    (@value flag) => { Box::new(|| false) };
    (@value action) => { Box::new(|| {}) };
    (@value none) => { None };
    (@value zero) => { Default::default() };
    (@value reading) => { Box::new(|| 0.0) };
    (@value ($e:expr)) => { $e };
}
pub(crate) use props_default;

/// The shared title text style, re-read every frame so it tracks the active theme. Three consumers: `heading`,
/// `section` and `modal`'s title.
///
/// A ratio of the text around it, where it used to be a flat 20px — so a theme that made its body text 11px
/// left every title at the size a 14px body wanted.
pub(crate) fn heading_style(inherited: TextStyle) -> TextStyle {
    control_text(inherited, HEADING_RATIO)
        .with_color(accent())
        .with_font_weight(600)
}

fn caption_box(text_size: f32) -> LayoutStyle {
    LayoutStyle::new().height(text_size * LINE_LEADING)
}

fn caption_column(width: f32) -> LayoutStyle {
    LayoutStyle::new()
        .flex_column()
        .gap(spacing() * 0.75)
        .width(width)
}

/// Stacks an optional small caption above `control`, or hands `control` back untouched when the label is
/// empty. The counterpart of [`labelled_control`], which puts the label *beside* the control instead.
pub(crate) fn captioned(
    control: Box<dyn LayoutItem>,
    label: impl Fn() -> String + 'static,
    width: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    if label().is_empty() {
        return Ok(box_item(control));
    }
    let caption = Text::declaring(
        move || label(),
        caption_box(font_size() * CAPTION_RATIO),
        |t| control_text(t, CAPTION_RATIO).with_color(muted()),
    )?;
    // The caption is a leaf, so its own node's style is followed from here — the column outlives it and is where an effect belonging to this subtree wants to be owned.
    let caption_node = caption.layout_node();
    let col = Container::new(
        caption_column(width),
        vec![box_item(caption), box_item(control)],
    )?
    .styled_by(move || caption_column(width))
    .keeping(style_follows(caption_node, move || {
        caption_box(control_text_size(caption_node, CAPTION_RATIO))
    }));
    Ok(box_item(col))
}

pub(crate) fn labelled_control(
    control: Box<dyn LayoutItem>,
    label: impl Fn() -> String + 'static,
    role: Role,
    toggled: impl Fn() -> bool + 'static,
    on_press: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut children: Vec<Box<dyn LayoutItem>> = vec![box_item(control)];
    if !label().is_empty() {
        let text = Text::declaring(
            move || label(),
            LayoutStyle::new(),
            |t| control_text(t, 1.0),
        )?;
        children.push(box_item(text));
    }
    // The whole row is the control, label included: clicking the word beside a checkbox toggles it, and the
    // ring goes round both — which is also what makes the label the thing a reader names it by.
    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .gap(spacing() * 1.25)
            .align_items(AlignItems::CENTER),
        |_r| RectStyle::default(),
        children,
    )?
    .control(role)
    .toggled(toggled)
    .on_press(on_press);
    Ok(box_item(row))
}
