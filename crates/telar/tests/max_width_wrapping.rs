//! Whether the max-width two-pass in `layout-reactive` still buys anything, measured against real text.
//!
//! `compute_layout` lifts every max-width box's width pin, lays out, pins each box to the width it resolved to, and lays out again. It was written against taffy 0.11, where a wrapping child inside a capped box was measured at taffy's uncapped one-line intrinsic estimate and came out one line tall however long the copy was. The unit test beside the pass already passes without it on taffy 0.13 — but with a synthetic measure function, which is not the case the workaround was written for.
//!
//! This is that case: a full-width page, a capped band inside it, and a paragraph long enough that its height is a question. It runs through the shaper the runner installs, so what is measured is what is drawn.

use telar::{
    AvailableSpace, Color, Container, LayoutItem, LayoutStyle, Text, TextStyle, box_item,
    compute_layout, new_container, track_layout,
};

const PAGE: f32 = 1200.0;
const BAND: f32 = 720.0;
const SIZE: f32 = 16.0;
const COPY: &str = "A page is as wide as the window and a band inside it is not: the band caps its own \
width, and the copy in the band wraps to whatever the cap left it. How tall the band ends up is therefore \
a question about the width it resolved to, which is the whole reason the layout runs twice.";

/// The band's laid-out rect, from a page the width of a window.
fn band(cap: Option<f32>) -> geometry_core::Rect {
    telar::reset_layout_runtime();
    let text = Text::new(
        || COPY.to_string(),
        LayoutStyle::new(),
        || TextStyle::new(SIZE, Color::BLACK),
    )
    .unwrap();
    let mut style = LayoutStyle::new().flex_column();
    if let Some(cap) = cap {
        style = style.max_width(cap);
    }
    let band = Container::new(style, vec![box_item(text)]).unwrap();
    let band_node = band.layout_node();
    let rect = track_layout(band_node).unwrap();
    let page = new_container(LayoutStyle::new().flex_column(), &[band_node]).unwrap();

    compute_layout(
        page,
        AvailableSpace::Definite(PAGE),
        AvailableSpace::Definite(800.0),
    )
    .unwrap();
    // Held to the end: dropping the container unregisters its node while the rect is still being read.
    let out = rect.get();
    drop(band);
    out
}

/// The premise everything below rests on: a capped band stops at its cap rather than filling the page.
#[test]
fn a_capped_band_is_as_wide_as_its_cap() {
    assert_eq!(band(Some(BAND)).width, BAND);
    assert_eq!(band(None).width, PAGE, "and an uncapped one fills it");
}

/// The bug the two-pass exists for. Copy measured at the page's width instead of the band's needs fewer lines and comes out shorter, so the two heights differ — and the capped one has to be the taller.
#[test]
fn a_capped_band_is_as_tall_as_its_copy_wrapped_at_the_cap() {
    let capped = band(Some(BAND));
    let uncapped = band(None);

    assert!(
        capped.height > SIZE * 2.0,
        "the copy wrapped past one line at {BAND}px, got {}",
        capped.height
    );
    assert!(
        capped.height > uncapped.height,
        "wrapping at {BAND}px takes more lines than wrapping at {PAGE}px: {} vs {}",
        capped.height,
        uncapped.height
    );
}
