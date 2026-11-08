//! What every catalogue component is drawn from: the theme's roles read for the thing being drawn, scaled by the [`ControlSize`] the component is drawn at, plus the scaffolds several components share.
//!
//! **The rule.** A component takes every length, corner, border, wash and weight from a function here (or from its own `XTokens`, which default to one of these), never from a literal or from a primitive token scaled by a factor of its own. Two components that draw the same kind of thing then cannot disagree, and a theme or a size moves them together. The subset an application needs to draw controls of its own that sit beside the catalogue's is public as [`crate::metrics`].

pub(crate) mod dropdown;
pub(crate) mod edit;
pub(crate) mod popup;
pub(crate) mod resize;
pub(crate) mod rows;
pub(crate) mod scrim;
pub(crate) mod selection;
pub(crate) mod type_ahead;

use telar::{
    Accessible, AlignItems, BODY_TEXT_CONTRAST, Border, BorderRadius, Chord, Color, Container,
    LayoutError, LayoutItem, LayoutStyle, NON_TEXT_CONTRAST, Reactive, Rect, RectStyle,
    StyledContainer, Text, TextStyle, box_item, children, focus::Role, style_follows,
    use_theme_tokens,
};

pub use telar::DOUBLE_CLICK;

use telar::ControlSize;

/// The ink the document starts in — the root of the cascade, not the text at any particular node.
///
/// Only for choosing *between* candidate inks, where what is wanted is one of the theme's two extremes rather than the colour in force somewhere. To paint text, take what was inherited: this reads past the cascade, and a component that writes it puts the theme's ink into a region that had declared its own.
fn base_ink() -> Color {
    telar::Inherited::initial().text.color.solid_color()
}
/// Readable ink for a label sitting on `fill`: whichever of the document's own ink and the theme's `on_primary` has the higher WCAG contrast ratio against it. A mid-tone fill neither of them reads on at 4.5:1 takes black or white instead, whichever reads better.
///
/// A hard-coded white was right only while a filled control was assumed to carry a saturated accent. A neutral palette — a theme built on greys, say — makes `primary` a near-white in dark mode, and the label disappeared into its own button. Reading both ends of the theme and picking by contrast ratio keeps a caller free to pass any colour at all.
pub fn ink_on(fill: Color) -> Color {
    let ink = fill.most_readable(&[use_theme_tokens().on_primary(), base_ink()]);
    if fill.contrast_ratio(ink) >= BODY_TEXT_CONTRAST {
        ink
    } else {
        fill.most_readable(&[ink, Color::BLACK, Color::WHITE])
    }
}

/// `color` taken toward the ink that reads on `background` just far enough to read on it at 4.5:1, so a label keeps its hue where it can: the text of a soft button, a soft or outline tag.
pub fn readable_on(color: Color, background: Color) -> Color {
    let ink = ink_on(background);
    (0..=READABLE_STEPS)
        .map(|step| color.mix(ink, step as f32 / READABLE_STEPS as f32))
        .find(|candidate| background.contrast_ratio(*candidate) >= BODY_TEXT_CONTRAST)
        .unwrap_or(ink)
}
const READABLE_STEPS: u8 = 10;

/// `top` laid over the opaque `bottom`: what a translucent fill looks like on the surface under it, which is what its label has to read against.
pub(crate) fn over(top: Color, bottom: Color) -> Color {
    let blend = |t: f32, b: f32| t * top.a + b * (1.0 - top.a);
    Color {
        r: blend(top.r, bottom.r),
        g: blend(top.g, bottom.g),
        b: blend(top.b, bottom.b),
        a: bottom.a,
    }
}

/// What a knob, a thumb or the ring round one is drawn in over `fill`: the theme's `thumb` wherever it stands out from `fill`, else the ink that reads on `fill`, so a light primary or a light track never swallows the knob. A toggle's knob, a slider's thumb, a handle and a colour picker's thumbs all take it.
pub fn thumb_on(fill: Color) -> Color {
    let thumb = use_theme_tokens().thumb();
    if fill.contrast_ratio(thumb) >= NON_TEXT_CONTRAST {
        thumb
    } else {
        ink_on(fill)
    }
}

