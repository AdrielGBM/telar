//! The style a node is laid out by, written in logical edges and resolved to taffy's physical ones.

use taffy::{
    Dimension, Display, FlexDirection, FlexWrap, GridPlacement, LengthPercentage,
    LengthPercentageAuto, Style,
};

pub use taffy::{AlignItems, AvailableSpace, JustifyContent};

use geometry_core::{LayoutGrid, Size};

use crate::direction::Direction;
use crate::track::TemplateTrack;

#[derive(Debug, Clone, Copy, PartialEq)]
/// A width or height: a length in logical pixels, a fraction of the parent, a fraction of the surface, or `auto`.
///
/// The surface variants are fractions like [`Percent`](Self::Percent) (`0.5` is half), taken of the surface the tree is laid out on — the window, the page, the terminal — rather than of the parent. They have no value until the engine knows that surface, so a style holding one is resolved against it at layout time and again whenever it changes; converted directly into a taffy type, one stands in as `auto` (or zero where there is no `auto`).
pub enum SizeDimension {
    Px(f32),
    Percent(f32),
    SurfaceWidth(f32),
    SurfaceHeight(f32),
    /// A fraction of whichever side of the surface is shorter.
    SurfaceMin(f32),
    /// A fraction of whichever side of the surface is longer.
    SurfaceMax(f32),
    Auto,
}

impl SizeDimension {
    /// Whether this length is a fraction of the surface, and so means nothing until the surface's size is known.
    pub fn is_surface_relative(self) -> bool {
        matches!(
            self,
            SizeDimension::SurfaceWidth(_)
                | SizeDimension::SurfaceHeight(_)
                | SizeDimension::SurfaceMin(_)
                | SizeDimension::SurfaceMax(_)
        )
    }

    /// This length in pixels against `surface` if it is a fraction of one, and unchanged otherwise.
    pub fn against(self, surface: Size) -> Self {
        match self {
            SizeDimension::SurfaceWidth(f) => SizeDimension::Px(f * surface.width),
            SizeDimension::SurfaceHeight(f) => SizeDimension::Px(f * surface.height),
            SizeDimension::SurfaceMin(f) => SizeDimension::Px(f * surface.min_side()),
            SizeDimension::SurfaceMax(f) => SizeDimension::Px(f * surface.max_side()),
            other => other,
        }
    }
}

impl From<f32> for SizeDimension {
    fn from(px: f32) -> Self {
        SizeDimension::Px(px)
    }
}

impl From<SizeDimension> for Dimension {
    fn from(d: SizeDimension) -> Self {
        match d {
            SizeDimension::Px(v) => Dimension::length(v),
            SizeDimension::Percent(v) => Dimension::percent(v),
            _ => Dimension::auto(),
        }
    }
}

/// For padding and gap, which resolve against the containing block but have no `auto` to fall back on — CSS has none there either, and taffy's type says so. `Auto` is the caller asking for a size decision on a property that makes none, so it is nothing.
impl From<SizeDimension> for LengthPercentage {
    fn from(d: SizeDimension) -> Self {
        match d {
            SizeDimension::Px(v) => LengthPercentage::length(v),
            SizeDimension::Percent(v) => LengthPercentage::percent(v),
            _ => LengthPercentage::length(0.0),
        }
    }
}

/// Every definite length a style is given, put on the surface's grid before taffy ever sees it.
///
/// Here rather than in the painter because a box has to *be* a whole number of steps, not merely be drawn as one. A backend that quantises rounds each edge of a box independently — the only way two boxes sharing an edge share a cell rather than leaving a seam — which makes the steps a box covers `round((y + h) / step) - round(y / step)`: a function of where it is as well as how big it is. Snap `h` and that collapses to `h / step` for every `y`, so a box keeps its size while it scrolls and two identical boxes agree wherever they landed.
///
/// It also buys symmetry for free. `padding_vertical` sets top and bottom from one value, so snapping that value gives the same cells above and below by construction; it was rounding the two *edges* separately that let one side absorb a cell the other did not.
///
/// Percentages and `auto` pass through untouched: both resolve against a container this cannot see, and a percentage of a snapped parent is the parent's problem, not this one's.
/// The pixels a length resolves to, or zero for a percentage — which cannot be resolved without the containing block, and treating it as no room is the safe half of the guess: the border gets its cell and the box is a shade roomier than it had to be.
fn length_of(v: LengthPercentage) -> f32 {
    let raw = v.into_raw();
    if raw.tag() == taffy::CompactLength::LENGTH_TAG {
        raw.value()
    } else {
        0.0
    }
}

