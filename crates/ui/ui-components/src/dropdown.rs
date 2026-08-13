//! Shared dropdown scaffold (constants, style fns, trigger + overlay/backdrop/anchoring) behind `menu` and `select`.

use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{AlignItems, LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke, TextStyle};
use theme_core::use_theme_tokens;
use ui_core::{
    Container, LayoutItem, Overlay, ReactiveList, StyledContainer, Text, box_item, track_layout,
};

use crate::shared;

/// The trigger's fixed size and the dropdown panel's geometry. Kept as constants (not content-sized) so the
/// panel lines up under the trigger and so the widgets' hit geometry is deterministic (their tests compute
/// click points from these).
pub(crate) const PANEL_WIDTH: f32 = 180.0;
/// Gap kept between the panel and the window edge it was pushed off.
const EDGE_MARGIN: f32 = 4.0;
pub(crate) const TRIGGER_HEIGHT: f32 = 36.0;
pub(crate) const ROW_HEIGHT: f32 = 32.0;
pub(crate) fn panel_pad() -> f32 {
    shared::spacing() * 0.5
}

/// The trigger + anchored-panel scaffold shared by `menu` and `select`: a bordered trigger button opens a
/// blocking overlay whose transparent backdrop dismisses on click-away, and whose anchored panel lists `rows`;
/// picking a row optionally writes into `selected`, fires `on_pick`, and closes.
///
/// `trigger_label` is a closure so `select` can track its bound signal reactively while `menu` supplies a
/// static label. `selected` drives the bound-selection behaviour: `Some` writes the picked index back and
/// highlights the selected row; `None` (menu) skips both, leaving one-shot actions.
///
/// `fill` gives the trigger the width its row offers instead of the fixed [`PANEL_WIDTH`], and the panel then
/// opens at whatever width the trigger was laid out to. That is what a form row wants — a control 180px wide
/// beside fields that span the row reads as a mistake — and it is the caller's choice because a dropdown
/// standing on its own has no row to take a width from.
/// Everything the scaffold is told, as one value. It outgrew what a positional argument list can be read at
/// a call site — and the two shape flags are the *caller's* to make rather than the component's.
pub(crate) struct Dropdown {
    pub label: Box<dyn Fn() -> String>,
    pub rows: Vec<&'static str>,
    pub color: Box<dyn Fn() -> Color>,
    pub on_pick: Option<Box<dyn Fn(u32)>>,
    pub selected: Option<RwSignal<u32>>,
    pub fill: bool,
    pub bordered: bool,
    pub caret: bool,
    pub style: Option<Box<dyn Fn(RectStyle) -> RectStyle>>,
}

