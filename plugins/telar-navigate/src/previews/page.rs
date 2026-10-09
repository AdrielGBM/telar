use telar::{LayoutError, LayoutStyle, Text, TextStyle, box_item};

use crate::{NavPage, SimplePage};

/// A screen of two lines: its title and what it would hold.
pub(super) fn page(
    title: &'static str,
    body: &'static str,
) -> Result<Box<dyn NavPage>, LayoutError> {
    let heading = Text::declaring(
        move || title.to_string(),
        LayoutStyle::new(),
        |inherited: TextStyle| {
            let size = inherited.font_size * 1.4;
            inherited.with_font_size(size)
        },
    )?;
    let line = Text::declaring(move || body.to_string(), LayoutStyle::new(), |t| t)?;
    let column = telar::Container::new(
        LayoutStyle::new().flex_column().gap(8.0).padding_all(16.0),
        vec![box_item(heading), box_item(line)],
    )?;
    Ok(Box::new(SimplePage::new(column)))
}