fn snapped(d: SizeDimension, f: impl Fn(LayoutGrid, f32) -> f32) -> SizeDimension {
    match d {
        SizeDimension::Px(v) => {
            let grid = geometry_core::layout_grid();
            if grid.is_unit() {
                d
            } else {
                SizeDimension::Px(f(grid, v))
            }
        }
        other => other,
    }
}

fn size_x(d: impl Into<SizeDimension>) -> SizeDimension {
    snapped(d.into(), |g, v| g.snap_size_x(v))
}
fn size_y(d: impl Into<SizeDimension>) -> SizeDimension {
    snapped(d.into(), |g, v| g.snap_size_y(v))
}
fn space_x(d: impl Into<SizeDimension>) -> SizeDimension {
    snapped(d.into(), |g, v| g.snap_space_x(v))
}
fn space_y(d: impl Into<SizeDimension>) -> SizeDimension {
    snapped(d.into(), |g, v| g.snap_space_y(v))
}
fn pos_x(d: impl Into<SizeDimension>) -> SizeDimension {
    snapped(d.into(), |g, v| g.snap_pos_x(v))
}
fn pos_y(d: impl Into<SizeDimension>) -> SizeDimension {
    snapped(d.into(), |g, v| g.snap_pos_y(v))
}

/// For margin and inset, where `auto` is a real answer: it is what centres a block and what leaves an edge unpinned.
impl From<SizeDimension> for LengthPercentageAuto {
    fn from(d: SizeDimension) -> Self {
        match d {
            SizeDimension::Px(v) => LengthPercentageAuto::length(v),
            SizeDimension::Percent(v) => LengthPercentageAuto::percent(v),
            _ => LengthPercentageAuto::auto(),
        }
    }
}

/// A physical property a surface-relative length can be written into. The logical edges need none: [`LogicalStyle`] already keeps them as [`SizeDimension`]s until resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    FlexBasis,
    PaddingTop,
    PaddingBottom,
    PaddingLeft,
    PaddingRight,
    MarginTop,
    MarginBottom,
    InsetTop,
    InsetBottom,
    GapX,
    GapY,
}

impl Slot {
    /// Puts `d` on the grid the way the builder for this property would have, had it been pixels all along.
    fn snap(self, d: SizeDimension) -> SizeDimension {
        match self {
            Slot::Width | Slot::MinWidth | Slot::MaxWidth => size_x(d),
            Slot::Height | Slot::MinHeight | Slot::MaxHeight => size_y(d),
            Slot::FlexBasis => d,
            Slot::PaddingLeft | Slot::PaddingRight | Slot::GapX => space_x(d),
            Slot::PaddingTop
            | Slot::PaddingBottom
            | Slot::MarginTop
            | Slot::MarginBottom
            | Slot::GapY => space_y(d),
            Slot::InsetTop | Slot::InsetBottom => pos_y(d),
        }
    }

    fn write(self, style: &mut Style, d: SizeDimension) {
        match self {
            Slot::Width => style.size.width = d.into(),
            Slot::Height => style.size.height = d.into(),
            Slot::MinWidth => style.min_size.width = d.into(),
            Slot::MinHeight => style.min_size.height = d.into(),
            Slot::MaxWidth => style.max_size.width = d.into(),
            Slot::MaxHeight => style.max_size.height = d.into(),
            Slot::FlexBasis => style.flex_basis = d.into(),
            Slot::PaddingTop => style.padding.top = d.into(),
            Slot::PaddingBottom => style.padding.bottom = d.into(),
            Slot::PaddingLeft => style.padding.left = d.into(),
            Slot::PaddingRight => style.padding.right = d.into(),
            Slot::MarginTop => style.margin.top = d.into(),
            Slot::MarginBottom => style.margin.bottom = d.into(),
            Slot::InsetTop => style.inset.top = d.into(),
            Slot::InsetBottom => style.inset.bottom = d.into(),
            Slot::GapX => style.gap.width = d.into(),
            Slot::GapY => style.gap.height = d.into(),
        }
    }
}

/// A grid track list, which a fraction of the surface has to be written into again on every resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrackSlot {
    TemplateColumns,
    TemplateRows,
    AutoColumns,
    AutoRows,
}

impl TrackSlot {
    fn is_auto(self) -> bool {
        matches!(self, TrackSlot::AutoColumns | TrackSlot::AutoRows)
    }

