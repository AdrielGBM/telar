use std::cell::RefCell;
use std::rc::Rc;

use reactive_core::effect;
use renderer_core::{Color, FontAsset, FontFamily, FontWeight, TextStyle};

use super::*;

const TEST_FACE: &[u8] = include_bytes!("../../../renderer/renderer-text/test-fonts/TelarTest.ttf");

/// What `opening_fit`-style code does by hand: measure a line in a face the page is still fetching. Measured in the fallback first, it has to measure again when the face lands, or it keeps the fallback's width for good.
#[test]
fn a_face_that_lands_runs_whatever_measured_text_again() {
    crate::context::reset_layout_runtime();
    let style = TextStyle::new(40.0, Color::BLACK).with_font_family(FontFamily::stack([
        FontFamily::Named("Telar Arriving Face".into()),
        FontFamily::Monospace,
    ]));
    let widths = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&widths);
    let _measuring = effect(move || {
        use_text_metrics_generation();
        seen.borrow_mut()
            .push(measure_text("Wide Words", None, 1.0e6, &style).0);
    });
    assert_eq!(widths.borrow().len(), 1);

    let generation = renderer_core::text_metrics_generation();
    renderer_text::fonts::add_faces(vec![
        FontAsset::embedded(TEST_FACE)
            .named("Telar Arriving Face")
            .with_weight(FontWeight::range(100, 900)),
    ]);
    assert!(renderer_core::text_metrics_generation() > generation);
    assert_eq!(
        widths.borrow().len(),
        1,
        "the generation moves on the next frame, not inside whatever added the face"
    );

    crate::context::relayout_if_dirty();
    let widths = widths.borrow();
    assert_eq!(
        widths.len(),
        2,
        "the frame after a face lands runs the effect again"
    );
    assert_ne!(
        widths[0], widths[1],
        "and it measures in the face that landed rather than the fallback"
    );
}
