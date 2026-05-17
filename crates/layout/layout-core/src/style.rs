use taffy::{
    AlignItems, Dimension, Display, FlexDirection, JustifyContent, LengthPercentage,
    LengthPercentageAuto, Style,
};

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

    pub fn align_items_center(mut self) -> Self {
        self.inner.align_items = Some(AlignItems::Center);
        self
    }

    pub fn align_items_stretch(mut self) -> Self {
        self.inner.align_items = Some(AlignItems::Stretch);
        self
    }

    pub fn justify_center(mut self) -> Self {
        self.inner.justify_content = Some(JustifyContent::Center);
        self
    }

    pub fn justify_space_between(mut self) -> Self {
        self.inner.justify_content = Some(JustifyContent::SpaceBetween);
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
        let style = LayoutStyle::new().align_items_center();
        assert_eq!(style.inner.align_items, Some(AlignItems::Center));
    }

    #[test]
    fn style_justify_center_sets_field() {
        let style = LayoutStyle::new().justify_center();
        assert_eq!(style.inner.justify_content, Some(JustifyContent::Center));
    }

    #[test]
    fn style_default_impl_matches_new() {
        let style = LayoutStyle::default();
        assert_eq!(style.inner.display, Display::Flex);
    }
}