    fn write(self, style: &mut Style, tracks: &[TemplateTrack], surface: Size) {
        let template = || {
            tracks
                .iter()
                .map(|track| track.template_component(surface))
                .collect()
        };
        let auto = || {
            tracks
                .iter()
                .map(|track| track.track_sizing_function(surface))
                .collect()
        };
        match self {
            TrackSlot::TemplateColumns => style.grid_template_columns = template(),
            TrackSlot::TemplateRows => style.grid_template_rows = template(),
            TrackSlot::AutoColumns => style.grid_auto_columns = auto(),
            TrackSlot::AutoRows => style.grid_auto_rows = auto(),
        }
    }
}

/// A logical edge's length in pixels against `surface`, snapped as its builder snaps pixels. A length that was never surface-relative was snapped when it was written and is returned untouched.
fn settle(
    d: SizeDimension,
    surface: Size,
    snap: fn(SizeDimension) -> SizeDimension,
) -> SizeDimension {
    if d.is_surface_relative() {
        snap(d.against(surface))
    } else {
        d
    }
}

/// The parts of a style that cannot be turned into physical edges until a [`Direction`] is known. Kept alongside the resolved `taffy::Style` rather than folded into it, so a direction flip can re-resolve the original intent instead of trying to un-swap edges it can no longer tell apart from physical ones.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct LogicalStyle {
    pub(crate) padding_start: Option<SizeDimension>,
    pub(crate) padding_end: Option<SizeDimension>,
    pub(crate) margin_start: Option<SizeDimension>,
    pub(crate) margin_end: Option<SizeDimension>,
    pub(crate) inset_start: Option<SizeDimension>,
    pub(crate) inset_end: Option<SizeDimension>,
    /// Set by [`LayoutStyle::flex_row`]: the main axis is the inline axis, so it reverses under RTL. An explicit [`LayoutStyle::flex_row_reverse`] leaves this clear — it means "reversed" in either direction.
    pub(crate) row_follows_direction: bool,
    /// Set by `LayoutEngine::make_flex_row` for a node whose own declared style never called `flex_row`.
    pub(crate) row_forced: bool,
    /// Set by [`LayoutStyle::shown`] / [`LayoutStyle::display_none`]: what the node's own style says about being in flow. A style is free to say the opposite of what it said last time — which is the whole of `shown:` in a `[view]`, re-resolved from whatever it reads.
    pub(crate) hidden: bool,
    /// Set by `LayoutEngine::set_display`: an answer given *out of band*, which no style knows about and so none may overwrite. `None` leaves the question to [`hidden`](Self::hidden).
    ///
    /// The two used to be one flag that `set_style` OR-ed into whatever came next, so a node hidden once — by either door — could never be shown again by a style. That is the trap this pair exists to close: the out-of-band answer is carried forward because nothing else knows it, and the declared one is replaced because the new style is exactly what does.
    pub(crate) display_override: Option<bool>,
    /// Set by `LayoutEngine::set_min_height`: overrides `inner.min_size.height`.
    pub(crate) min_height_override: Option<f32>,
    /// Set by `LayoutEngine::set_leading_margin`; `(is_row, px)`, placed by the engine since which physical edge is "leading" depends on the parent's axis.
    pub(crate) leading_margin: Option<(bool, f32)>,
    /// Set by [`LayoutStyle::bordered`] on a surface that draws strokes in whole cells: the cell its frame needs, reserved at resolution from the padding the box actually resolved to.
    pub(crate) stroke_cell: Option<LayoutGrid>,
}

impl LogicalStyle {
    /// Whether the node is out of flow: what was set out of band if anything was, and what the style declares otherwise.
    pub(crate) fn is_hidden(&self) -> bool {
        match self.display_override {
            Some(shown) => !shown,
            None => self.hidden,
        }
    }
}

/// A box's four margins, named by axis so they follow the writing direction rather than the screen.
///
/// The nine builders this replaces mixed two vocabularies — seven physical, two logical — and nothing in the name of `margin_left` said which of the two it belonged to.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Margin {
    pub block_start: f32,
    pub block_end: f32,
    pub inline_start: f32,
    pub inline_end: f32,
}

impl Margin {
    pub fn all(px: f32) -> Self {
        Self {
            block_start: px,
            block_end: px,
            inline_start: px,
            inline_end: px,
        }
    }

    pub fn symmetric(block: f32, inline: f32) -> Self {
        Self {
            block_start: block,
            block_end: block,
            inline_start: inline,
            inline_end: inline,
        }
    }
}