pub(crate) fn dropdown(props: Dropdown) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Dropdown {
        label: trigger_label,
        rows,
        color,
        on_pick,
        selected,
        fill,
        bordered,
        caret: with_caret,
        style: surface,
    } = props;
    let surface: shared::SurfaceStyle =
        surface.map(|f| -> Rc<dyn Fn(RectStyle) -> RectStyle> { Rc::from(f) });
    // Erased to `Rc` so the colour/callback can be cloned into the trigger, every option row, and the panel
    // builder (which re-runs on each open) — a `Box<dyn Fn>` can't be shared, but the widget needs to.
    let color: shared::ReactiveColor = Rc::from(color);
    let on_pick: Option<Rc<dyn Fn(u32)>> = on_pick.map(|f| -> Rc<dyn Fn(u32)> { Rc::from(f) });
    let open = signal(false);

    // Trigger: a bordered button with the label; a tap toggles `open`.
    // `auto` (measured) so the label has width in this row; `single_line` only sets height → width 0 → invisible.
    // `no_wrap`: a trigger's label is the *name* of a control, and splitting `File` across two lines to make
    // room for the caret beside it turns one word into two.
    let label_text = Text::auto(trigger_label, LayoutStyle::new(), || {
        TextStyle::new(shared::font_size(), shared::ink()).with_no_wrap(true)
    })?;
    let trigger_style = {
        let color = color.clone();
        let surface = surface.clone();
        move |_r: Rect| shared::amend(trigger_rect_style(color.as_ref(), bordered), &surface)
    };
    let toggle = {
        let open = open.clone();
        move || open.update(|o| *o = !*o)
    };
    let trigger_box = LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .height(TRIGGER_HEIGHT)
        .padding_horizontal(12.0)
        .gap(6.0)
        .justify_content(layout_core::JustifyContent::SPACE_BETWEEN);
    let trigger_box = if fill {
        trigger_box.flex_grow(1.0)
    } else {
        trigger_box.width(PANEL_WIDTH)
    };
    // Pushed apart with `SPACE_BETWEEN` and not with a growing spacer: a spacer takes the free space before
    // the label has claimed its own, and `File` came out wrapped to two letters a line.
    let mut trigger_children: Vec<Box<dyn LayoutItem>> = vec![box_item(label_text)];
    if with_caret {
        trigger_children.push(caret()?);
    }
    let trigger =
        StyledContainer::new(trigger_box, trigger_style, trigger_children)?.on_press(toggle);
    let trigger_node = trigger.layout_node();
    // The trigger's laid-out rect positions the panel; `track_layout` returns the signal now (default rect)
    // and layout fills it in, so by the time the panel is built (on open) the trigger's rect is known.
    let trigger_rect = track_layout(trigger_node).expect("trigger node is registered");

    // Open/close is modelled exactly like a reactive `if $open`: a single-item `ReactiveList` keyed on the
    // boolean. Building the overlay lazily (only while open) is what lets it portal correctly — the page has
    // been laid out by then, so the overlay attaches to the viewport host and the trigger's rect is known.
    let overlay_holder = ReactiveList::new(
        {
            let open = open.clone();
            move || vec![open.get()]
        },
        |o: &bool| *o,
        move |is_open: bool| -> Result<Box<dyn LayoutItem>, LayoutError> {
            if !is_open {
                // Closed: an empty placeholder (no overlay registered, so nothing blocks the page).
                return Ok(box_item(Container::new(LayoutStyle::new(), vec![])?));
            }
            let mut row_items: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(rows.len());
            for (i, label) in rows.iter().enumerate() {
                let idx = i as u32;
                let label = *label;
                let row_style = {
                    let selected = selected.clone();
                    let color = color.clone();
                    // Read `selected` at view time (not here in the builder) so re-highlighting a new
                    // selection re-renders the row instead of subscribing the list effect and rebuilding.
                    move |_r: Rect| {
                        let is_selected = selected.as_ref().is_some_and(|s| s.get() == idx);
                        option_row_style(is_selected, color.as_ref())
                    }
                };
                let hover_style = {
                    let color = color.clone();
                    move |_r: Rect| option_row_hover_style(color.as_ref())
                };
                let on_press = {
                    let selected = selected.clone();
                    let on_pick = on_pick.clone();
                    let open = open.clone();
                    move || {
                        if let Some(sel) = &selected {
                            sel.set(idx);
                        }
                        if let Some(cb) = &on_pick {
                            cb(idx);
                        }
                        open.set(false);
                    }
                };
                let text = Text::auto(
                    move || label.to_string(),
                    LayoutStyle::new(),
                    || TextStyle::new(shared::font_size(), shared::ink()),
                )?;
                let row = StyledContainer::new(
                    LayoutStyle::new()
                        .flex_row()
                        .align_items(AlignItems::CENTER)
                        .height(ROW_HEIGHT)
                        .padding_horizontal(10.0),
                    row_style,
                    vec![box_item(text)],
                )?
                .on_hover_style(hover_style)
                .on_press(on_press);
                row_items.push(box_item(row));
            }
            // Place the definite-width panel at the trigger's bottom-left via layout margins; positioning by
            // layout keeps the panel's hit rects in world space so the rows dispatch correctly. The trigger's
            // rect is window-absolute (the overlay hoists to the window), falling back to the local rect.
            let anchor = ui_core::overlay::anchor_rect(trigger_node, &trigger_rect);
            // The anchor is the trigger's rect as it stands when the panel opens: a re-resolve on a theme
            // switch keeps the margins it was built from, which is where the panel belongs either way.
            // A filled trigger widens the panel to match — it opens under a control the row sized, so a
            // narrower list would read as a different control — but only ever *widens* it. Taking the
            // trigger's width outright is what turned a compact `File` button into a 40px sheet with one
            // character per line; the trigger is a floor, never a ceiling. Radix says the same thing with
            // `--radix-dropdown-menu-trigger-width` as a `min-width`.
            let width = if fill {
                anchor.width.max(PANEL_WIDTH)
            } else {
                PANEL_WIDTH
            };
            // And it has to land on screen: a trigger near the right edge would otherwise open a panel that
            // runs past it, and one near the bottom would open below the window.
            let viewport =
                ui_core::overlay_viewport().unwrap_or(Rect::new(0.0, 0.0, f32::MAX, f32::MAX));
            let left = (anchor.x)
                .min(viewport.x + viewport.width - width - EDGE_MARGIN)
                .max(viewport.x);
            let below = anchor.y + anchor.height;
            let sheet = move || {
                LayoutStyle::new()
                    .flex_column()
                    .width(width)
                    .padding_all(panel_pad())
                    .margin_left(left)
                    .margin_top(below)
            };
            let panel = StyledContainer::new(sheet(), |_r| panel_rect_style(), row_items)?
                .styled_by(sheet)
                // Swallow clicks on the panel's own padding so they don't dismiss via the backdrop.
                .on_press(|| {});
            // A transparent full-viewport backdrop inside a BLOCKING overlay: stops hover/clicks bleeding
            // through to the page behind and dismisses the dropdown when a click lands outside the panel.
            let close = {
                let open = open.clone();
                move || open.set(false)
            };
            let backdrop = StyledContainer::new(
                LayoutStyle::new().flex_column().flex_grow(1.0),
                |_r| RectStyle::default(),
                vec![box_item(panel)],
            )?
            .on_press(close);
            let overlay = Overlay::new(LayoutStyle::new().flex_column(), vec![box_item(backdrop)])?;
            Ok(box_item(overlay))
        },
    )?;

    // The widget owns both the trigger and the (portaling) overlay holder; the holder's placeholder takes no
    // space in the column, so only the trigger participates in flow layout.
    // `fill` has to reach the root as well as the trigger: the trigger grows inside *this* box, and this box
    // is what the caller's row lays out — left content-sized it shrinks to the label and the trigger with it.
    let root_box = LayoutStyle::new().flex_column();
    let root_box = if fill {
        root_box.flex_grow(1.0)
    } else {
        root_box
    };
    let root = Container::new(root_box, vec![box_item(trigger), box_item(overlay_holder)])?;
    Ok(box_item(root))
}

