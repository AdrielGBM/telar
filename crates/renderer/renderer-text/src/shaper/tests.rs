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

// `max_lines` must cut the shaped paragraph to that many visual lines, and `ellipsis` must mark the cut with
// `…` rather than dropping the tail in silence. Every clamped label in the tree rests on this and nothing
// asserted it: the only test that named ellipsis was a PNG utility that never runs. Guarded on text that
// really wrapped past the clamp, so a font-less machine skips instead of passing vacuously.
#[test]
fn max_lines_cuts_the_shaped_lines_and_ellipsis_marks_the_cut() {
    let mut sh = TextShaper::new();
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi";
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 90.0,
        height: 10_000.0,
    };
    let base = TextStyle::new(16.0, Color::BLACK);
    let shaped = |sh: &mut TextShaper, style: &TextStyle| {
        make_buffer(&mut sh.font_system, text, rect, style)
    };

    if shaped(&mut sh, &base).layout_runs().count() <= 2 {
        return;
    }

    let clamped = shaped(&mut sh, &base.clone().with_max_lines(2));
    assert_eq!(
        clamped.layout_runs().count(),
        2,
        "max_lines(2) must leave two visual lines"
    );

    let elided = shaped(&mut sh, &base.clone().with_max_lines(2).with_ellipsis(true));
    assert_eq!(
        elided.layout_runs().count(),
        2,
        "the ellipsis must fit inside the clamp, not push a third line"
    );
    let elided_text: String = elided.lines.iter().map(|line| line.text()).collect();
    assert!(
        elided_text.ends_with('\u{2026}'),
        "a clamped label with ellipsis must end in `…`: {elided_text:?}"
    );
}

// The clamp has to reach `measure` too: a label reserves the box `measure_text` reports, so a height measured
// as if the dropped lines were still there leaves a hole under every truncated label.
#[test]
fn max_lines_clamps_the_measured_height() {
    let mut sh = TextShaper::new();
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi";
    let base = TextStyle::new(16.0, Color::BLACK);
    let (_, wrapped) = sh.measure_text(text, 90.0, &base);
    let (_, single) = sh.measure_text("alpha", 90.0, &base);
    if single <= 0.0 || wrapped <= single * 2.0 {
        return;
    }
    let (_, clamped) = sh.measure_text(text, 90.0, &base.with_max_lines(2));
    assert!(
        clamped < wrapped,
        "max_lines must shrink the measured height: wrapped={wrapped} clamped={clamped}"
    );
    assert!(
        clamped <= single * 2.5,
        "two clamped lines should measure about two lines tall: single={single} clamped={clamped}"
    );
}

// Default spacing keeps the exact packed style bits (so existing keys and the byte-golden are untouched), while any non-default line_height or letter_spacing yields distinct bits so cached measures/rasters aren't reused across spacing. Pure bit math: font-independent.
#[test]
fn text_style_bits_default_unchanged_and_spacing_perturbs() {
    let base = TextStyle::new(16.0, Color::BLACK);
    // The packed layout for a plain 400-weight style is exactly the weight value.
    assert_eq!(text_style_bits(&base), 400);
    let bits_default = text_style_bits(&base);
    let bits_lh = text_style_bits(&base.clone().with_line_height(1.5));
    let bits_ls = text_style_bits(&base.clone().with_letter_spacing(2.0));
    assert_ne!(bits_lh, bits_default, "line_height must perturb the bits");
    assert_ne!(
        bits_ls, bits_default,
        "letter_spacing must perturb the bits"
    );
    assert_ne!(bits_lh, bits_ls, "the two axes must not alias each other");
    let bits_pixel = text_style_bits(&base.clone().with_raster(GlyphRaster::Pixel));
    assert_ne!(
        bits_pixel, bits_default,
        "the raster grid must perturb the bits, or a smooth raster is served to a pixel style"
    );
    // Two families are two sets of glyphs for one string: a shared key serves one face's raster for the other.
    let bits_family = text_style_bits(&base.clone().with_font_family("LanaPixel"));
    let bits_other = text_style_bits(&base.clone().with_font_family("Inter"));
    assert_ne!(
        bits_family, bits_default,
        "a named family must perturb the bits"
    );
    assert_ne!(bits_family, bits_other, "two families must not share a key");
}