#[derive(Clone)]
/// How a node is laid out and placed, written in logical edges and resolved against the active direction.
pub struct LayoutStyle {
    pub(crate) inner: Style,
    pub(crate) logical: LogicalStyle,
    /// The physical properties written as a fraction of the surface, each still holding that fraction; [`inner`](Self::inner) holds a placeholder for them until [`resolve`](Self::resolve) knows the surface.
    surface: Vec<(Slot, SizeDimension)>,
    /// The grid track lists naming a fraction of the surface, kept as written for the same reason as [`surface`](Self::surface).
    tracks: Vec<(TrackSlot, Vec<TemplateTrack>)>,
}

impl LayoutStyle {
    /// A **block** box, as in CSS: children stack vertically and the flex properties do nothing.
    ///
    /// Worth saying out loud because the ones that do nothing do it silently. [`gap`](Self::gap), [`justify_content`](Self::justify_content) and [`align_items`](Self::align_items) all belong to flex layout, so on a box that never called [`flex_row`](Self::flex_row) or [`flex_column`](Self::flex_column) they are accepted and ignored — a row written without `flex_row` comes out as a column, and the reading on screen is not "that row is a column" but "why is this panel twice as tall as it should be".
    pub fn new() -> Self {
        Self {
            inner: Style {
                display: Display::Block,
                ..Style::default()
            },
            logical: LogicalStyle::default(),
            surface: Vec::new(),
            tracks: Vec::new(),
        }
    }

    fn put(&mut self, slot: Slot, d: SizeDimension) {
        let d = slot.snap(d);
        self.surface.retain(|(held, _)| *held != slot);
        if d.is_surface_relative() {
            self.surface.push((slot, d));
        }
        slot.write(&mut self.inner, d);
    }

    /// Sets a definite width, or `auto` when `None`, exactly as given: what the engine fills a root with is the space it was handed, already on the grid.
    pub(crate) fn set_definite_width(&mut self, width: Option<f32>) {
        self.surface.retain(|(held, _)| *held != Slot::Width);
        self.inner.size.width = width.map_or(Dimension::auto(), Dimension::length);
    }

    /// The height counterpart of [`set_definite_width`](Self::set_definite_width).
    pub(crate) fn set_definite_height(&mut self, height: Option<f32>) {
        self.surface.retain(|(held, _)| *held != Slot::Height);
        self.inner.size.height = height.map_or(Dimension::auto(), Dimension::length);
    }

    fn put_tracks(&mut self, slot: TrackSlot, tracks: Vec<TemplateTrack>) {
        if slot.is_auto() {
            tracks.iter().for_each(TemplateTrack::assert_single);
        }
        slot.write(&mut self.inner, &tracks, Size::ZERO);
        self.tracks.retain(|(held, _)| *held != slot);
        if tracks.iter().any(TemplateTrack::is_surface_relative) {
            self.tracks.push((slot, tracks));
        }
    }

    fn surface_slot(&self, slot: Slot) -> Option<SizeDimension> {
        self.surface
            .iter()
            .find_map(|&(held, d)| (held == slot).then_some(d))
    }

    /// Whether anything in this style is a fraction of the surface, and so has to be resolved again when the surface changes size.
    pub fn is_surface_relative(&self) -> bool {
        let logical = &self.logical;
        !self.surface.is_empty()
            || !self.tracks.is_empty()
            || [
                logical.padding_start,
                logical.padding_end,
                logical.margin_start,
                logical.margin_end,
                logical.inset_start,
                logical.inset_end,
            ]
            .into_iter()
            .flatten()
            .any(SizeDimension::is_surface_relative)
    }

    /// A flex row along the inline axis: items run left-to-right under [`Direction::Ltr`] and right-to-left under [`Direction::Rtl`], the way `flex-direction: row` follows `dir` on the web. Use [`flex_row_reverse`](Self::flex_row_reverse) for a row that is reversed in both directions.
    pub fn flex_row(mut self) -> Self {
        self.inner.display = Display::Flex;
        self.inner.flex_direction = FlexDirection::Row;
        self.logical.row_follows_direction = true;
        self
    }

    /// A flex row laid out against the writing direction, unconditionally. Unlike [`flex_row`](Self::flex_row) this is a physical choice and does not flip with [`Direction`].
    pub fn flex_row_reverse(mut self) -> Self {
        self.inner.display = Display::Flex;
        self.inner.flex_direction = FlexDirection::RowReverse;
        self.logical.row_follows_direction = false;
        self
    }

