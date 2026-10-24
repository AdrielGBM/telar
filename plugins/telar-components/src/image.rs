//! [`image`]: a picture and the words that stand for it, with a quiet placeholder while it loads and the words themselves where it could not.

use std::sync::Arc;

use telar::{
    Accessible, AlignItems, BorderRadius, Children, Clip, ClippedItem, ImageData, JustifyContent,
    LayoutError, LayoutItem, LayoutStyle, ObjectFit, Props, Raster, Reactive, RectStyle,
    ShapeStyle, StyledContainer, SurfaceAmendment, SurfaceStyle, Text, TextWrap, amend_surface,
    box_item, children,
};

use crate::shared;

/// An image's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the image is drawn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImageTokens {
    /// Corner of the picture and of its placeholder. `None` is the theme's `radius_item`. An image's own `radius` prop wins over it.
    pub radius: Option<f32>,
}

impl ImageTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_item)
    }
}

/// Where an [`image`]'s picture is: still on its way, here, or not coming.
///
/// A decoded picture converts into one, and so does the state an [`AssetLoader`](telar::AssetLoader) hands out under the `async-assets` feature, so an image follows a load by being given the loader's signal.
#[derive(Clone, Debug, Default)]
pub enum ImageSource {
    #[default]
    Loading,
    Ready(Arc<ImageData>),
    Failed,
}

impl From<Arc<ImageData>> for ImageSource {
    fn from(data: Arc<ImageData>) -> Self {
        Self::Ready(data)
    }
}

impl From<ImageData> for ImageSource {
    fn from(data: ImageData) -> Self {
        Self::Ready(Arc::new(data))
    }
}

#[cfg(feature = "async-assets")]
impl From<telar::AssetState<Arc<ImageData>>> for ImageSource {
    fn from(state: telar::AssetState<Arc<ImageData>>) -> Self {
        match state {
            telar::AssetState::Loading => Self::Loading,
            telar::AssetState::Ready(data) => Self::Ready(data),
            telar::AssetState::Failed(_) => Self::Failed,
        }
    }
}

impl ImageSource {
    /// What the image is showing, as the list it is drawn by keys it: a new picture is a new widget, since a picture's size is read from it as it is built.
    fn key(&self) -> (u8, u64) {
        match self {
            Self::Loading => (0, 0),
            Self::Ready(data) => (1, data.id),
            Self::Failed => (2, 0),
        }
    }
}

/// A picture, named for a reader by `alt`: what it shows, in words, or nothing at all for a picture that only decorates, which a reader then steps over.
///
/// While the picture loads it is a quiet box of the size it will take; where it cannot be had, that box shows `alt` instead. Give it a `width`, a `height` or both, so the page does not move when the picture arrives: a side left unset follows the picture's own proportions once it is here.
#[derive(Props)]
pub struct ImageProps {
    /// The picture: a decoded [`ImageData`], or where a load of one stands.
    #[props(into)]
    pub source: Reactive<ImageSource>,
    /// What the picture shows, in words. Required, because only the author knows it: an empty `alt` says the picture only decorates.
    #[props(into)]
    pub alt: Reactive<String>,
    /// How the picture fills its box when the box is not its shape. Contained whole by default.
    #[props(default)]
    pub fit: ObjectFit,
    /// Width in logical px. `0.0` (the default) is the picture's own, or what its proportions make of the height.
    #[props(default)]
    pub width: f32,
    /// Height in logical px. `0.0` (the default) is the picture's own, or what its proportions make of the width.
    #[props(default)]
    pub height: f32,
    /// Corner of the picture. `None` (the default) is [`ImageTokens::radius`].
    #[props(some, default)]
    pub radius: Option<f32>,
    /// Draws the picture on its pixel grid, nearest-neighbour, for pixel art. Smoothed by default.
    #[props(default)]
    pub pixelated: bool,
    /// Amends the paint of the placeholder — the box shown while the picture loads or when it failed, this component's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`ImageTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A picture with the words that stand for it, and a placeholder while it loads or when it cannot.
pub fn image(props: ImageProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ImageProps {
        source,
        alt,
        fit,
        width,
        height,
        radius,
        pixelated,
        style,
    } = props;
    let corner = move || radius.unwrap_or_else(|| ImageTokens::current().radius());
    let size = Size { width, height };
    let picture = telar::ReactiveList::with_style(
        LayoutStyle::row().flex_shrink(0.0),
        move || vec![(source.get(), corner().to_bits())],
        |(source, corner): &(ImageSource, u32)| (source.key(), *corner),
        move |(source, corner)| {
            let corner = f32::from_bits(corner);
            let raster = if pixelated {
                Raster::Pixel
            } else {
                Raster::Smooth
            };
            match source {
                ImageSource::Ready(data) => picture(data, &alt, size, fit, raster, corner),
                ImageSource::Loading => placeholder(&alt, size, corner, &style, false),
                ImageSource::Failed => placeholder(&alt, size, corner, &style, true),
            }
        },
    )?;
    Ok(box_item(picture))
}

#[derive(Clone, Copy)]
struct Size {
    width: f32,
    height: f32,
}

impl Size {
    fn sized(self, style: LayoutStyle) -> LayoutStyle {
        let style = if self.width > 0.0 {
            style.width(self.width)
        } else {
            style
        };
        if self.height > 0.0 {
            style.height(self.height)
        } else {
            style
        }
    }

