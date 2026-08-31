use taffy::{
    Dimension, Display, FlexDirection, FlexWrap, GridPlacement, LengthPercentage,
    LengthPercentageAuto, Style,
};

pub use taffy::{AlignItems, AvailableSpace, JustifyContent};

use crate::direction::Direction;
use crate::track::TemplateTrack;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeDimension {
    Px(f32),
    Percent(f32),
    Auto,
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
            SizeDimension::Auto => Dimension::auto(),
        }
    }
}

/// For padding and gap, which resolve against the containing block but have no `auto` to fall back on —
/// CSS has none there either, and taffy's type says so. `Auto` is the caller asking for a size decision on a
/// property that makes none, so it is nothing.
impl From<SizeDimension> for LengthPercentage {
    fn from(d: SizeDimension) -> Self {
        match d {
            SizeDimension::Px(v) => LengthPercentage::length(v),
            SizeDimension::Percent(v) => LengthPercentage::percent(v),
            SizeDimension::Auto => LengthPercentage::length(0.0),
        }
    }
}

/// For margin and inset, where `auto` is a real answer: it is what centres a block and what leaves an edge
/// unpinned.
impl From<SizeDimension> for LengthPercentageAuto {
    fn from(d: SizeDimension) -> Self {
        match d {
            SizeDimension::Px(v) => LengthPercentageAuto::length(v),
            SizeDimension::Percent(v) => LengthPercentageAuto::percent(v),
            SizeDimension::Auto => LengthPercentageAuto::auto(),
        }
    }
}

/// The parts of a style that cannot be turned into physical edges until a [`Direction`] is known. Kept
/// alongside the resolved `taffy::Style` rather than folded into it, so a direction flip can re-resolve the
/// original intent instead of trying to un-swap edges it can no longer tell apart from physical ones.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct LogicalStyle {
    pub(crate) padding_start: Option<SizeDimension>,
    pub(crate) padding_end: Option<SizeDimension>,
    pub(crate) margin_start: Option<SizeDimension>,
    pub(crate) margin_end: Option<SizeDimension>,
    pub(crate) inset_start: Option<SizeDimension>,
    pub(crate) inset_end: Option<SizeDimension>,
    /// Set by [`LayoutStyle::flex_row`]: the main axis is the inline axis, so it reverses under RTL. An
    /// explicit [`LayoutStyle::flex_row_reverse`] leaves this clear — it means "reversed" in either direction.
    pub(crate) row_follows_direction: bool,
    /// Set by `LayoutEngine::make_flex_row` for a node whose own declared style never called `flex_row`.
    pub(crate) row_forced: bool,
    /// Set by [`LayoutStyle::shown`] / [`LayoutStyle::display_none`]: what the node's own style says about
    /// being in flow. A style is free to say the opposite of what it said last time — which is the whole of
    /// `shown:` in a `[view]`, re-resolved from whatever it reads.
    pub(crate) hidden: bool,
    /// Set by `LayoutEngine::set_display`: an answer given *out of band*, which no style knows about and so
    /// none may overwrite. `None` leaves the question to [`hidden`](Self::hidden).
    ///
    /// The two used to be one flag that `set_style` OR-ed into whatever came next, so a node hidden once — by
    /// either door — could never be shown again by a style. That is the trap this pair exists to close: the
    /// out-of-band answer is carried forward because nothing else knows it, and the declared one is replaced
    /// because the new style is exactly what does.
    pub(crate) display_override: Option<bool>,
    /// Set by `LayoutEngine::set_min_height`: overrides `inner.min_size.height`.
    pub(crate) min_height_override: Option<f32>,
    /// Set by `LayoutEngine::set_leading_margin`; `(is_row, px)`, placed by the engine since which physical edge is "leading" depends on the parent's axis.
    pub(crate) leading_margin: Option<(bool, f32)>,
}