    pub fn flex_column(mut self) -> Self {
        self.inner.display = Display::Flex;
        self.inner.flex_direction = FlexDirection::Column;
        self.logical.row_follows_direction = false;
        self
    }

    pub fn flex_wrap(mut self) -> Self {
        self.inner.flex_wrap = FlexWrap::Wrap;
        self
    }

    /// Whether the node is in layout flow at all — the declarative half of the question, re-read whenever the style is: `shown:` in a `[view]` is this builder, so an area that comes and goes with what it reads keeps its subtree and its state, where an `if` would build it again from nothing.
    ///
    /// Says nothing about `LayoutEngine::set_display`, which answers out of band and wins while it holds.
    pub fn shown(mut self, shown: bool) -> Self {
        self.logical.hidden = !shown;
        self
    }

    /// Declares the node out of layout flow (no space, not laid out) as part of its own style — e.g. a tab panel that should start inactive, as opposed to the out-of-band `LayoutEngine::set_display`.
    pub fn display_none(self) -> Self {
        self.shown(false)
    }

    /// Takes the node out of normal flow (`position: absolute`) with all four insets pinned to 0, so it fills its containing block without affecting sibling layout — used by `overlay` to cover the viewport. Combine with `flex_column`/alignment to position the overlay's content within the layer.
    pub fn absolute_fill(mut self) -> Self {
        self.inner.position = taffy::Position::Absolute;
        let zero = LengthPercentageAuto::length(0.0);
        self.inner.inset = taffy::Rect {
            left: zero,
            right: zero,
            top: zero,
            bottom: zero,
        };
        self.surface
            .retain(|(held, _)| !matches!(held, Slot::InsetTop | Slot::InsetBottom));
        self
    }

    /// Takes the node out of normal flow (`position: absolute`) leaving every inset at `auto`, so the edges it is pinned by are exactly the ones the caller names. [`absolute_fill`](Self::absolute_fill) is this plus all four insets at 0; a floating panel wants three of them and its own size on the fourth axis, which pinning everything would override.
    pub fn absolute(mut self) -> Self {
        self.inner.position = taffy::Position::Absolute;
        self
    }

    /// Inset from the top edge, for a node already taken out of flow. Physical, not logical: `top` does not swap under RTL the way [`inset_start`](Self::inset_start) does.
    pub fn inset_top(mut self, size: impl Into<SizeDimension>) -> Self {
        self.put(Slot::InsetTop, size.into());
        self
    }

    /// Inset from the bottom edge, for a node already taken out of flow.
    pub fn inset_bottom(mut self, size: impl Into<SizeDimension>) -> Self {
        self.put(Slot::InsetBottom, size.into());
        self
    }

    /// The node's `width` in pixels if it is a definite length, else `None` (e.g. percent, a fraction of the surface, or auto). Lets widgets with an intrinsic size (e.g. `<svg>`/`<img>`) inspect a caller-supplied width before registering their layout leaf.
    pub fn width_px(&self) -> Option<f32> {
        self.inner.size.width.into_option()
    }

    /// True when `width` was left at its default, which taffy also treats as `auto`.
    pub fn is_width_auto(&self) -> bool {
        self.inner.size.width.is_auto() && self.surface_slot(Slot::Width).is_none()
    }

    pub fn width(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.put(Slot::Width, dim.into());
        self
    }

    /// The node's `height` in pixels if it is a definite length, else `None` (e.g. percent, a fraction of the surface, or auto).
    pub fn height_px(&self) -> Option<f32> {
        self.inner.size.height.into_option()
    }

    /// True when `height` was left at its default, which taffy also treats as `auto`.
    pub fn is_height_auto(&self) -> bool {
        self.inner.size.height.is_auto() && self.surface_slot(Slot::Height).is_none()
    }

    pub fn height(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.put(Slot::Height, dim.into());
        self
    }

    pub fn min_width(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.put(Slot::MinWidth, dim.into());
        self
    }

    pub fn min_height(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.put(Slot::MinHeight, dim.into());
        self
    }

    pub fn max_width(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.put(Slot::MaxWidth, dim.into());
        self
    }

    pub fn max_height(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.put(Slot::MaxHeight, dim.into());
        self
    }

    pub fn flex_grow(mut self, grow: f32) -> Self {
        self.inner.flex_grow = grow;
        self
    }

