use super::*;
use renderer_core::Color;

/// Why the reference is a flat capital: every flat capital inks one band, while a round one overshoots it at both ends.
#[test]
fn flat_capitals_ink_one_band_from_the_baseline_to_the_cap_height() {
    renderer_text::fonts::installed();
    renderer_text::fonts::add_faces(vec![
        renderer_core::FontAsset::embedded(include_bytes!(
            "../../../renderer/renderer-text/test-fonts/TelarTest.ttf"
        ))
        .named("Telar Centred"),
    ]);
    let style = TextStyle::new(96.0, Color::BLACK)
        .with_font_family(renderer_core::FontFamily::Named("Telar Centred".into()));
    let ink = |text: &str| crate::text_metrics::measure_ink_bounds(text, 1_000.0, &style);
    let (cap_top, cap_height) = ink(REFERENCE);
    assert!(cap_height > 0.0, "the face inks a capital");
    for flat in ["E", "I", "T", "HEIT"] {
        assert_eq!(
            ink(flat),
            (cap_top, cap_height),
            "{flat} inks the band H does"
        );
    }
    let (round_top, round_height) = ink("O");
    assert!(
        round_top < cap_top && round_top + round_height > cap_top + cap_height,
        "O overshoots the band H inks at both ends: {round_top}+{round_height} against {cap_top}+{cap_height}"
    );
}
