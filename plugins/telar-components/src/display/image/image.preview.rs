use std::sync::Arc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, by_text, preview};
use telar::{
    Container, ImageData, LayoutError, LayoutItem, LayoutStyle, ObjectFit, Role, place_item,
};

use crate::display::image::image::{ImageProps, ImageSource, image};

/// A sunset in bands, worked out rather than loaded, so the preview needs no file.
fn sunset() -> Arc<ImageData> {
    let (width, height) = (96u32, 64u32);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let t = y as f32 / height as f32;
            let sun = ((x as f32 - 60.0).powi(2) + (y as f32 - 40.0).powi(2)).sqrt() < 12.0;
            let (r, g, b) = if sun {
                (255.0, 214.0, 102.0)
            } else if t < 0.62 {
                (40.0 + 200.0 * t, 60.0 + 80.0 * t, 140.0 - 60.0 * t)
            } else {
                (30.0, 40.0 + 30.0 * (1.0 - t), 70.0)
            };
            pixels.extend([r as u8, g as u8, b as u8, 255]);
        }
    }
    Arc::new(ImageData::new(pixels, width, height).expect("the loop writes every pixel"))
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(image: ImageProps, "States", |p| {
            let width = p.arg("width", 144.0f32);
            let pictures = vec![
                placed(
                    image(
                        ImageProps::props()
                            .source(ImageSource::Ready(sunset()))
                            .alt(p.arg("alt", "A sunset over the sea"))
                            .fit(ObjectFit::Cover)
                            .build(),
                    )?,
                    move |style| style.width(width).height(96.0),
                ),
                placed(
                    image(
                        ImageProps::props()
                            .source(ImageSource::Loading)
                            .alt("Loading picture")
                            .build(),
                    )?,
                    |style| style.width(96.0).height(96.0),
                ),
                placed(
                    image(
                        ImageProps::props()
                            .source(ImageSource::Failed)
                            .alt("Chart of sales")
                            .build(),
                    )?,
                    |style| style.width(144.0).height(96.0),
                ),
            ];
            Ok::<_, LayoutError>(Container::new(LayoutStyle::row().gap(16.0), pictures)?)
        })
        .title("Display/Image")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.expect(by_role(Role::Drawing).named("A sunset over the sea"))?;
            canvas.expect_text("Chart of sales")?;
            canvas.expect(by_text("Chart of sales"))
        }),
    ]
}

fn placed(
    item: Box<dyn LayoutItem>,
    place: impl Fn(LayoutStyle) -> LayoutStyle + 'static,
) -> Box<dyn LayoutItem> {
    place_item(item.layout_node(), place);
    item
}
