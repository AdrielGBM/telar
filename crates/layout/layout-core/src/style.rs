use taffy::{
    Dimension, Display, FlexDirection, GridAutoFlow, GridPlacement, LengthPercentage,
    LengthPercentageAuto, Style,
};

use crate::track::Track;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AvailableSpace {
    Definite(f32),
    MinContent,
    MaxContent,
}

impl From<AvailableSpace> for taffy::AvailableSpace {
    fn from(s: AvailableSpace) -> Self {
        match s {
            AvailableSpace::Definite(v) => taffy::AvailableSpace::Definite(v),
            AvailableSpace::MinContent => taffy::AvailableSpace::MinContent,
            AvailableSpace::MaxContent => taffy::AvailableSpace::MaxContent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Start,
    End,
    Center,
    Stretch,
    Baseline,
}

impl From<AlignItems> for taffy::AlignItems {
    fn from(v: AlignItems) -> Self {
        match v {
            AlignItems::Start => taffy::AlignItems::Start,
            AlignItems::End => taffy::AlignItems::End,
            AlignItems::Center => taffy::AlignItems::Center,
            AlignItems::Stretch => taffy::AlignItems::Stretch,
            AlignItems::Baseline => taffy::AlignItems::Baseline,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl From<JustifyContent> for taffy::JustifyContent {
    fn from(v: JustifyContent) -> Self {
        match v {
            JustifyContent::Start => taffy::JustifyContent::Start,
            JustifyContent::End => taffy::JustifyContent::End,
            JustifyContent::Center => taffy::JustifyContent::Center,
            JustifyContent::SpaceBetween => taffy::JustifyContent::SpaceBetween,
            JustifyContent::SpaceAround => taffy::JustifyContent::SpaceAround,
            JustifyContent::SpaceEvenly => taffy::JustifyContent::SpaceEvenly,
        }
    }
}

pub struct LayoutStyle {
    pub(crate) inner: Style,
}

impl LayoutStyle {
    pub fn new() -> Self {
        Self {
            inner: Style {
                display: Display::Flex,
                ..Style::default()
            },
        }
    }

    pub fn flex_row(mut self) -> Self {
        self.inner.flex_direction = FlexDirection::Row;
        self
    }

    pub fn flex_column(mut self) -> Self {
        self.inner.flex_direction = FlexDirection::Column;
        self
    }

    pub fn width(mut self, px: f32) -> Self {
        self.inner.size.width = Dimension::length(px);
        self
    }

    pub fn height(mut self, px: f32) -> Self {
        self.inner.size.height = Dimension::length(px);
        self
    }

    pub fn width_percent(mut self, pct: f32) -> Self {
        self.inner.size.width = Dimension::percent(pct);
        self
    }

    pub fn height_percent(mut self, pct: f32) -> Self {
        self.inner.size.height = Dimension::percent(pct);
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

    pub fn min_width(mut self, px: f32) -> Self {
        self.inner.min_size.width = Dimension::length(px);
        self
    }

    pub fn min_height(mut self, px: f32) -> Self {
        self.inner.min_size.height = Dimension::length(px);
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

    pub fn gap(mut self, px: f32) -> Self {
        self.inner.gap = taffy::geometry::Size {
            width: LengthPercentage::length(px),
            height: LengthPercentage::length(px),
        };
        self
    }

    pub fn align_items(mut self, value: AlignItems) -> Self {
        self.inner.align_items = Some(value.into());
        self
    }

    pub fn justify_content(mut self, value: JustifyContent) -> Self {
        self.inner.justify_content = Some(value.into());
        self
    }

    pub fn display_grid(mut self) -> Self {
        self.inner.display = Display::Grid;
        self
    }

    pub fn grid_template_columns(mut self, tracks: Vec<Track>) -> Self {
        self.inner.grid_template_columns = tracks
            .into_iter()
            .map(|t| t.into_template_component())
            .collect();
        self
    }

    pub fn grid_template_rows(mut self, tracks: Vec<Track>) -> Self {
        self.inner.grid_template_rows = tracks
            .into_iter()
            .map(|t| t.into_template_component())
            .collect();
        self
    }

    pub fn grid_auto_rows(mut self, tracks: Vec<Track>) -> Self {
        self.inner.grid_auto_rows = tracks.into_iter().map(|t| t.into_auto_track()).collect();
        self
    }

    pub fn grid_auto_columns(mut self, tracks: Vec<Track>) -> Self {
        self.inner.grid_auto_columns = tracks.into_iter().map(|t| t.into_auto_track()).collect();
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

    pub fn grid_auto_flow_row_dense(mut self) -> Self {
        self.inner.grid_auto_flow = GridAutoFlow::RowDense;
        self
    }

    pub fn grid_auto_flow_column_dense(mut self) -> Self {
        self.inner.grid_auto_flow = GridAutoFlow::ColumnDense;
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

    pub fn grid_column_span(mut self, n: u16) -> Self {
        self.inner.grid_column = taffy::geometry::Line {
            start: GridPlacement::Span(n),
            end: GridPlacement::Auto,
        };
        self
    }

    pub fn grid_row_span(mut self, n: u16) -> Self {
        self.inner.grid_row = taffy::geometry::Line {
            start: GridPlacement::Span(n),
            end: GridPlacement::Auto,
        };
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
    fn style_default_is_flex() {
        let style = LayoutStyle::new();
        assert_eq!(style.inner.display, Display::Flex);
    }

    #[test]
    fn style_width_sets_dimension() {
        let style = LayoutStyle::new().width(120.0);
        assert_eq!(style.inner.size.width, Dimension::length(120.0));
    }

    #[test]
    fn style_height_sets_dimension() {
        let style = LayoutStyle::new().height(80.0);
        assert_eq!(style.inner.size.height, Dimension::length(80.0));
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
        let style = LayoutStyle::new().align_items(AlignItems::Center);
        assert_eq!(style.inner.align_items, Some(taffy::AlignItems::Center));
    }

    #[test]
    fn style_justify_center_sets_field() {
        let style = LayoutStyle::new().justify_content(JustifyContent::Center);
        assert_eq!(
            style.inner.justify_content,
            Some(taffy::JustifyContent::Center)
        );
    }

    #[test]
    fn style_default_impl_matches_new() {
        let style = LayoutStyle::default();
        assert_eq!(style.inner.display, Display::Flex);
    }
}