/// The smallest a pointer target is made on either side, whatever is drawn inside it.
pub const MIN_TARGET: f32 = 24.0;

/// The panel surface a floating thing sits on.
pub(crate) fn surface() -> Color {
    use_theme_tokens().surface()
}
/// The quiet surface behind a key cap, a code block or a field.
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
/// The scrim laid over the page behind a modal, a drawer or the command palette.
pub(crate) fn overlay() -> Color {
    use_theme_tokens().overlay()
}

/// Corner of a control a pointer presses or types into: a button, a field, a trigger, a tab, an icon button, a stepper. Not scaled by a [`ControlSize`]: a smaller control is smaller, not flatter.
pub fn radius_control() -> f32 {
    use_theme_tokens().radius_control()
}
/// Corner of a panel floating over the page: a popup, a menu, a tooltip, a toast, a code block.
pub fn radius_surface() -> f32 {
    use_theme_tokens().radius_surface()
}
/// Corner of a modal, the command palette, a drawer.
pub fn radius_dialog() -> f32 {
    use_theme_tokens().radius_dialog()
}
/// Corner of a row, option or item inside a surface.
pub fn radius_item() -> f32 {
    use_theme_tokens().radius_item()
}
/// Fully rounded ends for a box `short_side` across: a radio, a toggle's track, a slider's thumb.
pub fn pill_radius(short_side: f32) -> f32 {
    short_side / 2.0
}
/// [`pill_radius`] for the box a style closure is handed, so the shape follows the box whatever it is laid out at.
pub fn pill(rect: Rect) -> BorderRadius {
    BorderRadius::all(pill_radius(rect.width.min(rect.height)))
}

/// Width of a hairline: a field's frame, a divider, a panel's edge.
pub fn border_width() -> f32 {
    use_theme_tokens().border_width()
}
/// Width of a frame that is the control's shape rather than a hairline: an outline button, a checkbox's box.
pub fn border_width_strong() -> f32 {
    use_theme_tokens().border_width_strong()
}
/// The theme's focus ring as a frame, for the cursor a list, a tree or a toolbar draws on the item the arrows reached. A focusable box gets the same ring by itself.
pub fn focus_ring() -> Border {
    telar::focus_ring_border()
}
/// The focus ring's thickness, for a cursor drawn as a line rather than a frame: where a dragged item will land.
pub(crate) fn focus_ring_width() -> f32 {
    use_theme_tokens().focus_ring_width()
}

/// The wash a pointer resting on a control or a row lays over it.
pub fn hover_wash() -> Color {
    use_theme_tokens().highlight_low()
}
/// The wash of the selected row, or of a control held open.
pub fn held_wash() -> Color {
    use_theme_tokens().highlight_med()
}
/// The wash of a control while it is pressed.
pub fn pressed_wash() -> Color {
    use_theme_tokens().highlight_high()
}
/// How much a filled surface darkens while a pointer rests on it, and while it is pressed. A fill cannot take a wash over itself, since a box has one fill, so the step is a darkening shared by every filled control.
const FILL_HOVER_STEP: f32 = 0.15;
const FILL_PRESS_STEP: f32 = 0.25;
/// `fill` as a filled control shows it while a pointer rests on it.
pub fn hovered_fill(fill: Color) -> Color {
    fill.darken(FILL_HOVER_STEP)
}
/// `fill` as a filled control shows it while it is pressed.
pub fn pressed_fill(fill: Color) -> Color {
    fill.darken(FILL_PRESS_STEP)
}