    pub fn flex_shrink(mut self, shrink: f32) -> Self {
        self.inner.flex_shrink = shrink;
        self
    }

    pub fn flex_basis(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.put(Slot::FlexBasis, dim.into());
        self
    }

    pub fn padding_all(mut self, size: impl Into<SizeDimension>) -> Self {
        let d = size.into();
        // Snapped per axis, not once: the two steps differ, and a cell is taller than it is wide.
        for slot in [
            Slot::PaddingLeft,
            Slot::PaddingRight,
            Slot::PaddingTop,
            Slot::PaddingBottom,
        ] {
            self.put(slot, d);
        }
        self
    }

    pub fn padding_horizontal(mut self, size: impl Into<SizeDimension>) -> Self {
        let d = size.into();
        self.put(Slot::PaddingLeft, d);
        self.put(Slot::PaddingRight, d);
        self
    }

    pub fn padding_vertical(mut self, size: impl Into<SizeDimension>) -> Self {
        let d = size.into();
        self.put(Slot::PaddingTop, d);
        self.put(Slot::PaddingBottom, d);
        self
    }

    pub fn padding_top(mut self, size: impl Into<SizeDimension>) -> Self {
        self.put(Slot::PaddingTop, size.into());
        self
    }

    pub fn padding_bottom(mut self, size: impl Into<SizeDimension>) -> Self {
        self.put(Slot::PaddingBottom, size.into());
        self
    }

    pub fn padding_left(mut self, px: f32) -> Self {
        self.put(Slot::PaddingLeft, SizeDimension::Px(px));
        self
    }

    pub fn padding_right(mut self, px: f32) -> Self {
        self.put(Slot::PaddingRight, SizeDimension::Px(px));
        self
    }

    /// Padding on the edge the text starts from — `left` under [`Direction::Ltr`], `right` under [`Direction::Rtl`].
    pub fn padding_start(mut self, size: impl Into<SizeDimension>) -> Self {
        self.logical.padding_start = Some(space_x(size));
        self
    }

    /// Padding on the edge the text runs towards — `right` under [`Direction::Ltr`], `left` under [`Direction::Rtl`].
    pub fn padding_end(mut self, size: impl Into<SizeDimension>) -> Self {
        self.logical.padding_end = Some(space_x(size));
        self
    }

    /// All four margins at once, named by axis rather than by side so they follow the writing direction.
    pub fn margin(self, m: Margin) -> Self {
        self.margin_block_start(m.block_start)
            .margin_block_end(m.block_end)
            .margin_inline_start(m.inline_start)
            .margin_inline_end(m.inline_end)
    }

    /// Margin on the edge the block axis starts from — the top, in every writing mode this engine supports.
    pub fn margin_block_start(mut self, size: impl Into<SizeDimension>) -> Self {
        self.put(Slot::MarginTop, size.into());
        self
    }

    /// Margin on the edge the block axis ends at — the bottom.
    pub fn margin_block_end(mut self, size: impl Into<SizeDimension>) -> Self {
        self.put(Slot::MarginBottom, size.into());
        self
    }

    /// Margin on the edge the text starts from — `left` under [`Direction::Ltr`], `right` under [`Direction::Rtl`].
    pub fn margin_inline_start(mut self, size: impl Into<SizeDimension>) -> Self {
        self.logical.margin_start = Some(space_x(size));
        self
    }

    /// Margin on the edge the text runs towards — `right` under [`Direction::Ltr`], `left` under [`Direction::Rtl`].
    pub fn margin_inline_end(mut self, size: impl Into<SizeDimension>) -> Self {
        self.logical.margin_end = Some(space_x(size));
        self
    }

    /// A margin from the viewport's physical left edge, which does **not** follow the writing direction.
    ///
    /// The one place that is right: placing an in-flow box at an x already worked out in physical viewport coordinates — a dropdown panel under its trigger, a picker under its anchor. Those come from a laid-out rect, so mirroring them under RTL would put the panel on the wrong side of the screen. For a margin that is part of a box's own spacing, use [`margin_inline_start`](Self::margin_inline_start).
    pub fn margin_from_left(mut self, px: f32) -> Self {
        self.inner.margin.left =
            LengthPercentageAuto::length(geometry_core::layout_grid().snap_pos_x(px));
        self
    }

    /// Inset from the edge the text starts from, for a node already taken out of flow (see [`absolute_fill`](Self::absolute_fill)); ignored on an in-flow node, as `inset` is in CSS.
    pub fn inset_start(mut self, size: impl Into<SizeDimension>) -> Self {
        self.logical.inset_start = Some(pos_x(size));
        self
    }