impl LogicalStyle {
    /// Whether any edge needs re-resolving on a direction flip. A direction-following row alone does not: it
    /// is a single flag the engine can toggle in place, without the original style to resolve against.
    pub(crate) fn has_edges(&self) -> bool {
        self.padding_start.is_some()
            || self.padding_end.is_some()
            || self.margin_start.is_some()
            || self.margin_end.is_some()
            || self.inset_start.is_some()
            || self.inset_end.is_some()
    }

    /// Whether the node is out of flow: what was set out of band if anything was, and what the style declares
    /// otherwise.
    pub(crate) fn is_hidden(&self) -> bool {
        match self.display_override {
            Some(shown) => !shown,
            None => self.hidden,
        }
    }
    /// Whether the engine must keep this node's full style around: an edge, or out-of-band mutator state.
    pub(crate) fn needs_tracking(&self) -> bool {
        self.has_edges()
            || self.row_forced
            || self.hidden
            || self.display_override.is_some()
            || self.min_height_override.is_some()
            || self.leading_margin.is_some()
    }
}

/// A box's four margins, named by axis so they follow the writing direction rather than the screen.
///
/// The nine builders this replaces mixed two vocabularies — seven physical, two logical — and nothing in the
/// name of `margin_left` said which of the two it belonged to.
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
pub struct LayoutStyle {
    pub(crate) inner: Style,
    pub(crate) logical: LogicalStyle,
}

impl LayoutStyle {
    /// A **block** box, as in CSS: children stack vertically and the flex properties do nothing.
    ///
    /// Worth saying out loud because the ones that do nothing do it silently. [`gap`](Self::gap),
    /// [`justify_content`](Self::justify_content) and [`align_items`](Self::align_items) all belong to
    /// flex layout, so on a box that never called [`flex_row`](Self::flex_row) or
    /// [`flex_column`](Self::flex_column) they are accepted and ignored — a row written without
    /// `flex_row` comes out as a column, and the reading on screen is not "that row is a column" but
    /// "why is this panel twice as tall as it should be".
    pub fn new() -> Self {
        Self {
            inner: Style {
                display: Display::Block,
                ..Style::default()
            },
            logical: LogicalStyle::default(),
        }
    }

    /// A flex row along the inline axis: items run left-to-right under [`Direction::Ltr`] and right-to-left
    /// under [`Direction::Rtl`], the way `flex-direction: row` follows `dir` on the web. Use
    /// [`flex_row_reverse`](Self::flex_row_reverse) for a row that is reversed in both directions.
    pub fn flex_row(mut self) -> Self {
        self.inner.display = Display::Flex;
        self.inner.flex_direction = FlexDirection::Row;
        self.logical.row_follows_direction = true;
        self
    }

    /// A flex row laid out against the writing direction, unconditionally. Unlike
    /// [`flex_row`](Self::flex_row) this is a physical choice and does not flip with [`Direction`].
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

    /// Whether the node is in layout flow at all — the declarative half of the question, re-read whenever the
    /// style is: `shown:` in a `[view]` is this builder, so an area that comes and goes with what it reads
    /// keeps its subtree and its state, where an `if` would build it again from nothing.
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

    /// Takes the node out of normal flow (`position: absolute`) with all four insets pinned to 0, so it
    /// fills its containing block without affecting sibling layout — used by `overlay` to cover the
    /// viewport. Combine with `flex_column`/alignment to position the overlay's content within the layer.
    pub fn absolute_fill(mut self) -> Self {
        self.inner.position = taffy::Position::Absolute;
        let zero = LengthPercentageAuto::length(0.0);
        self.inner.inset = taffy::Rect {
            left: zero,
            right: zero,
            top: zero,
            bottom: zero,
        };
        self
    }

    /// Takes the node out of normal flow (`position: absolute`) leaving every inset at `auto`, so the
    /// edges it is pinned by are exactly the ones the caller names. [`absolute_fill`](Self::absolute_fill)
    /// is this plus all four insets at 0; a floating panel wants three of them and its own size on the
    /// fourth axis, which pinning everything would override.
    pub fn absolute(mut self) -> Self {
        self.inner.position = taffy::Position::Absolute;
        self
    }