/// [`hovered_fill`] for a fill whose label ink was chosen to read on it at rest: stepped away from the ink rather than always darker, so a dark label on a light fill keeps its contrast under the pointer.
pub(crate) fn hovered_fill_under(fill: Color, ink: Color) -> Color {
    away_from(fill, ink, FILL_HOVER_STEP)
}
/// [`pressed_fill`] for a fill whose label ink was chosen to read on it at rest, as [`hovered_fill_under`] is.
pub(crate) fn pressed_fill_under(fill: Color, ink: Color) -> Color {
    away_from(fill, ink, FILL_PRESS_STEP)
}
fn away_from(fill: Color, ink: Color, step: f32) -> Color {
    match ink.relative_luminance() < fill.relative_luminance() {
        true => fill.mix(Color::WHITE, step),
        false => fill.darken(step),
    }
}

/// Weight of text that stands out from the body around it: a title, a heading, a matched run.
pub fn strong_weight() -> u16 {
    use_theme_tokens().font_weight_strong()
}

/// Base spacing unit from the theme at `size`, which is the whole of how a component gets smaller or larger: the unit underneath its proportions moves.
pub fn spacing(size: ControlSize) -> f32 {
    use_theme_tokens().spacing() * size.scale()
}
/// The smallest step: between a glyph and the mark beside it.
pub fn gap_xs(size: ControlSize) -> f32 {
    gap_sm(size) / 2.0
}
/// The theme's spacing steps at `size`: `sm` between the parts of one control, `md` between controls, `lg` around a group, `xl` between sections.
pub fn gap_sm(size: ControlSize) -> f32 {
    use_theme_tokens().spacing_sm() * size.scale()
}
pub fn gap_md(size: ControlSize) -> f32 {
    use_theme_tokens().spacing_md() * size.scale()
}
pub fn gap_lg(size: ControlSize) -> f32 {
    use_theme_tokens().spacing_lg() * size.scale()
}
pub fn gap_xl(size: ControlSize) -> f32 {
    use_theme_tokens().spacing_xl() * size.scale()
}

/// The height every single-line control of `size` is drawn at — a button, a field, a select or menu trigger, a tab, a stepper, a scrub field — so any of them stand in one row edge to edge: an icon with its padding, in whole pixels so every control snaps to the same edge, and never below [`MIN_TARGET`]. A control centres its line inside it rather than padding it vertically.
pub fn control_height(size: ControlSize) -> f32 {
    (icon_size(size) + icon_pad(size) * 2.0)
        .max(MIN_TARGET)
        .round()
}
/// How far past each edge something drawn `drawn` across has to be found by the pointer for its target to reach [`MIN_TARGET`]: what a mark or an icon a pointer operates on its own needs, since every control is already at least that large.
pub fn target_outset(drawn: f32) -> f32 {
    ((MIN_TARGET - drawn) / 2.0).max(0.0)
}
/// [`target_outset`] past each edge of a box laid out at `laid`, on whichever side it is drawn smaller than [`MIN_TARGET`]: the outset of a control whose drawn size depends on what it holds, such as a checkbox with no label.
pub(crate) fn target_reach(laid: Rect) -> telar::Insets {
    let (across, down) = (target_outset(laid.width), target_outset(laid.height));
    telar::Insets::new(down, across, down, across)
}
/// The space between a single-line control's edge and its label or value.
pub fn control_pad_x(size: ControlSize) -> f32 {
    gap_lg(size)
}
/// The padding around an icon that is the whole face of a control, so every such control of one size is the same square, [`control_height`] across.
pub fn icon_pad(size: ControlSize) -> f32 {
    spacing(size)
}
/// The theme's standalone icon size at `size`.
pub fn icon_size(size: ControlSize) -> f32 {
    use_theme_tokens().icon_size() * size.scale()
}
/// The height of a row in a list, a menu or a tree: a control's height and the step that separates one row's wash from the next, in whole pixels like [`control_height`] so every list's rows snap to the same edges.
pub fn row_height(size: ControlSize) -> f32 {
    (control_height(size) + gap_sm(size)).round()
}
/// The padding between a floating panel's edge and the rows inside it.
pub fn panel_pad(size: ControlSize) -> f32 {
    gap_sm(size)
}
/// The space between a panel that drops from a control and the control: a menu's list under its button, a select's options under its field.
pub fn popup_offset(size: ControlSize) -> f32 {
    gap_md(size)
}