    /// Inset from the edge the text runs towards, for a node already taken out of flow.
    pub fn inset_end(mut self, size: impl Into<SizeDimension>) -> Self {
        self.logical.inset_end = Some(pos_x(size));
        self
    }

    /// Reserves the room a painted stroke takes on a surface that draws it in whole cells.
    ///
    /// On a raster backend a stroke is drawn inside the box and costs no layout at all — a one-pixel rule overlaps the padding and nobody notices. A terminal has no sub-cell line: a border claims an entire cell on each side. A box laid out with no room for one therefore has its own frame and its content competing for the same row, and below three rows the text is drawn on top of the frame.
    ///
    /// Nothing on a unit grid, so a desktop window keeps exactly the geometry it had.
    pub fn bordered(self) -> Self {
        let grid = geometry_core::layout_grid();
        if grid.is_unit() {
            return self;
        }
        self.bordered_on(grid)
    }

    /// The rule itself, with the grid handed in rather than read. Split out so it can be tested without writing the process-wide grid: that global is documented as written once by a frontend before any component exists, and a test that re-writes it mid-run breaks the contract every other test in the binary is relying on.
    fn bordered_on(mut self, grid: LayoutGrid) -> Self {
        self.logical.stroke_cell = Some(grid);
        self
    }

    pub fn gap(mut self, size: impl Into<SizeDimension>) -> Self {
        let d = size.into();
        self.put(Slot::GapX, d);
        self.put(Slot::GapY, d);
        self
    }

    pub fn gap_x(mut self, size: impl Into<SizeDimension>) -> Self {
        self.put(Slot::GapX, size.into());
        self
    }

    pub fn gap_y(mut self, size: impl Into<SizeDimension>) -> Self {
        self.put(Slot::GapY, size.into());
        self
    }

    pub fn align_items(mut self, value: AlignItems) -> Self {
        self.inner.align_items = Some(value);
        self
    }

    pub fn align_self_stretch(mut self) -> Self {
        self.inner.align_self = Some(taffy::AlignSelf::STRETCH);
        self
    }

    /// Overrides the parent's `align_items` for this child, centering it on the cross axis instead of stretching — so a fixed-size child (e.g. a square icon chip) keeps its size and stays centered.
    pub fn align_self_center(mut self) -> Self {
        self.inner.align_self = Some(taffy::AlignSelf::CENTER);
        self
    }

    /// Aligns this child to the start of the cross axis, overriding the parent's `align_items`.
    pub fn align_self_start(mut self) -> Self {
        self.inner.align_self = Some(taffy::AlignSelf::FLEX_START);
        self
    }

    /// Aligns this child to the end of the cross axis, overriding the parent's `align_items`.
    pub fn align_self_end(mut self) -> Self {
        self.inner.align_self = Some(taffy::AlignSelf::FLEX_END);
        self
    }

    pub fn justify_content(mut self, value: JustifyContent) -> Self {
        self.inner.justify_content = Some(value);
        self
    }

    pub fn display_grid(mut self) -> Self {
        self.inner.display = Display::Grid;
        self
    }

    pub fn grid_template_columns(mut self, tracks: Vec<TemplateTrack>) -> Self {
        self.put_tracks(TrackSlot::TemplateColumns, tracks);
        self
    }

    pub fn grid_template_rows(mut self, tracks: Vec<TemplateTrack>) -> Self {
        self.put_tracks(TrackSlot::TemplateRows, tracks);
        self
    }

    /// Sizes the implicit rows a grid creates beyond its `grid_template_rows` — the tracks explicit placement (`grid_row`) or overflow auto-placed items land on. Defaults to `auto`, which is what taffy assumes when this is never called.
    pub fn grid_auto_rows(mut self, tracks: Vec<TemplateTrack>) -> Self {
        self.put_tracks(TrackSlot::AutoRows, tracks);
        self
    }

    /// Sizes the implicit columns, as [`Self::grid_auto_rows`] does for rows.
    pub fn grid_auto_columns(mut self, tracks: Vec<TemplateTrack>) -> Self {
        self.put_tracks(TrackSlot::AutoColumns, tracks);
        self
    }

    pub fn grid_column_span(mut self, count: u16) -> Self {
        self.inner.grid_column = taffy::geometry::Line {
            start: GridPlacement::Span(count),
            end: GridPlacement::Auto,
        };
        self
    }

