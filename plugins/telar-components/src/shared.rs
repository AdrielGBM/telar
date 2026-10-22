//! What every catalogue component is drawn from: the theme's roles read for the thing being drawn, scaled by the control size in force, plus the scaffolds several components share.
//!
//! **The rule.** A component takes every length, corner, border, wash and weight from a function here (or from its own `XTokens`, which default to one of these), never from a literal or from a primitive token scaled by a factor of its own. Two components that draw the same kind of thing then cannot disagree, and a theme or a control size moves them together. The subset an application needs to draw controls of its own that sit beside the catalogue's is public as [`crate::metrics`].

use telar::{
    Accessible, AlignItems, Border, BorderRadius, Chord, Color, Container, LayoutError, LayoutItem,
    LayoutStyle, Reactive, Rect, RectStyle, StyledContainer, Text, TextStyle, box_item, children,
    focus::Role, style_follows, use_theme_tokens,
};

pub use telar::DOUBLE_CLICK;

/// The ink the document starts in — the root of the cascade, not the text at any particular node.
///
/// Only for choosing *between* candidate inks, where what is wanted is one of the theme's two extremes rather than the colour in force somewhere. To paint text, take what was inherited: this reads past the cascade, and a component that writes it puts the theme's ink into a region that had declared its own.
fn base_ink() -> Color {
    telar::Inherited::initial().text.color.solid_color()
}
/// The WCAG AA contrast ratio for body text.
const BODY_TEXT_CONTRAST: f32 = 4.5;

/// Readable ink for a label sitting on `fill`: whichever of the document's own ink and the theme's `on_primary` has the higher WCAG contrast ratio against it. A mid-tone fill neither of them reads on at 4.5:1 takes black or white instead, whichever reads better.
///
/// A hard-coded white was right only while a filled control was assumed to carry a saturated accent. A neutral palette — the greys a shadcn-style theme builds on, say — makes `primary` a near-white in dark mode, and the label disappeared into its own button. Reading both ends of the theme and picking by contrast ratio keeps a caller free to pass any colour at all.
pub fn ink_on(fill: Color) -> Color {
    let ink = fill.most_readable(&[use_theme_tokens().on_primary(), base_ink()]);
    if fill.contrast_ratio(ink) >= BODY_TEXT_CONTRAST {
        ink
    } else {
        fill.most_readable(&[ink, Color::BLACK, Color::WHITE])
    }
}

/// `color` taken toward the ink that reads on `background` just far enough to read on it at 4.5:1, so a label keeps its hue where it can: the text of a soft button, a tinted badge.
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

/// WCAG's non-text contrast: what a part of a control needs against what it sits on to be told apart from it.
const NON_TEXT_CONTRAST: f32 = 3.0;

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
/// The scrim laid over the page behind a modal, a drawer or the command palette.
#[cfg(feature = "overlays")]
pub(crate) fn overlay() -> Color {
    use_theme_tokens().overlay()
}

/// Corner of a control a pointer presses or types into: a button, a field, a trigger, a tab, an icon button, a stepper. Not scaled by the control size: a smaller control is smaller, not flatter.
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
/// Fully rounded ends for a box `short_side` across: a chip, a badge, a radio, a toggle's track, a slider's thumb.
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
#[cfg(feature = "advanced")]
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

/// Weight of text that stands out from the body around it: a title, a heading, a matched run.
pub fn strong_weight() -> u16 {
    use_theme_tokens().font_weight_strong()
}

/// Base spacing unit from the theme, scaled by the control size in force — which is the whole of how a control gets smaller: the unit underneath its proportions moves. See `theme-core`'s `density` module for why that is one number rather than a size matrix per component.
pub fn spacing() -> f32 {
    use_theme_tokens().spacing() * telar::control_scale()
}
/// The smallest step: between a glyph and the mark beside it.
pub fn gap_xs() -> f32 {
    gap_sm() / 2.0
}
/// The theme's spacing steps at the control size in force: `sm` between the parts of one control, `md` between controls, `lg` around a group, `xl` between sections.
pub fn gap_sm() -> f32 {
    use_theme_tokens().spacing_sm() * telar::control_scale()
}
pub fn gap_md() -> f32 {
    use_theme_tokens().spacing_md() * telar::control_scale()
}
pub fn gap_lg() -> f32 {
    use_theme_tokens().spacing_lg() * telar::control_scale()
}
pub fn gap_xl() -> f32 {
    use_theme_tokens().spacing_xl() * telar::control_scale()
}

