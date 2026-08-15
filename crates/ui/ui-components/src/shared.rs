//! Internal helpers shared across the widget catalogue: the reactive-colour resolver, the common accent/surface
//! fallback constants, and the labelled-control row scaffold. Not part of the public API (`pub(crate)`).

use std::rc::Rc;

use layout_core::{AlignItems, LayoutError, LayoutStyle};
use reactive_core::RwSignal;
use renderer_core::{Color, RectStyle, TextStyle};
use theme_core::use_theme_tokens;
use ui_core::focus::Role;
use ui_core::{LayoutItem, StyledContainer, Text, box_item};

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

/// Fallback accent when no reactive `color` is supplied and no theme is active (matches `Button`'s default primary).
pub(crate) const DEFAULT_ACCENT: Color = Color::rgba(0.24, 0.47, 0.98, 1.0);
/// Opaque white surface fallback shared by `modal` and `drawer` when their `color` is unset.
pub(crate) const DEFAULT_SURFACE: Color = Color::rgba(1.0, 1.0, 1.0, 1.0);
/// Default text ink for the catalogue (labels, titles, values). `ThemeTokens` exposes no ink token, so
/// this is the shared fallback for text that isn't an accent.
pub(crate) const INK: Color = Color::rgba(0.15, 0.15, 0.2, 1.0);
/// Muted rail/track fallback shared by `slider`/`progress`/`spinner` when their `track_color` is unset.
pub(crate) const DEFAULT_TRACK: Color = Color::rgba(0.5, 0.5, 0.6, 0.3);
/// Quiet surface-alt fallback (chip/tag backgrounds) when no theme is active.
pub(crate) const SURFACE_ALT: Color = Color::rgba(0.5, 0.5, 0.55, 0.1);
/// Hairline border fallback when no theme is active.
pub(crate) const BORDER: Color = Color::rgba(0.5, 0.5, 0.55, 0.35);

/// Theme-resolved text ink (`ink`), falling back to [`INK`] when no theme is active. Call it
/// INSIDE a style closure so widget text recolours when the theme switches (e.g. dark mode).
pub(crate) fn ink() -> Color {
    use_theme_tokens().map(|t| t.ink()).unwrap_or(INK)
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
    let light = use_theme_tokens()
        .map(|t| t.on_primary())
        .unwrap_or(Color::rgba(1.0, 1.0, 1.0, 1.0));
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

/// Theme-resolved panel surface (`surface`), falling back to [`DEFAULT_SURFACE`].
pub(crate) fn surface() -> Color {
    use_theme_tokens()
        .map(|t| t.surface())
        .unwrap_or(DEFAULT_SURFACE)
}
/// Theme-resolved quiet surface (`surface_alt`), falling back to [`SURFACE_ALT`].
pub(crate) fn surface_alt() -> Color {
    use_theme_tokens()
        .map(|t| t.surface_alt())
        .unwrap_or(SURFACE_ALT)
}
/// Theme-resolved hairline border (`border`), falling back to [`BORDER`].
pub(crate) fn border() -> Color {
    use_theme_tokens().map(|t| t.border()).unwrap_or(BORDER)
}

/// Base corner radius from the theme. A component that wants a different shape multiplies this rather than
/// declaring its own constant, so one theme number still moves it.
pub(crate) fn radius() -> f32 {
    use_theme_tokens().map(|t| t.radius()).unwrap_or(4.0)
}
/// The steps either side of [`radius`], for the shapes that are not a card: a chip or a row rounds less, a
/// panel or a bubble sits between. Every one of these was a literal in the component that drew it, which
/// meant a theme could move its base radius and watch half the catalogue ignore it.
pub(crate) fn radius_sm() -> f32 {
    use_theme_tokens().map(|t| t.radius_sm()).unwrap_or(2.4)
}
pub(crate) fn radius_md() -> f32 {
    use_theme_tokens().map(|t| t.radius_md()).unwrap_or(3.2)
}
/// Base spacing unit from the theme, and what a component derives its own padding from.
///
/// Scaled by the ambient [`ControlSize`](theme_core::ControlSize) — which is the whole of how a control gets
/// smaller here: each component keeps its own proportions and the unit underneath them moves. See
/// `theme-core`'s `density` module for why that is one number rather than a size matrix per component.
pub(crate) fn spacing() -> f32 {
    use_theme_tokens().map(|t| t.spacing()).unwrap_or(8.0) * theme_core::control_scale()
}
/// Base body text size from the theme, scaled by the ambient control size.
///
/// A component scales this by a ratio of its own — `font_size() * 1.4` for a heading, `* 0.85` for a caption —
/// instead of asking for a named role. Naming the roles here would decide for every application which roles it
/// is allowed to have, and a theme's own vocabulary belongs to its own type.
pub(crate) fn font_size() -> f32 {
    use_theme_tokens().map(|t| t.font_size()).unwrap_or(14.0) * theme_core::control_scale()
}
/// Default standalone icon size from the theme, scaled by the ambient control size.
pub(crate) fn icon_size() -> f32 {
    use_theme_tokens().map(|t| t.icon_size()).unwrap_or(16.0) * theme_core::control_scale()
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
    open.or_else(|| (!id.is_empty()).then(|| ui_core::named_overlay::state(id)))
}

/// A control (checkbox box / radio ring / toggle pill) plus an optional label, laid out as one gap-10 row that
/// is itself the tap target: a tap runs `on_press`. Shared by checkbox, radio and toggle.
pub(crate) fn labelled_control(
    control: Box<dyn LayoutItem>,
    label: impl Fn() -> String + 'static,
    role: Role,
    toggled: impl Fn() -> bool + 'static,
    on_press: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut children: Vec<Box<dyn LayoutItem>> = vec![box_item(control)];
    if !label().is_empty() {
        // `auto` (measured leaf) so the label gets its intrinsic WIDTH in this row; a plain `Text::new` only stretches its cross-axis (height here), leaving width 0 and the label invisible.
        let text = Text::auto(
            move || label(),
            LayoutStyle::new(),
            || TextStyle::new(14.0, ink()),
        )?;
        children.push(box_item(text));
    }
    // The whole row is the control, label included: clicking the word beside a checkbox toggles it, and the
    // ring goes round both — which is also what makes the label the thing a reader names it by.
    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .gap(10.0)
            .align_items(AlignItems::CENTER),
        |_r| RectStyle::default(),
        children,
    )?
    .control(role)
    .toggled(toggled)
    .on_press(on_press);
    Ok(box_item(row))
}