// Coverage under the pixel raster is ink or nothing: a glyph half-covering a pixel is the grid the artist drew being taken apart. Skipped rather than failed on a font-less machine, where nothing is inked at all.
#[test]
fn pixel_raster_leaves_no_partial_coverage() {
    let mut sh = TextShaper::new();
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 40.0,
    };
    let base = TextStyle::new(16.0, Color::WHITE);
    let (smooth, _, _) = sh.rasterize("Hamburgefonstiv", rect, &base);
    let partial = |pixels: &[u8]| {
        pixels
            .chunks_exact(4)
            .filter(|px| px[3] > 0 && px[3] < 255)
            .count()
    };
    if partial(&smooth) == 0 {
        return;
    }
    let (pixel, _, _) = sh.rasterize(
        "Hamburgefonstiv",
        rect,
        &base.with_raster(GlyphRaster::Pixel),
    );
    assert_eq!(
        partial(&pixel),
        0,
        "pixel raster must resolve every glyph pixel to on or off"
    );
}

// The other half of the grid, and the half that does not show in a `PhysicalGlyph`'s integer coordinates: the fractional part of a glyph's origin rides in its cache key as a quarter-pixel bin the rasterizer bakes into the image. Rounding before the binning is what collapses those four rasters into one, so the same glyph half a pixel further along is that glyph moved rather than a differently-offset one.
#[test]
fn pixel_raster_collapses_the_subpixel_bins_smooth_keeps() {
    let mut sh = TextShaper::new();
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 40.0,
    };
    let style = TextStyle::new(16.0, Color::WHITE);
    let buffer = make_buffer(&mut sh.font_system, "Hi", rect, &style);
    let Some(glyph) = buffer
        .layout_runs()
        .flat_map(|run| run.glyphs.iter())
        .next()
    else {
        return;
    };
    let key_at = |dx: f32, raster| physical_glyph(glyph, (dx, 0.0), 1.0, raster).cache_key;
    assert_eq!(
        key_at(0.0, GlyphRaster::Pixel),
        key_at(0.5, GlyphRaster::Pixel),
        "a half-pixel shift must not mint a second raster of the same glyph"
    );
    assert_ne!(
        key_at(0.0, GlyphRaster::Smooth),
        key_at(0.5, GlyphRaster::Smooth),
        "the smooth raster is supposed to keep subpixel positions; this test proves nothing without it"
    );
}

// A pixel-raster glyph must not land in the atlas slot of the smooth raster of the same glyph at the same size — the two are different pictures, and the atlas is keyed by cosmic-text's `CacheKey`. Shaping with `PIXEL_FONT` is what keeps them apart, so it has to reach the shaped glyphs.
#[test]
fn pixel_raster_shapes_under_its_own_cache_key() {
    let mut sh = TextShaper::new();
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 40.0,
    };
    let base = TextStyle::new(16.0, Color::WHITE);
    let flags = |style: &TextStyle, sh: &mut TextShaper| {
        make_buffer(&mut sh.font_system, "Hi", rect, style)
            .layout_runs()
            .flat_map(|run| run.glyphs.iter())
            .next()
            .map(|glyph| glyph.cache_key_flags)
    };
    let Some(smooth) = flags(&base, &mut sh) else {
        return;
    };
    let pixel = flags(&base.with_raster(GlyphRaster::Pixel), &mut sh).expect("same string shapes");
    assert!(!smooth.contains(CacheKeyFlags::PIXEL_FONT));
    assert!(pixel.contains(CacheKeyFlags::PIXEL_FONT));
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
    let cap_entries = limits::TEXT_MEASURE.capacity / limits::SMALL_ENTRY_BYTES;
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
    for i in 0..(cap_entries as u32 + 50) {
        sh.measure_text(&format!("cold entry {i}"), 200.0, &style);
        // Keep the hot entry most-recently-used so the LRU never evicts it.
        sh.measure_text(hot, 200.0, &style);
    }
    assert!(sh.measure_cache.contains(&hot_key));
    assert!(sh.measure_cache.len() <= cap_entries);
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

    assert!(!sh.has_colr_cache.contains(&flag_key));
    let mut out = Vec::new();
    sh.collect_colr_glyphs(text, rect, &style, &mut out);
    assert!(
        sh.has_colr_cache.contains(&flag_key),
        "first call must record the COLR flag"
    );

    // Force the "no COLR glyphs" flag and confirm the next call short-circuits to an empty result.
    sh.has_colr_cache.insert(flag_key, false);
    let mut out2 = Vec::new();
    sh.collect_colr_glyphs(text, rect, &style, &mut out2);
    assert!(
        out2.is_empty(),
        "a cached false flag must skip COLR collection"
    );
}

// The clock case, end to end: admission itself is covered in `renderer-cache`, so what matters here is that
// `rasterize` is actually wired to it — the previous arrangement applied admission in the shaper and then had the
// software backend keep an unconditional second copy, which made the policy a no-op where it counted.
#[test]
fn a_string_rasterized_once_is_not_kept_and_a_second_sighting_keeps_it() {
    let mut shaper = TextShaper::new();
    let style = TextStyle::new(16.0, Color::BLACK);
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 120.0,
        height: 24.0,
    };

    shaper.rasterize("14:32:07", rect, &style);
    assert_eq!(
        shaper.raster_cache.len(),
        0,
        "a clock's seconds are drawn once and never asked for again"
    );

    shaper.rasterize("14:32:07", rect, &style);
    assert_eq!(
        shaper.raster_cache.len(),
        1,
        "a string asked for twice is the only kind a cache can serve"
    );
}

