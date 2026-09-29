use geometry_core::Rect;
use renderer_core::{ImageFill, Raster};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use super::*;

wasm_bindgen_test_configure!(run_in_browser);

/// One opaque red pixel.
const RED_DOT: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

fn frame(data: ImageData) -> Vec<DrawCommand> {
    vec![DrawCommand::Image {
        data: Arc::new(data),
        rect: Rect::new(0.0, 0.0, 10.0, 10.0),
        raster: Raster::Smooth,
        fill: ImageFill::Stretch,
    }]
}

async fn arrived(pictures: &Pictures, url: &str) {
    for _ in 0..200 {
        if matches!(
            pictures.arrivals.borrow().get(url),
            Some(Arrival::Ready(_) | Arrival::Failed)
        ) {
            return;
        }
        let tick = js_sys::Promise::new(&mut |resolve, _| {
            let _ = crate::dom_window()
                .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 10);
        });
        let _ = wasm_bindgen_futures::JsFuture::from(tick).await;
    }
    panic!("{url} never arrived");
}

#[wasm_bindgen_test]
fn a_frame_without_linked_pictures_is_passed_through() {
    let commands = frame(ImageData::new(vec![0, 0, 0, 255], 1, 1));
    assert!(matches!(
        Pictures::default().resolve(&commands),
        Cow::Borrowed(_)
    ));
}

#[wasm_bindgen_test]
async fn a_linked_picture_is_left_out_until_it_arrives_then_drawn_from_its_pixels() {
    let pictures = Pictures::default();
    let commands = frame(ImageData::linked(RED_DOT, 1, 1, &[]));
    assert!(
        pictures.resolve(&commands).is_empty(),
        "nothing to draw yet"
    );

    arrived(&pictures, RED_DOT).await;
    let resolved = pictures.resolve(&commands);
    let [DrawCommand::Image { data, .. }] = &resolved[..] else {
        panic!("the picture is drawn once it arrives: {resolved:?}");
    };
    assert_eq!((data.width, data.height), (1, 1));
    let [red, green, blue, alpha] = data.pixels() else {
        panic!("one pixel: {:?}", data.pixels());
    };
    // Within a step: a browser decodes through its own colour management.
    assert!(
        *red >= 250 && *green <= 5 && *blue <= 5 && *alpha == 255,
        "{:?}",
        data.pixels()
    );
}