/// The height every single-line control is drawn at — a button, a field, a select or menu trigger, a tab, a stepper, a scrub field — so any of them stand in one row edge to edge: an icon with its padding, at the control size in force, never below the smallest pointer target, and in whole pixels so every control snaps to the same edge. A control centres its line inside it rather than padding it vertically.
pub fn control_height() -> f32 {
    (icon_size() + icon_pad() * 2.0).max(MIN_TARGET).round()
}
/// The space between a single-line control's edge and its label or value.
pub fn control_pad_x() -> f32 {
    gap_lg()
}
/// The padding around an icon that is the whole face of a control, so every such control is the same square.
pub fn icon_pad() -> f32 {
    spacing() * 0.75
}
/// The theme's standalone icon size at the control size in force.
pub fn icon_size() -> f32 {
    use_theme_tokens().icon_size() * telar::control_scale()
}
/// The height of a row in a list, a menu or a tree: a control's height and the step that separates one row's wash from the next, in whole pixels like [`control_height`] so every list's rows snap to the same edges.
pub fn row_height() -> f32 {
    (control_height() + gap_sm()).round()
}
/// The padding between a floating panel's edge and the rows inside it.
pub fn panel_pad() -> f32 {
    gap_sm()
}
/// The space between a panel that drops from a control and the control: a menu's list under its button, a select's options under its field.
pub fn popup_offset() -> f32 {
    gap_md()
}

/// A caption's share of the text it labels.
pub(crate) const CAPTION_RATIO: f32 = 0.85;
/// What is left of a line's ink once it is only supporting the line above it: the least that still reads at 4.5:1 on the tinted rows and key caps the catalogue paints behind such a line.
pub(crate) const QUIET_ALPHA: f32 = 0.7;

/// The text a control draws: whatever the tree above it says, at the control size in force, at this control's own ratio to the body size around it.
///
/// The ratio is how a component says "a caption" or "a close glyph" — a proportion of the text beside it rather than a number of its own, so a field's label and a plain label next to it cannot be two sizes.
pub fn control_text(inherited: TextStyle, ratio: f32) -> TextStyle {
    let size = inherited.font_size * telar::control_scale() * ratio;
    inherited.with_font_size(size)
}

/// `ems` of the document's text at the control size in force: the default width of every catalogue box that is not sized by what it holds — a field, a slider, a trigger and its list, a tooltip, a dialog, a notice — so each keeps as many characters to a line whatever size its theme sets the text at. In whole pixels, the edge layout snaps it to.
pub fn measure(ems: f32) -> f32 {
    (telar::Inherited::initial().text.font_size * telar::control_scale() * ems).round()
}

/// A width a component's tokens may name: `token` at the regular control size, scaled with the control size in force and in whole pixels, else [`measure`]`(ems)`.
pub(crate) fn width_or_measure(token: Option<f32>, ems: f32) -> f32 {
    token.map_or_else(
        || measure(ems),
        |width| (width * telar::control_scale()).round(),
    )
}

/// [`control_text`] in a fainter shade of the ink around it: a hint, a group heading, an inactive tab.
///
/// Fades what the tree above declared instead of naming `ink()`, because `ink()` is the very value the cascade seeds itself with — writing it can only repeat the root or overrule a region that said otherwise. Fading also keeps a colour that arrived already soft from being pushed back up to [`QUIET_ALPHA`].
pub(crate) fn quiet(inherited: TextStyle, ratio: f32) -> TextStyle {
    let text = control_text(inherited, ratio);
    let ink = text.color.faded(QUIET_ALPHA);
    text.with_color(ink)
}

