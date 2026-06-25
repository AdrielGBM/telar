use taffy::{
    Dimension, Display, FlexDirection, FlexWrap, GridAutoFlow, GridPlacement, LengthPercentage,
    LengthPercentageAuto, Style,
};

pub use taffy::{AlignItems, AvailableSpace, JustifyContent};

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

#[derive(Clone)]
pub struct LayoutStyle {
    pub(crate) inner: Style,
}

impl LayoutStyle {
    pub fn new() -> Self {
        Self {
            inner: Style {
                display: Display::Block,
                ..Style::default()
            },
        }
    }

    pub fn flex_row(mut self) -> Self {
        self.inner.display = Display::Flex;
        self.inner.flex_direction = FlexDirection::Row;
        self
    }

    pub fn flex_column(mut self) -> Self {
        self.inner.display = Display::Flex;
        self.inner.flex_direction = FlexDirection::Column;
        self
    }

    pub fn flex_wrap(mut self) -> Self {
        self.inner.flex_wrap = FlexWrap::Wrap;
        self
    }

    pub fn width(mut self, dim: impl Into<SizeDimension>) -> Self {
        self.inner.size.width = dim.into().into();
        self
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