    pub fn grid_row_span(mut self, count: u16) -> Self {
        self.inner.grid_row = taffy::geometry::Line {
            start: GridPlacement::Span(count),
            end: GridPlacement::Auto,
        };
        self
    }

    /// Places the item at an explicit column line (1-based, or negative to count from the end, as CSS defines) and has it span `span` tracks from there. Overlap with another explicitly placed item is not checked — that's the caller's job. Leaving the row auto on an otherwise-pinned item still moves taffy's shared auto-placement cursor, which can shift where a later, fully auto-placed sibling lands — pin both axes to avoid that.
    pub fn grid_column(mut self, start: i16, span: u16) -> Self {
        self.inner.grid_column = taffy::geometry::Line {
            start: GridPlacement::Line(start.into()),
            end: GridPlacement::Span(span),
        };
        self
    }

    /// Places the item at an explicit row line, as [`Self::grid_column`] does for columns.
    pub fn grid_row(mut self, start: i16, span: u16) -> Self {
        self.inner.grid_row = taffy::geometry::Line {
            start: GridPlacement::Line(start.into()),
            end: GridPlacement::Span(span),
        };
        self
    }

    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.inner.aspect_ratio = Some(ratio);
        self
    }

    /// The physical `taffy::Style` this describes under `direction`, on a surface of `surface` size. Called by the engine at every point a style reaches a node, and again for each affected node when the direction flips or the surface is resized.
    ///
    /// Does not place the leading margin ([`LogicalStyle::leading_margin`]) — the engine does that afterwards, since it needs the parent's axis to know which physical edge "leading" means.
    pub(crate) fn resolve(&self, direction: Direction, surface: Size) -> Style {
        let mut style = self.inner.clone();
        for &(slot, d) in &self.surface {
            slot.write(&mut style, slot.snap(d.against(surface)));
        }
        let logical = &self.logical;
        if logical.row_follows_direction || logical.row_forced {
            style.flex_direction = if direction.is_rtl() {
                FlexDirection::RowReverse
            } else {
                FlexDirection::Row
            };
        }
        if logical.is_hidden() {
            style.display = Display::None;
        }
        if let Some(min_height) = logical.min_height_override {
            style.min_size.height = LengthPercentageAuto::length(min_height);
        }
        let (start, end) = if direction.is_rtl() {
            (Edge::Right, Edge::Left)
        } else {
            (Edge::Left, Edge::Right)
        };
        for (edge, size) in [(start, logical.padding_start), (end, logical.padding_end)] {
            if let Some(size) = size {
                *edge.of_mut(&mut style.padding) = settle(size, surface, space_x).into();
            }
        }
        for (edge, size) in [(start, logical.margin_start), (end, logical.margin_end)] {
            if let Some(size) = size {
                *edge.of_mut(&mut style.margin) = settle(size, surface, space_x).into();
            }
        }
        for (edge, size) in [(start, logical.inset_start), (end, logical.inset_end)] {
            if let Some(size) = size {
                *edge.of_mut(&mut style.inset) = settle(size, surface, pos_x).into();
            }
        }
        for (slot, tracks) in &self.tracks {
            slot.write(&mut style, tracks, surface);
        }
        if let Some(grid) = logical.stroke_cell {
            // Only what the padding does not already give: a raster backend draws a stroke inside the box over its padding, and the cell equivalent is a frame in the outermost cell, so a box padded by a cell or more already has the room. Read from the resolved padding, logical and surface-relative edges included, so the builder order does not matter.
            let short = |padding: LengthPercentage, step: f32| {
                LengthPercentage::length((step - length_of(padding)).max(0.0))
            };
            style.border = taffy::geometry::Rect {
                left: short(style.padding.left, grid.x),
                right: short(style.padding.right, grid.x),
                top: short(style.padding.top, grid.y),
                bottom: short(style.padding.bottom, grid.y),
            };
        }
        style
    }
}

#[derive(Clone, Copy)]
enum Edge {
    Left,
    Right,
}

impl Edge {
    fn of_mut<T>(self, rect: &mut taffy::geometry::Rect<T>) -> &mut T {
        match self {
            Edge::Left => &mut rect.left,
            Edge::Right => &mut rect.right,
        }
    }
}

impl Default for LayoutStyle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "style_border_reservation_test.rs"]
mod border_reservation_tests;

#[cfg(test)]
#[path = "style_surface_test.rs"]
mod surface_tests;

#[cfg(test)]
#[path = "style_test.rs"]
mod tests;