/// A caption's share of the text it labels.
pub(crate) const CAPTION_RATIO: f32 = 0.85;
/// What is left of a line's ink once it is only supporting the line above it: the least that still reads at 4.5:1 on the tinted rows and key caps the catalogue paints behind such a line.
pub(crate) const QUIET_ALPHA: f32 = 0.7;

/// The text a control of `size` draws: whatever the tree above it says, at this control's own ratio to the body size around it.
///
/// The ratio is how a component says "a caption" or "a close glyph" — a proportion of the text beside it rather than a number of its own, so a field's label and a plain label next to it cannot be two sizes.
pub fn control_text(size: ControlSize, inherited: TextStyle, ratio: f32) -> TextStyle {
    let font_size = inherited.font_size * size.text_scale() * ratio;
    inherited.with_font_size(font_size)
}

/// `ems` of the document's text at `size`: the default width of every catalogue box that is not sized by what it holds — a field, a slider, a trigger and its list, a tooltip, a dialog, a notice — so each keeps as many characters to a line whatever size its theme sets the text at. In whole pixels, the edge layout snaps it to.
pub fn measure(size: ControlSize, ems: f32) -> f32 {
    (telar::Inherited::initial().text.font_size * size.text_scale() * ems).round()
}

/// A width a component's tokens may name: `token` at the regular size, scaled with the text to `size` and in whole pixels, else [`measure`]`(size, ems)`.
pub(crate) fn width_or_measure(size: ControlSize, token: Option<f32>, ems: f32) -> f32 {
    token.map_or_else(
        || measure(size, ems),
        |width| (width * size.text_scale()).round(),
    )
}

/// [`control_text`] in a fainter shade of the ink around it: a hint, a group heading, an inactive tab.
///
/// Fades what the tree above declared instead of naming `ink()`, because `ink()` is the very value the cascade seeds itself with — writing it can only repeat the root or overrule a region that said otherwise. Fading also keeps a colour that arrived already soft from being pushed back up to [`QUIET_ALPHA`].
pub(crate) fn quiet(size: ControlSize, inherited: TextStyle, ratio: f32) -> TextStyle {
    let text = control_text(size, inherited, ratio);
    let ink = text.color.faded(QUIET_ALPHA);
    text.with_color(ink)
}

/// [`control_text`]'s size on its own, for the box a control sizes to hold that text.
///
/// Takes the node rather than the style because a box is sized outside the closure that styles its text — and reading the context here is what makes the box follow a declaration the text is already following.
pub(crate) fn control_text_size(size: ControlSize, node: telar::NodeId, ratio: f32) -> f32 {
    control_text(size, telar::inherited_text_style(node), ratio).font_size
}

/// Keeps `node` in layout only while `visible` holds, toggling its display rather than replacing its style, so a text leaf keeps the stretch it is optically centred by.
pub(crate) fn displayed_while(
    node: telar::NodeId,
    visible: impl Fn() -> bool + 'static,
) -> telar::Effect {
    telar::effect(move || {
        telar::set_display(node, visible());
        let _ = telar::mark_dirty(node);
    })
}

/// Resolve a reactive colour: `color()` unless it is `Color::TRANSPARENT` (the "unset" sentinel), else `fallback()`. `color()` is evaluated once. Collapses the per-widget accent/track/surface/bubble resolvers into one shape.
pub(crate) fn resolve(color: &Reactive<Color>, fallback: impl FnOnce() -> Color) -> Color {
    let c = color.get();
    if c == Color::TRANSPARENT {
        fallback()
    } else {
        c
    }
}