// Trimming the glyph rasters is all-or-nothing, so the risk is not that it fails to fire but that it fires when it
// should not: clearing a shell's working set costs re-rasterizing every glyph still on screen.
#[test]
fn an_idle_sweep_keeps_the_glyph_rasters_a_shell_actually_uses() {
    let mut sh = TextShaper::new();
    let style = TextStyle::new(16.0, Color::BLACK);
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 24.0,
    };
    sh.rasterize("workspace 1", rect, &style);
    let resident = sh.swash_cache.image_cache.len();

    sh.sweep_idle();

    assert_eq!(
        sh.swash_cache.image_cache.len(),
        resident,
        "a glyph cache well under its ceiling must survive the sweep"
    );
}

// The shaping cache takes no admission: its entries are a couple of hundred bytes, so the budget bounds them without
// help, and making a string shape twice to save those bytes would trade the expensive half of drawing text for the
// cheap half.
#[test]
fn shaped_positions_are_kept_on_the_first_sighting() {
    let mut shaper = TextShaper::new();
    let style = TextStyle::new(16.0, Color::BLACK);
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 24.0,
    };
    let mut out = Vec::new();

    shaper.layout_glyphs("ui label", rect, &style, 1.0, &mut out);
    assert_eq!(shaper.shaping_cache.len(), 1);
}

// N lines measure N line heights. The box used to be anchored on the last line's *baseline*, which counted the ascent on top of the line height and left every text block reserving a constant slab it never drew into — nearly half the box at UI sizes. Read as a ratio so it holds for whatever face the machine resolves, and skipped where no font draws at all.
#[test]
fn a_text_box_measures_its_line_heights_and_nothing_more() {
    let mut sh = TextShaper::new();
    let style = TextStyle::new(16.0, Color::BLACK).with_line_height(1.5);
    let line_height = 16.0 * 1.5;
    let (_, one) = sh.measure_text("Una línea", 400.0, &style);
    if one <= 0.0 {
        return;
    }
    assert!(
        (one - line_height).abs() < 0.01,
        "one line should measure one line height ({line_height}), not {one}"
    );

    // A width narrow enough to force a wrap, so the second line's contribution is the line height too.
    let (_, two) = sh.measure_text("Una línea que no cabe entera", 60.0, &style);
    let lines = (two / line_height).round();
    assert!(lines >= 2.0, "the text did not wrap: {two}");
    assert!(
        (two - lines * line_height).abs() < 0.01,
        "{lines} lines should measure {} , not {two}",
        lines * line_height
    );
}

/// A label that is a token, not prose: it keeps one line whatever width the box offers, and measures the
/// width it actually needs. Wrapping it is how a status bar turns "object mode" into two stacked words.
#[test]
fn a_no_wrap_style_measures_one_line_however_narrow_the_box() {
    let style = TextStyle::new(14.0, Color::BLACK);
    let text = "Frame selected";
    let (wrapped_w, wrapped_h) = crate::measure_text(text, 40.0, &style);
    let (flat_w, flat_h) = crate::measure_text(text, 40.0, &style.clone().with_no_wrap(true));
    assert!(
        flat_h < wrapped_h,
        "wrapping stacks lines ({wrapped_h}) that no-wrap does not ({flat_h})"
    );
    assert!(
        flat_w > wrapped_w,
        "and it reports the width it needs ({flat_w}) rather than the box it was offered ({wrapped_w})"
    );
}

// Driven off two families the machine actually has, so this tests face selection rather than font installation.
#[test]
fn a_named_family_shapes_in_that_face() {
    let mut sh = TextShaper::new();
    let mut names: Vec<String> = sh
        .font_system
        .db()
        .faces()
        .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
        .collect();
    names.sort();
    names.dedup();
    let [first, second] = &names[..] else {
        return;
    };

    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 10_000.0,
        height: 1000.0,
    };
    let face_of = |sh: &mut TextShaper, family: &str| {
        let style = TextStyle::new(16.0, Color::BLACK).with_font_family(family);
        let buffer = make_buffer(&mut sh.font_system, "Ag", rect, &style);
        buffer
            .layout_runs()
            .next()
            .and_then(|run| run.glyphs.first().map(|g| g.font_id))
    };
    assert_ne!(
        face_of(&mut sh, first),
        face_of(&mut sh, second),
        "two different families must shape in two different faces"
    );
}
