use super::cache::{hash_text, text_style_bits};
use super::*;
use geometry_core::Rect;
use renderer_core::{Color, TextStyle};

// Text positions are logical, so a multi-line block must occupy the same vertical extent regardless of the device scale factor. cosmic-text's `physical` adds the y-offset unscaled, so passing the line baseline there unscaled collapsed every line onto the first one at high-DPI (e.g. Android).
#[test]
fn line_layout_is_scale_independent() {
    let mut sh = TextShaper::new();
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa";
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 90.0,
        height: 100000.0,
    };
    let style = TextStyle::new(20.0, renderer_core::Color::rgba(0.0, 0.0, 0.0, 1.0));
    let bottom_extent = |sh: &mut TextShaper, sf: f32| -> f32 {
        let mut out = Vec::new();
        sh.layout_glyphs(text, rect, &style, sf, &mut out);
        out.iter().map(|g| g.dest_rect[1]).fold(0.0_f32, f32::max)
    };
    let at_1x = bottom_extent(&mut sh, 1.0);
    let at_3x = bottom_extent(&mut sh, 3.0);
    assert!(at_1x > 40.0, "text did not wrap to multiple lines: {at_1x}");
    assert!(
        (at_1x - at_3x).abs() < 5.0,
        "line layout collapsed at high DPI: 1x bottom={at_1x}, 3x bottom={at_3x}"
    );
}

// A larger line_height must measure to a taller box so multi-line text reserves the extra vertical space. Guarded on a real measured height so a font-less CI machine can't turn a vacuous 0==0 into a false pass.
#[test]
fn larger_line_height_increases_measured_height() {
    let mut sh = TextShaper::new();
    let text = "Line height affects the reserved vertical space";
    let base = TextStyle::new(16.0, Color::BLACK);
    let (_, h_natural) = sh.measure_text(text, 120.0, &base);
    if h_natural <= 0.0 {
        return;
    }
    let (_, h_tall) = sh.measure_text(text, 120.0, &base.with_line_height(2.5));
    assert!(
        h_tall > h_natural,
        "a larger line_height should increase measured height: natural={h_natural} tall={h_tall}"
    );
}

// Default spacing keeps the exact packed style bits (so existing keys and the byte-golden are untouched), while any non-default line_height or letter_spacing yields distinct bits so cached measures/rasters aren't reused across spacing. Pure bit math: font-independent.
#[test]
fn text_style_bits_default_unchanged_and_spacing_perturbs() {
    let base = TextStyle::new(16.0, Color::BLACK);
    // The packed layout for a plain 400-weight style is exactly the weight value.
    assert_eq!(text_style_bits(&base), 400);
    let bits_default = text_style_bits(&base);
    let bits_lh = text_style_bits(&base.with_line_height(1.5));
    let bits_ls = text_style_bits(&base.with_letter_spacing(2.0));
    assert_ne!(bits_lh, bits_default, "line_height must perturb the bits");
    assert_ne!(
        bits_ls, bits_default,
        "letter_spacing must perturb the bits"
    );
    assert_ne!(bits_lh, bits_ls, "the two axes must not alias each other");
}

#[test]
fn text_shaper_new_does_not_panic() {
    let _ = TextShaper::new();
}

#[test]
fn text_shaper_with_empty_config() {
    let _ = TextShaper::with_config(TextShaperConfig::default());
}

#[test]
fn text_shaper_with_font_data_empty_vec() {
    let config = TextShaperConfig {
        font: renderer_core::FontConfig {
            font_data: vec![],
            extra_font_paths: vec![],
            system_fonts_dir: None,
            sans_serif_family_candidates: Vec::new(),
        },
        ..TextShaperConfig::default()
    };
    let _ = TextShaper::with_config(config);
}