/// The open/close state a scrim overlay is driven by: an explicitly bound `open` signal, else the shared state of the `id` it is named by, else `None` (unbound — it can never open).
///
/// An explicit signal wins on purpose. Given both, the two would be independent states racing each other, and the one written next to the widget is the one the author most likely meant.
pub(crate) fn resolve_open(
    open: Option<telar::RwSignal<bool>>,
    id: &'static str,
) -> Option<telar::RwSignal<bool>> {
    open.or_else(|| (!id.is_empty()).then(|| telar::overlay_state(id)))
}

fn caption_box(text_size: f32) -> LayoutStyle {
    LayoutStyle::new().height(telar::single_line_box(text_size))
}

fn caption_column(size: ControlSize, width: f32) -> LayoutStyle {
    LayoutStyle::column().gap(spacing(size) * 0.75).width(width)
}

/// Stacks an optional small caption, at `size`, above `control`, or hands `control` back untouched when the label is empty. The counterpart of [`labelled_control`], which puts the label *beside* the control instead.
pub(crate) fn captioned(
    size: ControlSize,
    control: Box<dyn LayoutItem>,
    label: Reactive<String>,
    width: impl Into<Reactive<f32>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    if label.get().is_empty() {
        return Ok(box_item(control));
    }
    let width = width.into();
    let caption = Text::declaring(
        move || label.get(),
        LayoutStyle::new(),
        move |t| control_text(size, t, CAPTION_RATIO).with_color(muted()),
    )?;
    // The caption is a leaf, so its style is followed from the column that outlives it and owns the effect.
    let caption_node = caption.layout_node();
    let col = Container::new(
        caption_column(size, width.get()),
        children![caption, control],
    )?
    .styled_by(move || caption_column(size, width.get()));
    style_follows(caption_node, move || {
        caption_box(control_text_size(size, caption_node, CAPTION_RATIO))
    });
    Ok(box_item(col))
}

/// A control height of `size` tall whatever the box drawn in it, so a checkbox stands level with a button or a field of its size beside it.
fn labelled_row(size: ControlSize) -> LayoutStyle {
    LayoutStyle::row()
        .gap(gap_md(size))
        .min_height(control_height(size))
        .align_items(AlignItems::CENTER)
}

/// A control (checkbox box / radio ring / toggle pill) plus an optional label at `size`, laid out as one row that is itself the tap target: a tap runs `on_press`. Shared by checkbox, radio and toggle.
pub(crate) fn labelled_control(
    size: ControlSize,
    control: Box<dyn LayoutItem>,
    label: Reactive<String>,
    role: Role,
    toggled: impl Fn() -> bool + 'static,
    on_press: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut children: Vec<Box<dyn LayoutItem>> = children![control];
    if !label.get().is_empty() {
        let text = Text::declaring(
            move || label.get(),
            LayoutStyle::new(),
            move |t| control_text(size, t, 1.0),
        )?;
        children.push(box_item(text));
    }
    // The whole row is the control, label included: clicking the word beside a checkbox toggles it, and the ring goes round both.
    let row = StyledContainer::new(labelled_row(size), |_r| RectStyle::default(), children)?;
    let laid = telar::track_layout(row.layout_node()).expect("a row is registered");
    let row = row
        .styled_by(move || labelled_row(size))
        .control(role)
        .toggled(toggled)
        .hit_outset(Reactive::of(move || target_reach(laid.peek())))
        .on_press(on_press);
    Ok(box_item(row))
}

/// Makes `backdrop` close what it holds when it is pressed, around it rather than on it. That press is the pointer's way out, as Escape is the keyboard's, so the backdrop stays a box: a box that answers a press is otherwise a button, and a button wrapped around the dialog or the menu it holds is one a reader is offered and cannot make sense of.
pub(crate) fn dismissed_by_press(
    backdrop: StyledContainer,
    dismiss: impl Fn() + 'static,
) -> StyledContainer {
    backdrop.role(Role::Group).on_press(dismiss)
}

