use telar::preview::{Play, PlayError, PlayResult, Query};
use telar::{Border, BorderRadius, Children, RectStyle, ShapeStyle, StyledContainer};
use telar::{LayoutError, LayoutItem, LayoutStyle, Reactive, Text, box_item};

pub(super) fn text(content: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(move || content.to_string(), LayoutStyle::new(), |t| t)?;
    Ok(box_item(text))
}

pub(super) fn paragraph(content: &'static str) -> Children {
    Children::one(move || text(content))
}

pub(super) fn text_of(content: Reactive<String>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(move || content.get(), LayoutStyle::new(), |t| t)?;
    Ok(box_item(text))
}

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

/// That the nodes `query` matches are named `names`, in reading order: the order a reader meets a list in after it was rearranged.
pub(super) fn expect_names(canvas: &mut Play, query: &Query, names: &[&str]) -> PlayResult {
    let read: Vec<String> = canvas
        .find_all(query)
        .into_iter()
        .map(|node| node.name)
        .collect();
    if read == names {
        Ok(())
    } else {
        Err(PlayError::new(format!(
            "expected {query} to read {names:?}, but it reads {read:?}"
        )))
    }
}

/// `item` as a call placing it `width` wide leaves it, for a preview whose `width` control sizes the component it shows.
pub(super) fn at_width(item: Box<dyn LayoutItem>, width: f32) -> Box<dyn LayoutItem> {
    telar::place_item(item.layout_node(), move |style| style.width(width));
    item
}

/// Hands an optional arg to its prop's setter only while the control holds a value, so an unset control leaves the prop as the component defaults it.
pub(super) trait Given: Sized {
    fn given<T>(self, value: Option<T>, set: impl FnOnce(Self, T) -> Self) -> Self {
        match value {
            Some(value) => set(self, value),
            None => self,
        }
    }
}

impl<B> Given for B {}
