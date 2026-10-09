#[cfg(feature = "advanced")]
use telar::{Border, BorderRadius, RectStyle, ShapeStyle, StyledContainer};
use telar::{Children, LayoutError, LayoutItem, LayoutStyle, Reactive, Slots, Text, box_item};

pub(super) fn text(content: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(move || content.to_string(), LayoutStyle::new(), |t| t)?;
    Ok(box_item(text))
}

pub(super) fn children(
    build: impl Fn() -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> + 'static,
) -> Children {
    Children::new(move || {
        let mut slots = Slots::new();
        for item in build()? {
            slots.push(None, item);
        }
        Ok(slots)
    })
}

#[cfg(feature = "advanced")]
pub(super) fn paragraph(content: &'static str) -> Children {
    children(move || Ok(vec![text(content)?]))
}

pub(super) fn text_of(content: Reactive<String>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(move || content.get(), LayoutStyle::new(), |t| t)?;
    Ok(box_item(text))
}

#[cfg(feature = "advanced")]
pub(super) fn tile(label: Reactive<String>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = text_of(label)?;
    let tile = StyledContainer::new(
        LayoutStyle::new()
            .padding_horizontal(12.0)
            .padding_vertical(6.0),
        |_| {
            let tokens = telar::use_theme_tokens();
            RectStyle::default()
                .with_fill(tokens.surface_alt())
                .with_border(Border::uniform(tokens.border(), 1.0))
                .with_radius(BorderRadius::all(tokens.radius()))
        },
        vec![text],
    )?;
    Ok(box_item(tile))
}
