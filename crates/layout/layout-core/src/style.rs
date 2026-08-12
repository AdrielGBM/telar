use taffy::{
    Dimension, Display, FlexDirection, FlexWrap, GridAutoFlow, GridPlacement, LengthPercentage,
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

/// The parts of a style that cannot be turned into physical edges until a [`Direction`] is known. Kept
/// alongside the resolved `taffy::Style` rather than folded into it, so a direction flip can re-resolve the
/// original intent instead of trying to un-swap edges it can no longer tell apart from physical ones.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct LogicalStyle {
    pub(crate) padding_start: Option<f32>,
    pub(crate) padding_end: Option<f32>,
    pub(crate) margin_start: Option<f32>,
    pub(crate) margin_end: Option<f32>,
    pub(crate) inset_start: Option<f32>,
    pub(crate) inset_end: Option<f32>,
    /// Set by [`LayoutStyle::flex_row`]: the main axis is the inline axis, so it reverses under RTL. An
    /// explicit [`LayoutStyle::flex_row_reverse`] leaves this clear — it means "reversed" in either direction.
    pub(crate) row_follows_direction: bool,
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

    /// The node's `max-width` in pixels if it is a definite length, else `None`
    /// (e.g. percent or unset). Used by the layout pass to pin a resolved width.
    pub fn max_width_px(&self) -> Option<f32> {
        self.inner.max_size.width.into_option()
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

    pub fn padding_all(mut self, px: f32) -> Self {
        let value = LengthPercentage::length(px);
        self.inner.padding = taffy::geometry::Rect {
            left: value,
            right: value,
            top: value,
            bottom: value,
        };
        self
    }

    pub fn padding_horizontal(mut self, px: f32) -> Self {
        self.inner.padding.left = LengthPercentage::length(px);
        self.inner.padding.right = LengthPercentage::length(px);
        self
    }

    pub fn padding_vertical(mut self, px: f32) -> Self {
        self.inner.padding.top = LengthPercentage::length(px);
        self.inner.padding.bottom = LengthPercentage::length(px);
        self
    }

    pub fn padding_top(mut self, px: f32) -> Self {
        self.inner.padding.top = LengthPercentage::length(px);
        self
    }

    pub fn padding_bottom(mut self, px: f32) -> Self {
        self.inner.padding.bottom = LengthPercentage::length(px);
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
    pub fn padding_start(mut self, px: f32) -> Self {
        self.logical.padding_start = Some(px);
        self
    }

    /// Padding on the edge the text runs towards — `right` under [`Direction::Ltr`], `left` under
    /// [`Direction::Rtl`].
    pub fn padding_end(mut self, px: f32) -> Self {
        self.logical.padding_end = Some(px);
        self
    }

    pub fn margin_all(mut self, px: f32) -> Self {
        let value = LengthPercentageAuto::length(px);
        self.inner.margin = taffy::geometry::Rect {
            left: value,
            right: value,
            top: value,
            bottom: value,
        };
        self
    }

    pub fn margin_horizontal(mut self, px: f32) -> Self {
        self.inner.margin.left = LengthPercentageAuto::length(px);
        self.inner.margin.right = LengthPercentageAuto::length(px);
        self
    }

    pub fn margin_vertical(mut self, px: f32) -> Self {
        self.inner.margin.top = LengthPercentageAuto::length(px);
        self.inner.margin.bottom = LengthPercentageAuto::length(px);
        self
    }

    pub fn margin_top(mut self, px: f32) -> Self {
        self.inner.margin.top = LengthPercentageAuto::length(px);
        self
    }

    pub fn margin_bottom(mut self, px: f32) -> Self {
        self.inner.margin.bottom = LengthPercentageAuto::length(px);
        self
    }

    pub fn margin_left(mut self, px: f32) -> Self {
        self.inner.margin.left = LengthPercentageAuto::length(px);
        self
    }

    pub fn margin_right(mut self, px: f32) -> Self {
        self.inner.margin.right = LengthPercentageAuto::length(px);
        self
    }

    /// Margin on the edge the text starts from — `left` under [`Direction::Ltr`], `right` under
    /// [`Direction::Rtl`].
    pub fn margin_start(mut self, px: f32) -> Self {
        self.logical.margin_start = Some(px);
        self
    }

    /// Margin on the edge the text runs towards — `right` under [`Direction::Ltr`], `left` under
    /// [`Direction::Rtl`].
    pub fn margin_end(mut self, px: f32) -> Self {
        self.logical.margin_end = Some(px);
        self
    }

    /// Inset from the edge the text starts from, for a node already taken out of flow (see
    /// [`absolute_fill`](Self::absolute_fill)); ignored on an in-flow node, as `inset` is in CSS.
    pub fn inset_start(mut self, px: f32) -> Self {
        self.logical.inset_start = Some(px);
        self
    }

    /// Inset from the edge the text runs towards, for a node already taken out of flow.
    pub fn inset_end(mut self, px: f32) -> Self {
        self.logical.inset_end = Some(px);
        self
    }

    pub fn gap(mut self, px: f32) -> Self {
        self.inner.gap = taffy::geometry::Size {
            width: LengthPercentage::length(px),
            height: LengthPercentage::length(px),
        };
        self
    }

    pub fn gap_x(mut self, px: f32) -> Self {
        self.inner.gap.width = LengthPercentage::length(px);
        self
    }

    pub fn gap_y(mut self, px: f32) -> Self {
        self.inner.gap.height = LengthPercentage::length(px);
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

    pub fn grid_template_rows(mut self, tracks: Vec<TemplateTrack>) -> Self {
        self.inner.grid_template_rows = tracks
            .into_iter()
            .map(|t| t.into_template_component())
            .collect();
        self
    }

    pub fn grid_auto_flow_row(mut self) -> Self {
        self.inner.grid_auto_flow = GridAutoFlow::Row;
        self
    }

    pub fn grid_auto_flow_column(mut self) -> Self {
        self.inner.grid_auto_flow = GridAutoFlow::Column;
        self
    }

    pub fn grid_column(mut self, start: i16, end: i16) -> Self {
        self.inner.grid_column = taffy::geometry::Line {
            start: taffy::style_helpers::line(start),
            end: taffy::style_helpers::line(end),
        };
        self
    }

    pub fn grid_row(mut self, start: i16, end: i16) -> Self {
        self.inner.grid_row = taffy::geometry::Line {
            start: taffy::style_helpers::line(start),
            end: taffy::style_helpers::line(end),
        };
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
    pub(crate) fn resolve(&self, direction: Direction) -> Style {
        let mut style = self.inner.clone();
        let logical = &self.logical;
        if logical.row_follows_direction && direction.is_rtl() {
            style.flex_direction = FlexDirection::RowReverse;
        }
        let (start, end) = if direction.is_rtl() {
            (Edge::Right, Edge::Left)
        } else {
            (Edge::Left, Edge::Right)
        };
        for (edge, px) in [(start, logical.padding_start), (end, logical.padding_end)] {
            if let Some(px) = px {
                *edge.of_mut(&mut style.padding) = LengthPercentage::length(px);
            }
        }
        for (edge, px) in [(start, logical.margin_start), (end, logical.margin_end)] {
            if let Some(px) = px {
                *edge.of_mut(&mut style.margin) = LengthPercentageAuto::length(px);
            }
        }
        for (edge, px) in [(start, logical.inset_start), (end, logical.inset_end)] {
            if let Some(px) = px {
                *edge.of_mut(&mut style.inset) = LengthPercentageAuto::length(px);
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
        assert!(LayoutStyle::new().margin_start(4.0).logical.has_edges());
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

    #[test]
    fn style_margin_horizontal_sets_left_right() {
        let style = LayoutStyle::new().margin_horizontal(12.0);
        assert_eq!(style.inner.margin.left, LengthPercentageAuto::length(12.0));
        assert_eq!(style.inner.margin.right, LengthPercentageAuto::length(12.0));
    }

    #[test]
    fn style_margin_top_sets_field() {
        let style = LayoutStyle::new().margin_top(6.0);
        assert_eq!(style.inner.margin.top, LengthPercentageAuto::length(6.0));
    }
}
