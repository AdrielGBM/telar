use super::measure_text;

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