/// [`control_text`]'s size on its own, for the box a control sizes to hold that text.
///
/// Takes the node rather than the style because a box is sized outside the closure that styles its text — and reading the context here is what makes the box follow a declaration the text is already following.
pub(crate) fn control_text_size(node: telar::NodeId, ratio: f32) -> f32 {
    control_text(telar::inherited_text_style(node), ratio).font_size
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
#[cfg(feature = "overlays")]
pub(crate) fn resolve_open(
    open: Option<telar::RwSignal<bool>>,
    id: &'static str,
) -> Option<telar::RwSignal<bool>> {
    open.or_else(|| (!id.is_empty()).then(|| telar::overlay_state(id)))
}

fn caption_box(text_size: f32) -> LayoutStyle {
    LayoutStyle::new().height(telar::single_line_box(text_size))
}

fn caption_column(width: f32) -> LayoutStyle {
    LayoutStyle::column().gap(spacing() * 0.75).width(width)
}

/// Stacks an optional small caption above `control`, or hands `control` back untouched when the label is empty. The counterpart of [`labelled_control`], which puts the label *beside* the control instead.
pub(crate) fn captioned(
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
        |t| control_text(t, CAPTION_RATIO).with_color(muted()),
    )?;
    // The caption is a leaf, so its style is followed from the column that outlives it and owns the effect.
    let caption_node = caption.layout_node();
    let col = Container::new(caption_column(width.get()), children![caption, control])?
        .styled_by(move || caption_column(width.get()));
    style_follows(caption_node, move || {
        caption_box(control_text_size(caption_node, CAPTION_RATIO))
    });
    Ok(box_item(col))
}

/// A control height tall whatever the box drawn in it, so a checkbox stands level with a button or a field beside it and is never a smaller target than one.
fn labelled_row() -> LayoutStyle {
    LayoutStyle::row()
        .gap(gap_md())
        .min_height(control_height())
        .align_items(AlignItems::CENTER)
}

/// A control (checkbox box / radio ring / toggle pill) plus an optional label, laid out as one row that is itself the tap target: a tap runs `on_press`. Shared by checkbox, radio and toggle.
pub(crate) fn labelled_control(
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
            |t| control_text(t, 1.0),
        )?;
        children.push(box_item(text));
    }
    // The whole row is the control, label included: clicking the word beside a checkbox toggles it, and the ring goes round both.
    let row = StyledContainer::new(labelled_row(), |_r| RectStyle::default(), children)?
        .styled_by(labelled_row)
        .control(role)
        .toggled(toggled)
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

fn mark_box(shape: Mark) -> LayoutStyle {
    // Whole, even pixels on both sides, so the strokes meet at the same point wherever the box lands on the pixel grid.
    let whole = |ratio: f32| (icon_size() * ratio / 2.0).round() * 2.0;
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

/// A mark drawn rather than spelled, since a face cannot be relied on to carry a caret, a tick or a cross at the size and weight of the label beside it. Strokes are sized from the icon size in force. `ink` picks its colour from the text the tree above it declared, so it follows a region that declared its own. Hidden from readers: the control it sits in has the name.
pub fn mark(
    shape: impl Fn() -> Mark + Clone + 'static,
    ink: impl Fn(&TextStyle) -> telar::Paint + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let current = shape.clone();
    let canvas = telar::Canvas::declaring(mark_box(shape()), move |rect, text| {
        telar::RenderNode::path(
            std::sync::Arc::new(mark_path(current(), rect)),
            telar::PathStyle::default()
                .with_stroke(telar::Stroke::new(ink(&text), border_width_strong())),
        )
    })?
    .a11y_hidden();
    style_follows(canvas.layout_node(), move || mark_box(shape()));
    Ok(box_item(canvas))
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

/// `chord` the way the platform writes it, `⇧⌘Z` on a Mac and `Ctrl+Shift+Z` elsewhere: what a hint shows and a reader hears. Text that names no [`Chord`] is shown as written.
pub(crate) fn shown_chord(chord: &str) -> String {
    match Chord::parse(chord) {
        Ok(parsed) => parsed.to_string(),
        Err(_) => chord.trim().to_string(),
    }
}
