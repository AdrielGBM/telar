//! [`marker`]: a component written in Rust that draws an icon it names from Rust, so the fixture bakes and ships an icon no `.rsx` writes.

use telar::{Children, LayoutError, LayoutItem};
use telar_icons::{IconProps, icon};

/// A small square mark.
pub fn marker(_children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    icon(
        IconProps::props()
            .name(telar_icons::icon!("fixture:square"))
            .size(16.0)
            .build(),
        Children::default(),
    )
}

#[cfg(test)]
#[path = "marker_test.rs"]
mod tests;