/// A drawn mark: what a control wears where a text glyph would depend on a face carrying it. `ChevronForward` is toward the end of the line and `ChevronBack` toward its start, so both turn round in a right-to-left region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    ChevronDown,
    ChevronUp,
    ChevronForward,
    ChevronBack,
    Close,
    Check,
    Minimize,
    Maximize,
    Restore,
    Ring,
    /// A magnifying glass: a search field's.
    Search,
    /// An arrow leaving its box up and toward the end of the line: a link that leaves the application. Turns round in a right-to-left region.
    External,
}

/// A chevron's long side as a share of the icon size: wide enough to read beside a label at the body size, narrow enough not to compete with it.
const CHEVRON_RATIO: f32 = 0.7;

/// The side of the square the other marks fill, as a share of the icon size: smaller than a chevron's span, since a cross or a box is dense where a chevron is two strokes.
const SQUARE_RATIO: f32 = 0.6;

/// How far the square behind a restore mark stands up and across from the one in front, as a share of the mark's side.
const RESTORE_OFFSET: f32 = 0.3;

/// Where a check's ends and elbow sit in its box, as shares of its width and height.
/// The distance of a cubic handle from its end, as a share of the radius, that best fits a quarter circle.
const CIRCLE_HANDLE: f32 = 0.552_284_8;

/// A magnifying glass's lens, as a share of the mark's side; the handle runs from the lens's edge to the far corner.
const SEARCH_LENS: f32 = 0.7;

/// Where a circle's edge crosses its diagonal toward the far corner, as a share of its diameter: the middle plus the radius times one over the square root of two.
const CIRCLE_EDGE: f32 = 0.5 + std::f32::consts::FRAC_1_SQRT_2 / 2.0;

/// How far an arrowhead's two strokes run back from its point, as a share of the mark's side.
const EXTERNAL_HEAD: f32 = 0.6;

const CHECK_POINTS: [(f32, f32); 3] = [(0.0, 0.55), (0.38, 0.95), (1.0, 0.1)];

fn mark_box(size: ControlSize, shape: Mark) -> LayoutStyle {
    // Whole, even pixels on both sides, so the strokes meet at the same point wherever the box lands on the pixel grid.
    let whole = |ratio: f32| (icon_size(size) * ratio / 2.0).round() * 2.0;
    let (width, height) = match shape {
        Mark::ChevronDown | Mark::ChevronUp => (whole(CHEVRON_RATIO), whole(CHEVRON_RATIO) / 2.0),
        Mark::ChevronForward | Mark::ChevronBack => {
            (whole(CHEVRON_RATIO) / 2.0, whole(CHEVRON_RATIO))
        }
        Mark::Close
        | Mark::Check
        | Mark::Minimize
        | Mark::Maximize
        | Mark::Restore
        | Mark::Ring
        | Mark::Search
        | Mark::External => (whole(SQUARE_RATIO), whole(SQUARE_RATIO)),
    };
    LayoutStyle::new()
        .width(width)
        .height(height)
        .flex_shrink(0.0)
}