    /// The box a picture not yet here stands in: the sides it was given, the one it was given on both sides, or a control's height square.
    fn standing_in(self) -> (f32, f32) {
        match (self.width > 0.0, self.height > 0.0) {
            (true, true) => (self.width, self.height),
            (true, false) => (self.width, self.width),
            (false, true) => (self.height, self.height),
            (false, false) => (shared::control_height(), shared::control_height()),
        }
    }
}

fn named(item: impl Accessible + 'static, alt: &Reactive<String>) -> Box<dyn LayoutItem> {
    if alt.get().is_empty() {
        box_item(item.a11y_hidden())
    } else {
        let alt = alt.clone();
        box_item(item.a11y_label(move || alt.get()))
    }
}

fn picture(
    data: Arc<ImageData>,
    alt: &Reactive<String>,
    size: Size,
    fit: ObjectFit,
    raster: Raster,
    corner: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let picture = telar::Image::new(
        size.sized(LayoutStyle::new().flex_shrink(0.0)),
        move || data.clone(),
        move || raster,
        move || fit,
    )?
    .with_radius(corner);
    Ok(named(picture, alt))
}

/// The box a picture stands in while it loads, quiet and of the size it will take; where it `failed`, with its words in it, as a page shows an image it could not fetch.
fn placeholder(
    alt: &Reactive<String>,
    size: Size,
    corner: f32,
    style: &SurfaceStyle,
    failed: bool,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (width, height) = size.standing_in();
    let content: Vec<Box<dyn LayoutItem>> = if failed && !alt.get().is_empty() {
        let words = {
            let alt = alt.clone();
            Text::declaring(
                move || alt.get(),
                LayoutStyle::new().min_width(0.0).flex_shrink(1.0),
                |t| shared::quiet(t, shared::CAPTION_RATIO).with_text_wrap(TextWrap::NoWrap),
            )?
            .a11y_hidden()
        };
        children![ClippedItem::new(box_item(words), Clip::both())]
    } else {
        Vec::new()
    };
    let style = style.clone();
    let stand_in = StyledContainer::new(
        LayoutStyle::row()
            .width(width)
            .height(height)
            .flex_shrink(0.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_horizontal(shared::gap_sm()),
        move |_r| {
            let quiet = RectStyle::default()
                .with_fill(shared::surface_alt())
                .with_radius(BorderRadius::all(corner));
            amend_surface(quiet, &style)
        },
        content,
    )?;
    Ok(named(stand_in, alt))
}

#[cfg(test)]
#[path = "image_test.rs"]
mod tests;