#[test]
#[ignore]
fn measure_text_returns_nonzero_for_text() {
    let mut shaper = TextShaper::new();
    let (w, h) = shaper.measure_text("hello", 500.0, &TextStyle::new(16.0, Color::BLACK));
    assert!(
        w > 0.0 && h > 0.0,
        "Systems without installed fonts may not render text correctly"
    );
}

#[test]
fn measure_text_empty_returns_zero() {
    let mut shaper = TextShaper::new();
    let (w, h) = shaper.measure_text("", 500.0, &TextStyle::new(16.0, Color::BLACK));
    assert_eq!(w, 0.0);
    assert_eq!(h, 0.0);
}

#[test]
fn measure_cache_keeps_hot_entry_past_cap() {
    // The old policy cleared the whole cache at 1000 entries, evicting even constantly-used keys. The LRU must keep a re-touched "hot" entry alive while cold ones flood past the cap. Pure cache bookkeeping: independent of whether fonts are installed.
    let mut sh = TextShaper::new();
    let hot = "hot text that stays warm";
    let style = TextStyle::new(16.0, Color::BLACK);
    let hot_key = (
        hash_text(hot),
        200.0f32.to_bits(),
        16.0f32.to_bits(),
        text_style_bits(&style),
    );
    sh.measure_text(hot, 200.0, &style);
    for i in 0..(MEASURE_CACHE_CAP as u32 + 50) {
        sh.measure_text(&format!("cold entry {i}"), 200.0, &style);
        // Keep the hot entry most-recently-used so the LRU never evicts it.
        sh.measure_text(hot, 200.0, &style);
    }
    assert!(sh.measure_cache.contains(&hot_key));
    assert!(sh.measure_cache.len() <= MEASURE_CACHE_CAP);
}

#[test]
fn collect_colr_gating_records_and_skips() {
    // The first call records a has-COLR flag for (text, font_size); a later call with a cached `false` must short-circuit (no re-shaping, empty result). Both halves are font-independent: the flag VALUE depends on installed fonts, but that a flag is recorded and that a false flag skips collection do not.
    let mut sh = TextShaper::new();
    let text = "ui label";
    let style = TextStyle::new(16.0, Color::BLACK);
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 100.0,
    };
    let flag_key = (hash_text(text), 16.0f32.to_bits(), text_style_bits(&style));

    assert!(sh.has_colr_cache.peek(&flag_key).is_none());
    let mut out = Vec::new();
    sh.collect_colr_glyphs(text, rect, &style, &mut out);
    assert!(
        sh.has_colr_cache.peek(&flag_key).is_some(),
        "first call must record the COLR flag"
    );

    // Force the "no COLR glyphs" flag and confirm the next call short-circuits to an empty result.
    sh.has_colr_cache.put(flag_key, false);
    let mut out2 = Vec::new();
    sh.collect_colr_glyphs(text, rect, &style, &mut out2);
    assert!(
        out2.is_empty(),
        "a cached false flag must skip COLR collection"
    );
}

// The clock case: a string rasterized once and never asked for again must not occupy the cache. Admission is what separates "drawn" from "worth keeping", and only a second sighting proves the difference.
#[test]
fn a_string_seen_once_is_not_admitted_and_a_second_sighting_admits_it() {
    let mut shaper = TextShaper::new();
    assert!(!shaper.admit(1), "first sighting must not be cached");
    assert!(shaper.admit(1), "second sighting earns a place");
}

#[test]
fn admission_tracks_each_key_on_its_own() {
    let mut shaper = TextShaper::new();
    assert!(!shaper.admit(1));
    assert!(!shaper.admit(2));
    assert!(shaper.admit(2));
    assert!(shaper.admit(1));
}

// Admission is consumed, not remembered: once a key is in the real cache it must not keep a slot in the seen-once table too, or the table fills with keys that will never be asked about again.
#[test]
fn admitting_a_key_clears_it_from_the_pending_table() {
    let mut shaper = TextShaper::new();
    shaper.admit(7);
    assert!(shaper.admit(7));
    assert!(!shaper.admit(7), "admission starts over after being spent");
}