/// A select is a form control and wears a border; a **menu** is a button that happens to open a list, and
/// the original gives it `variant="ghost"` for exactly that reason. A bordered menu trigger reads as a field
/// with the caret in it — or, sitting alone in a header, as a button stuck in its focused state.
fn trigger_rect_style(color: &dyn Fn() -> Color, bordered: bool) -> RectStyle {
    let radius = BorderRadius::all(shared::radius_md());
    if !bordered {
        return RectStyle::default().with_radius(radius);
    }
    let accent = shared::resolve(color, || {
        use_theme_tokens()
            .map(|t| t.primary())
            .unwrap_or(shared::DEFAULT_ACCENT)
    });
    RectStyle::default()
        .with_fill(shared::surface())
        .with_stroke(Stroke::new(accent, 1.0))
        .with_radius(radius)
}

/// The caret every trigger carries, drawn rather than spelled: a glyph would depend on the face having it
/// and on it being the size the label is. Two strokes, 11px across, as the original asks for.
fn caret() -> Result<Box<dyn LayoutItem>, LayoutError> {
    const W: f32 = 11.0;
    const H: f32 = 5.5;
    let canvas = ui_core::Canvas::new(LayoutStyle::new().width(W).height(H), |rect| {
        let top = (rect.height - H) / 2.0;
        let data = std::sync::Arc::new(
            renderer_core::PathData::new()
                .move_to(geometry_core::Point::new(0.0, top))
                .line_to(geometry_core::Point::new(W / 2.0, top + H))
                .line_to(geometry_core::Point::new(W, top)),
        );
        ui_core::RenderNode::path(
            data,
            renderer_core::PathStyle::default()
                .with_stroke(Stroke::new(shared::ink().with_alpha(0.6), 1.4)),
        )
    })?;
    Ok(box_item(canvas))
}

fn panel_rect_style() -> RectStyle {
    RectStyle::default()
        .with_fill(shared::surface())
        .with_stroke(Stroke::new(shared::border(), 1.0))
        .with_radius(BorderRadius::all(shared::radius()))
}

// A menu row (`selected: None` → always `is_selected == false`) yields the plain radius-only style; a select
// row highlights the bound choice with a faint accent fill.
fn option_row_style(is_selected: bool, color: &dyn Fn() -> Color) -> RectStyle {
    let radius = BorderRadius::all(shared::radius_sm());
    if is_selected {
        let accent = shared::resolve(color, || {
            use_theme_tokens()
                .map(|t| t.primary())
                .unwrap_or(shared::DEFAULT_ACCENT)
        });
        RectStyle::default()
            .with_fill(accent.with_alpha(0.14))
            .with_radius(radius)
    } else {
        RectStyle::default().with_radius(radius)
    }
}

fn option_row_hover_style(color: &dyn Fn() -> Color) -> RectStyle {
    let accent = shared::resolve(color, || {
        use_theme_tokens()
            .map(|t| t.primary())
            .unwrap_or(shared::DEFAULT_ACCENT)
    });
    RectStyle::default()
        .with_fill(accent.with_alpha(0.10))
        .with_radius(BorderRadius::all(4.0))
}