    /// Inset from the top edge, for a node already taken out of flow. Physical, not logical: `top` does not
    /// swap under RTL the way [`inset_start`](Self::inset_start) does.
    pub fn inset_top(mut self, size: impl Into<SizeDimension>) -> Self {
        self.inner.inset.top = size.into().into();
        self
    }

    /// Inset from the bottom edge, for a node already taken out of flow.
    pub fn inset_bottom(mut self, size: impl Into<SizeDimension>) -> Self {
        self.inner.inset.bottom = size.into().into();
        self
    }

    /// The node's `width` in pixels if it is a definite length, else `None` (e.g. percent or auto).
    /// Lets widgets with an intrinsic size (e.g. `<svg>`/`<img>`) inspect a caller-supplied width before registering their layout leaf.
    pub fn width_px(&self) -> Option<f32> {
        self.inner.size.width.into_option()
    }

    /// True when `width` was left at its default, which taffy also treats as `auto`.
    pub fn is_width_auto(&self) -> bool {
        self.inner.size.width.is_auto()
    }

    pub fn width(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.inner.size.width = dim.into().into();
        self
    }

    /// The node's `height` in pixels if it is a definite length, else `None` (e.g. percent or auto).
    pub fn height_px(&self) -> Option<f32> {
        self.inner.size.height.into_option()
    }

    /// True when `height` was left at its default, which taffy also treats as `auto`.
    pub fn is_height_auto(&self) -> bool {
        self.inner.size.height.is_auto()
    }

    pub fn height(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.inner.size.height = dim.into().into();
        self
    }

    pub fn min_width(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.inner.min_size.width = dim.into().into();
        self
    }

    pub fn min_height(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.inner.min_size.height = dim.into().into();
        self
    }

    pub fn max_width(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.inner.max_size.width = dim.into().into();
        self
    }

    pub fn max_height(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.inner.max_size.height = dim.into().into();
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
        self.inner.flex_basis = dim.into().into();
        self
    }

    pub fn padding_all(mut self, size: impl Into<SizeDimension>) -> Self {
        let value: LengthPercentage = size.into().into();
        self.inner.padding = taffy::geometry::Rect {
            left: value,
            right: value,
            top: value,
            bottom: value,
        };
        self
    }

    pub fn padding_horizontal(mut self, size: impl Into<SizeDimension>) -> Self {
        let value: LengthPercentage = size.into().into();
        self.inner.padding.left = value;
        self.inner.padding.right = value;
        self
    }

    pub fn padding_vertical(mut self, size: impl Into<SizeDimension>) -> Self {
        let value: LengthPercentage = size.into().into();
        self.inner.padding.top = value;
        self.inner.padding.bottom = value;
        self
    }

    pub fn padding_top(mut self, size: impl Into<SizeDimension>) -> Self {
        self.inner.padding.top = size.into().into();
        self
    }

    pub fn padding_bottom(mut self, size: impl Into<SizeDimension>) -> Self {
        self.inner.padding.bottom = size.into().into();
        self
    }

    pub fn padding_left(mut self, px: f32) -> Self {
        self.inner.padding.left = LengthPercentage::length(px);
        self
    }

    pub fn padding_right(mut self, px: f32) -> Self {
        self.inner.padding.right = LengthPercentage::length(px);
        self
    }

    /// Padding on the edge the text starts from — `left` under [`Direction::Ltr`], `right` under
    /// [`Direction::Rtl`].
    pub fn padding_start(mut self, size: impl Into<SizeDimension>) -> Self {
        self.logical.padding_start = Some(size.into());
        self
    }

    /// Padding on the edge the text runs towards — `right` under [`Direction::Ltr`], `left` under
    /// [`Direction::Rtl`].
    pub fn padding_end(mut self, size: impl Into<SizeDimension>) -> Self {
        self.logical.padding_end = Some(size.into());
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
        self.inner.margin.top = size.into().into();
        self
    }

    /// Margin on the edge the block axis ends at — the bottom.
    pub fn margin_block_end(mut self, size: impl Into<SizeDimension>) -> Self {
        self.inner.margin.bottom = size.into().into();
        self
    }