fn mark_path(shape: Mark, rect: Rect) -> telar::PathData {
    let (w, h) = (rect.width, rect.height);
    let at = |(x, y): (f32, f32)| telar::Point::new(x, y);
    let stroke = |points: &[(f32, f32)]| {
        let mut path = telar::PathData::new().move_to(at(points[0]));
        for point in &points[1..] {
            path = path.line_to(at(*point));
        }
        path
    };
    match (shape, telar::current_direction()) {
        (Mark::ChevronDown, _) => stroke(&[(0.0, 0.0), (w / 2.0, h), (w, 0.0)]),
        (Mark::ChevronUp, _) => stroke(&[(0.0, h), (w / 2.0, 0.0), (w, h)]),
        (Mark::ChevronForward, telar::Direction::Rtl)
        | (Mark::ChevronBack, telar::Direction::Ltr) => stroke(&[(w, 0.0), (0.0, h / 2.0), (w, h)]),
        (Mark::ChevronForward | Mark::ChevronBack, _) => {
            stroke(&[(0.0, 0.0), (w, h / 2.0), (0.0, h)])
        }
        (Mark::Close, _) => stroke(&[(0.0, 0.0), (w, h)])
            .move_to(at((w, 0.0)))
            .line_to(at((0.0, h))),
        (Mark::Check, _) => stroke(&CHECK_POINTS.map(|(x, y)| (x * w, y * h))),
        (Mark::Minimize, _) => stroke(&[(0.0, h / 2.0), (w, h / 2.0)]),
        (Mark::Maximize, _) => stroke(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]).close(),
        (Mark::Restore, _) => {
            let offset = (w * RESTORE_OFFSET).round();
            stroke(&[
                (0.0, offset),
                (w - offset, offset),
                (w - offset, h),
                (0.0, h),
            ])
            .close()
            .move_to(at((offset, offset)))
            .line_to(at((offset, 0.0)))
            .line_to(at((w, 0.0)))
            .line_to(at((w, h - offset)))
            .line_to(at((w - offset, h - offset)))
        }
        (Mark::Ring, _) => ring(rect),
        (Mark::Search, _) => {
            let lens = (w * SEARCH_LENS).round();
            let reach = lens * CIRCLE_EDGE;
            ring(Rect::new(0.0, 0.0, lens, lens))
                .move_to(at((reach, reach)))
                .line_to(at((w, h)))
        }
        (Mark::External, telar::Direction::Rtl) => {
            let (head_x, head_y) = (w * EXTERNAL_HEAD, h * EXTERNAL_HEAD);
            stroke(&[(head_x, 0.0), (0.0, 0.0), (0.0, head_y)])
                .move_to(at((0.0, 0.0)))
                .line_to(at((w, h)))
        }
        (Mark::External, _) => {
            let (head_x, head_y) = (w * EXTERNAL_HEAD, h * EXTERNAL_HEAD);
            stroke(&[(w - head_x, 0.0), (w, 0.0), (w, head_y)])
                .move_to(at((w, 0.0)))
                .line_to(at((0.0, h)))
        }
    }
}

/// A circle inscribed in `rect`, from four cubic curves.
fn ring(rect: Rect) -> telar::PathData {
    let (rx, ry) = (rect.width / 2.0, rect.height / 2.0);
    let (kx, ky) = (rx * CIRCLE_HANDLE, ry * CIRCLE_HANDLE);
    let at = |x: f32, y: f32| telar::Point::new(x, y);
    telar::PathData::new()
        .move_to(at(rx * 2.0, ry))
        .cubic_to(
            at(rx * 2.0, ry + ky),
            at(rx + kx, ry * 2.0),
            at(rx, ry * 2.0),
        )
        .cubic_to(at(rx - kx, ry * 2.0), at(0.0, ry + ky), at(0.0, ry))
        .cubic_to(at(0.0, ry - ky), at(rx - kx, 0.0), at(rx, 0.0))
        .cubic_to(at(rx + kx, 0.0), at(rx * 2.0, ry - ky), at(rx * 2.0, ry))
        .close()
}

/// A mark drawn rather than spelled, since a face cannot be relied on to carry a caret, a tick or a cross at the size and weight of the label beside it. Strokes are sized from the icon size at `size`. `ink` picks its colour from the text the tree above it declared, so it follows a region that declared its own. Hidden from readers: the control it sits in has the name.
pub fn mark(
    size: ControlSize,
    shape: impl Fn() -> Mark + Clone + 'static,
    ink: impl Fn(&TextStyle) -> telar::Paint + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let current = shape.clone();
    let canvas = telar::Canvas::declaring(mark_box(size, shape()), move |rect, text| {
        telar::RenderNode::path(
            std::sync::Arc::new(mark_path(current(), rect)),
            telar::PathStyle::default()
                .with_stroke(telar::Stroke::new(ink(&text), border_width_strong())),
        )
    })?
    .a11y_hidden();
    style_follows(canvas.layout_node(), move || mark_box(size, shape()));
    Ok(box_item(canvas))
}

