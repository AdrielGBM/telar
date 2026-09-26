use super::{measure_min_content, measure_text};

// Constraining a box to its own measured width must not push the text onto an extra line, so the measured width must cover the line's full advance.
#[test]
fn measured_width_does_not_rewrap() {
    let style = renderer_core::TextStyle::new(14.0, renderer_core::Color::BLACK);
    let (w, h_unbounded) = measure_text("Features", None, 1.0e6, &style);
    let (_, h_at_measured) = measure_text("Features", None, w, &style);
    assert!(
        (h_unbounded - h_at_measured).abs() < 0.5,
        "box at measured width re-wrapped: w={w} h0={h_unbounded} h1={h_at_measured}"
    );
}

/// A box squeezed to a text's min-content width holds each word whole, one to a line; only a box narrower than that breaks a word, which is why min-content cannot be asked as a measure at width zero.
#[test]
fn min_content_is_the_longest_word_and_keeps_every_word_whole() {
    let style = renderer_core::TextStyle::new(16.0, renderer_core::Color::BLACK);
    let (overview, line) = measure_text("Overview", None, 1.0e6, &style);
    let (pricing, _) = measure_text("Pricing", None, 1.0e6, &style);

    let (width, height) = measure_min_content("Overview Pricing", None, &style);
    assert_eq!(width, overview.max(pricing));
    assert_eq!(height, 2.0 * line, "one whole word to a line");

    let (_, squeezed) = measure_text("Overview Pricing", None, width / 2.0, &style);
    assert!(
        squeezed > height,
        "narrower than min-content, a word has to break: {squeezed} vs {height}"
    );
}