    /// Margin on the edge the text starts from — `left` under [`Direction::Ltr`], `right` under
    /// [`Direction::Rtl`].
    pub fn margin_inline_start(mut self, size: impl Into<SizeDimension>) -> Self {
        self.logical.margin_start = Some(size.into());
        self
    }

    /// Margin on the edge the text runs towards — `right` under [`Direction::Ltr`], `left` under
    /// [`Direction::Rtl`].
    pub fn margin_inline_end(mut self, size: impl Into<SizeDimension>) -> Self {
        self.logical.margin_end = Some(size.into());
        self
    }

    /// A margin from the viewport's physical left edge, which does **not** follow the writing direction.
    ///
    /// The one place that is right: placing an in-flow box at an x already worked out in physical viewport
    /// coordinates — a dropdown panel under its trigger, a picker under its anchor. Those come from a
    /// laid-out rect, so mirroring them under RTL would put the panel on the wrong side of the screen. For a
    /// margin that is part of a box's own spacing, use [`margin_inline_start`](Self::margin_inline_start).
    pub fn margin_from_left(mut self, px: f32) -> Self {
        self.inner.margin.left = LengthPercentageAuto::length(px);
        self
    }

    /// Inset from the edge the text starts from, for a node already taken out of flow (see
    /// [`absolute_fill`](Self::absolute_fill)); ignored on an in-flow node, as `inset` is in CSS.
    pub fn inset_start(mut self, size: impl Into<SizeDimension>) -> Self {
        self.logical.inset_start = Some(size.into());
        self
    }

    /// Inset from the edge the text runs towards, for a node already taken out of flow.
    pub fn inset_end(mut self, size: impl Into<SizeDimension>) -> Self {
        self.logical.inset_end = Some(size.into());
        self
    }

    pub fn gap(mut self, size: impl Into<SizeDimension>) -> Self {
        let value: LengthPercentage = size.into().into();
        self.inner.gap = taffy::geometry::Size {
            width: value,
            height: value,
        };
        self
    }

    pub fn gap_x(mut self, size: impl Into<SizeDimension>) -> Self {
        self.inner.gap.width = size.into().into();
        self
    }

    pub fn gap_y(mut self, size: impl Into<SizeDimension>) -> Self {
        self.inner.gap.height = size.into().into();
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

    /// Overrides the parent's `align_items` for this child, centering it on the cross axis instead of
    /// stretching — so a fixed-size child (e.g. a square icon chip) keeps its size and stays centered.
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
        self.inner.grid_template_columns = tracks
            .into_iter()
            .map(|t| t.into_template_component())
            .collect();
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

    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.inner.aspect_ratio = Some(ratio);
        self
    }

