//! [`image`]: a picture and the words that stand for it, with a quiet placeholder while it loads and the words themselves where it could not.

use std::sync::Arc;

use telar::{
    Accessible, AlignItems, BorderRadius, Clip, ClippedItem, ImageData, JustifyContent,
    LayoutError, LayoutItem, LayoutStyle, ObjectFit, Props, Raster, Reactive, RectStyle,
    ShapeStyle, SizeDimension, StyledContainer, SurfaceAmendment, SurfaceStyle, Text, TextWrap,
    amend_surface, box_item, children,
};

use crate::shared;
use telar::ControlSize;

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
/// A decoded picture converts into one, and so does the state a `telar::AssetLoader` hands out under the `async-assets` feature, so an image follows a load by being given the loader's signal.
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
            Self::Ready(data) => (1, data.id()),
            Self::Failed => (2, 0),
        }
    }
}

/// A picture, named for a reader by `alt`: what it shows, in words, or nothing at all for a picture that only decorates, which a reader then steps over.
///
/// While the picture loads it is a quiet box of the size it will take; where it cannot be had, that box shows `alt` instead. Its own size is the picture's, or a control's height square while there is no picture yet, and a column that stretches what it holds leaves it at that size. A caller placing `width:` or `height:` alone has the other worked out from the picture's proportions. Place its size before the picture arrives and the page does not move when it does.
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
pub fn image(props: ImageProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ImageProps {
        source,
        alt,
        fit,
        radius,
        pixelated,
        style,
    } = props;
    let corner = move || radius.unwrap_or_else(|| ImageTokens::current().radius());
    let shape = source.clone();
    let picture = telar::ReactiveList::with_style(
        own_box(&shape.get()),
        move || vec![(source.get(), corner().to_bits())],
        |(source, corner): &(ImageSource, u32)| (source.key(), *corner),
        move |(source, corner)| {
            let corner = f32::from_bits(corner);
            let natural = natural_size(&source);
            let raster = if pixelated {
                Raster::Pixel
            } else {
                Raster::Smooth
            };
            match source {
                ImageSource::Ready(data) => picture(data, &alt, fit, raster, corner, natural),
                ImageSource::Loading => placeholder(&alt, corner, &style, false, natural),
                ImageSource::Failed => placeholder(&alt, corner, &style, true, natural),
            }
        },
    )?;
    telar::style_follows(picture.layout_node(), move || own_box(&shape.get()));
    Ok(box_item(picture))
}

/// The size an image is when nothing places it: the picture's own, or a control's height square while there is none.
fn natural_size(source: &ImageSource) -> (f32, f32) {
    match source {
        ImageSource::Ready(data) if data.width() > 0 && data.height() > 0 => {
            (data.width() as f32, data.height() as f32)
        }
        _ => {
            let side = shared::control_height(ControlSize::Regular);
            (side, side)
        }
    }
}

/// The image's box: as big as what [`filling`] gives it, with the proportions that make the other side of a single side a caller places.
fn own_box(source: &ImageSource) -> LayoutStyle {
    let (width, height) = natural_size(source);
    LayoutStyle::row()
        .flex_shrink(0.0)
        .width(SizeDimension::Content)
        .height(SizeDimension::Content)
        .aspect_ratio(width / height)
}

/// What fills the image's box: the box's size once it has one, and `natural` while the box is still asking what it holds how big it is, when a fraction of it means nothing.
fn filling((width, height): (f32, f32)) -> LayoutStyle {
    let whole = SizeDimension::Percent(1.0);
    LayoutStyle::new()
        .width(width)
        .height(height)
        .min_width(whole)
        .min_height(whole)
        .max_width(whole)
        .max_height(whole)
}

fn named(item: impl Accessible + 'static, alt: &Reactive<String>) -> Box<dyn LayoutItem> {
    let alt = alt.clone();
    box_item(item.a11y_decorative().a11y_label(move || alt.get()))
}

fn picture(
    data: Arc<ImageData>,
    alt: &Reactive<String>,
    fit: ObjectFit,
    raster: Raster,
    corner: f32,
    natural: (f32, f32),
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let picture = telar::Image::new(
        filling(natural),
        move || data.clone(),
        move || raster,
        move || fit,
    )?
    .with_radius(corner);
    Ok(named(picture, alt))
}

/// The box a picture stands in while it loads, quiet and filling the image; where it `failed`, with its words in it, as a page shows an image it could not fetch.
fn placeholder(
    alt: &Reactive<String>,
    corner: f32,
    style: &SurfaceStyle,
    failed: bool,
    natural: (f32, f32),
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let content: Vec<Box<dyn LayoutItem>> = if failed && !alt.get().is_empty() {
        let words = {
            let alt = alt.clone();
            Text::declaring(
                move || alt.get(),
                LayoutStyle::new().min_width(0.0).flex_shrink(1.0),
                |t| {
                    shared::quiet(ControlSize::Regular, t, shared::CAPTION_RATIO)
                        .with_text_wrap(TextWrap::NoWrap)
                },
            )?
            .a11y_hidden()
        };
        children![ClippedItem::new(box_item(words), Clip::both())]
    } else {
        Vec::new()
    };
    let style = style.clone();
    let stand_in = StyledContainer::new(
        filling(natural)
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_horizontal(shared::gap_sm(ControlSize::Regular)),
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
#[path = "image.test.rs"]
mod tests;