/// What [`glyph`] draws.
#[derive(telar::Props)]
pub(crate) struct GlyphProps {
    pub shape: Mark,
    #[props(default)]
    pub size: ControlSize,
}

/// [`mark`] as a component a `.rsx` view places, at its `size`, in the ink of the text around it.
pub(crate) fn glyph(props: GlyphProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let GlyphProps { shape, size } = props;
    mark(size, move || shape, |text| text.color)
}

/// What `build` makes while `show` reads true, and nothing at all while it reads false: a mark only some states draw, a button only a filled field needs. Laid out in a box of `style` that is empty and takes no room while there is nothing in it, so whatever spaces it from its neighbours belongs in what `build` makes.
pub(crate) fn shown_while(
    show: impl Fn() -> bool + 'static,
    style: LayoutStyle,
    build: impl Fn() -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let list = telar::ReactiveList::with_style(
        style.flex_shrink(0.0),
        move || if show() { vec![()] } else { Vec::new() },
        |()| (),
        move |()| build(),
    )?;
    Ok(box_item(list))
}

/// What `build` makes, under a tooltip of `size` showing `text` and `shortcut` (empty for none): a control that is only a picture, or one whose binding is worth showing. A recipe because the tooltip builds its trigger where it places it, and again whenever it places it again, so whatever `build` draws from has to outlive one build: a `#[derive(Clone)]` struct of the parts with a `build(&self)`, moved in as `move || parts.build()`.
pub(crate) fn hinted(
    build: impl Fn() -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    size: ControlSize,
    text: Reactive<String>,
    shortcut: Reactive<String>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    crate::tooltip(
        crate::TooltipProps::props()
            .size(size)
            .text(text)
            .shortcut(shortcut)
            .children(telar::Children::one(build))
            .build(),
    )
}

/// `chord` the way the platform writes it, `⇧⌘Z` on a Mac and `Ctrl+Shift+Z` elsewhere, a sequence step after step (`Ctrl+K, then S`): what a hint shows and a reader hears. Text that names no [`Chord`] is shown as written.
pub(crate) fn shown_chord(chord: &str) -> String {
    match chord_steps(chord) {
        Some(steps) => steps
            .iter()
            .map(ChordStep::to_string)
            .reduce(|first, next| {
                crate::strings::text_with(
                    crate::strings::THEN,
                    &[("first", &first), ("next", &next)],
                )
            })
            .unwrap_or_default(),
        None => chord.trim().to_string(),
    }
}

/// One step of a sequence: a chord, or a step that names none, kept as written.
pub(crate) enum ChordStep {
    Chord(Chord),
    Written(String),
}

impl std::fmt::Display for ChordStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChordStep::Chord(chord) => chord.fmt(f),
            ChordStep::Written(text) => f.write_str(text),
        }
    }
}

/// The steps of a sequence written as editors write one, `"Mod+K S"`: what is pressed together, step after step, each read on its own so one step that names no [`Chord`] is kept as written beside the others. `None` when no step names a chord: text, not a shortcut.
pub(crate) fn chord_steps(text: &str) -> Option<Vec<ChordStep>> {
    let steps: Vec<ChordStep> = step_words(text)
        .into_iter()
        .map(|step| match Chord::parse(&step) {
            Ok(chord) => ChordStep::Chord(chord),
            Err(_) => ChordStep::Written(step),
        })
        .collect();
    steps
        .iter()
        .any(|step| matches!(step, ChordStep::Chord(_)))
        .then_some(steps)
}

/// Whitespace parts the steps of a sequence, except beside a `+`, which joins the keys of one chord however it is spaced: `Ctrl + K` is one step, as is `Ctrl + +`.
fn step_words(text: &str) -> Vec<String> {
    let mut steps: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match steps.last_mut() {
            Some(step)
                if word.starts_with('+') || (step.ends_with('+') && !step.ends_with("++")) =>
            {
                step.push_str(word)
            }
            _ => steps.push(word.to_string()),
        }
    }
    steps
}