    /// The physical `taffy::Style` this describes under `direction`. Called by the engine at every point a
    /// style reaches a node, and again for each affected node when the direction flips.
    ///
    /// Does not place the leading margin ([`LogicalStyle::leading_margin`]) — the engine does that afterwards, since it needs the parent's axis to know which physical edge "leading" means.
    pub(crate) fn resolve(&self, direction: Direction) -> Style {
        let mut style = self.inner.clone();
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
            style.min_size.height = Dimension::length(min_height);
        }
        let (start, end) = if direction.is_rtl() {
            (Edge::Right, Edge::Left)
        } else {
            (Edge::Left, Edge::Right)
        };
        for (edge, size) in [(start, logical.padding_start), (end, logical.padding_end)] {
            if let Some(size) = size {
                *edge.of_mut(&mut style.padding) = size.into();
            }
        }
        for (edge, size) in [(start, logical.margin_start), (end, logical.margin_end)] {
            if let Some(size) = size {
                *edge.of_mut(&mut style.margin) = size.into();
            }
        }
        for (edge, size) in [(start, logical.inset_start), (end, logical.inset_end)] {
            if let Some(size) = size {
                *edge.of_mut(&mut style.inset) = size.into();
            }
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
mod tests {
    use super::*;

    #[test]
    fn logical_padding_resolves_to_the_edge_the_direction_starts_from() {
        let style = LayoutStyle::new().padding_start(8.0).padding_end(2.0);
        let ltr = style.resolve(Direction::Ltr);
        assert_eq!(ltr.padding.left, LengthPercentage::length(8.0));
        assert_eq!(ltr.padding.right, LengthPercentage::length(2.0));
        let rtl = style.resolve(Direction::Rtl);
        assert_eq!(rtl.padding.right, LengthPercentage::length(8.0));
        assert_eq!(rtl.padding.left, LengthPercentage::length(2.0));
    }

    #[test]
    fn a_physical_edge_is_left_alone_by_the_direction() {
        let style = LayoutStyle::new().padding_left(12.0);
        for direction in [Direction::Ltr, Direction::Rtl] {
            let resolved = style.resolve(direction);
            assert_eq!(resolved.padding.left, LengthPercentage::length(12.0));
            assert_eq!(resolved.padding.right, LengthPercentage::length(0.0));
        }
    }

    #[test]
    fn resolving_twice_does_not_accumulate() {
        // The engine re-resolves from the same LayoutStyle on every flip, so resolution must be a pure function of the intent.
        let style = LayoutStyle::new().padding_start(8.0);
        let _ = style.resolve(Direction::Rtl);
        let back = style.resolve(Direction::Ltr);
        assert_eq!(back.padding.left, LengthPercentage::length(8.0));
        assert_eq!(back.padding.right, LengthPercentage::length(0.0));
    }

    #[test]
    fn a_row_reverses_under_rtl_but_an_explicit_reverse_does_not_flip_back() {
        let row = LayoutStyle::new().flex_row();
        assert_eq!(
            row.resolve(Direction::Ltr).flex_direction,
            FlexDirection::Row
        );
        assert_eq!(
            row.resolve(Direction::Rtl).flex_direction,
            FlexDirection::RowReverse
        );
        let reversed = LayoutStyle::new().flex_row_reverse();
        for direction in [Direction::Ltr, Direction::Rtl] {
            assert_eq!(
                reversed.resolve(direction).flex_direction,
                FlexDirection::RowReverse,
                "an explicit reverse is physical"
            );
        }
    }

    #[test]
    fn a_column_is_unaffected_by_direction() {
        let col = LayoutStyle::new().flex_column();
        for direction in [Direction::Ltr, Direction::Rtl] {
            assert_eq!(col.resolve(direction).flex_direction, FlexDirection::Column);
        }
    }

    #[test]
    fn only_logical_edges_need_the_style_kept_for_a_flip() {
        assert!(!LayoutStyle::new().flex_row().logical.has_edges());
        assert!(
            LayoutStyle::new()
                .margin_inline_start(4.0)
                .logical
                .has_edges()
        );
        assert!(LayoutStyle::new().inset_end(4.0).logical.has_edges());
    }

    #[test]
    fn style_default_is_block() {
        let style = LayoutStyle::new();
        assert_eq!(style.inner.display, Display::Block);
    }

    #[test]
    fn style_width_sets_dimension() {
        let style = LayoutStyle::new().width(120.0);
        assert_eq!(style.inner.size.width, Dimension::length(120.0));
    }

    #[test]
    fn style_width_px_reads_back_length() {
        let style = LayoutStyle::new().width(120.0);
        assert_eq!(style.width_px(), Some(120.0));
        assert!(!style.is_width_auto());
    }

    #[test]
    fn style_width_px_none_for_percent_or_default() {
        assert_eq!(LayoutStyle::new().width_px(), None);
        assert!(LayoutStyle::new().is_width_auto());
        let percent = LayoutStyle::new().width(SizeDimension::Percent(0.5));
        assert_eq!(percent.width_px(), None);
        assert!(!percent.is_width_auto());
    }

    #[test]
    fn style_width_percent_sets_dimension() {
        let style = LayoutStyle::new().width(SizeDimension::Percent(0.5));
        assert_eq!(style.inner.size.width, Dimension::percent(0.5));
    }

    #[test]
    fn style_height_sets_dimension() {
        let style = LayoutStyle::new().height(80.0);
        assert_eq!(style.inner.size.height, Dimension::length(80.0));
    }

    #[test]
    fn style_max_width_sets_dimension() {
        let style = LayoutStyle::new().max_width(200.0);
        assert_eq!(style.inner.max_size.width, Dimension::length(200.0));
    }

    #[test]
    fn style_max_height_sets_dimension() {
        let style = LayoutStyle::new().max_height(150.0);
        assert_eq!(style.inner.max_size.height, Dimension::length(150.0));
    }

    #[test]
    fn style_flex_basis_percent_sets_dimension() {
        let style = LayoutStyle::new().flex_basis(SizeDimension::Percent(0.5));
        assert_eq!(style.inner.flex_basis, Dimension::percent(0.5));
    }

    #[test]
    fn style_flex_row_sets_direction() {
        let style = LayoutStyle::new().flex_row();
        assert_eq!(style.inner.flex_direction, FlexDirection::Row);
    }

    #[test]
    fn style_flex_column_sets_direction() {
        let style = LayoutStyle::new().flex_column();
        assert_eq!(style.inner.flex_direction, FlexDirection::Column);
    }

    #[test]
    fn style_align_items_center_sets_field() {
        let style = LayoutStyle::new().align_items(AlignItems::CENTER);
        assert_eq!(style.inner.align_items, Some(taffy::AlignItems::CENTER));
    }

    #[test]
    fn style_justify_center_sets_field() {
        let style = LayoutStyle::new().justify_content(JustifyContent::CENTER);
        assert_eq!(
            style.inner.justify_content,
            Some(taffy::JustifyContent::CENTER)
        );
    }

    #[test]
    fn style_default_impl_matches_new() {
        let style = LayoutStyle::default();
        assert_eq!(style.inner.display, Display::Block);
    }

    #[test]
    fn style_padding_horizontal_sets_left_right() {
        let style = LayoutStyle::new().padding_horizontal(10.0);
        assert_eq!(style.inner.padding.left, LengthPercentage::length(10.0));
        assert_eq!(style.inner.padding.right, LengthPercentage::length(10.0));
    }

    #[test]
    fn style_padding_vertical_sets_top_bottom() {
        let style = LayoutStyle::new().padding_vertical(8.0);
        assert_eq!(style.inner.padding.top, LengthPercentage::length(8.0));
        assert_eq!(style.inner.padding.bottom, LengthPercentage::length(8.0));
    }

    #[test]
    fn style_padding_top_sets_field() {
        let style = LayoutStyle::new().padding_top(4.0);
        assert_eq!(style.inner.padding.top, LengthPercentage::length(4.0));
    }

    // The block pair is physical-by-construction: there is no vertical writing mode here, so block start is the top in every direction. The inline pair is the one a flip moves, and it is the one kept in `logical` for `resolve` to place.
    #[test]
    fn margin_writes_the_block_pair_directly_and_defers_the_inline_pair() {
        let style = LayoutStyle::new().margin(Margin::symmetric(6.0, 12.0));
        assert_eq!(style.inner.margin.top, LengthPercentageAuto::length(6.0));
        assert_eq!(style.inner.margin.bottom, LengthPercentageAuto::length(6.0));
        assert_eq!(style.logical.margin_start, Some(SizeDimension::Px(12.0)));
        assert_eq!(style.logical.margin_end, Some(SizeDimension::Px(12.0)));
    }

    // What `margin_from_left` exists for: an x already in physical viewport coordinates must not mirror.
    #[test]
    fn a_margin_from_the_left_stays_left_under_rtl() {
        let style = LayoutStyle::new().margin_from_left(20.0);
        let ltr = style.resolve(Direction::Ltr);
        let rtl = style.resolve(Direction::Rtl);
        assert_eq!(ltr.margin.left, LengthPercentageAuto::length(20.0));
        assert_eq!(rtl.margin.left, LengthPercentageAuto::length(20.0));
    }
}
