use super::*;
use telar_parser::parse;

fn color_at<'a>(infos: &'a [ColorInformation], src: &str, frag: &str) -> Option<&'a Color> {
    let byte = src.find(frag)?;
    let pos = offset_to_position(src, byte);
    infos
        .iter()
        .find(|i| i.range.start == pos)
        .map(|i| &i.color)
}

#[test]
fn view_hex_and_keyword_attrs_get_swatches_but_quoted_and_tokens_do_not() {
    let src =
        "[view]\nbox fill:#ff0000 stroke:transparent color:primary\n    text label:\"#nothex\"\n";
    let doc = parse(src).unwrap();
    let infos = document_colors(&doc, src);
    assert!(
        color_at(&infos, src, "#ff0000").is_some(),
        "hex attr swatch"
    );
    assert!(
        color_at(&infos, src, "transparent").is_some(),
        "keyword swatch"
    );
    assert!(
        color_at(&infos, src, "primary").is_none(),
        "token ref must not get a swatch"
    );
    assert!(
        color_at(&infos, src, "#nothex").is_none(),
        "quoted value must not get a swatch"
    );
}

#[test]
fn signal_color_reference_degrades_without_panic() {
    // `fill:$accent` (transpiler T-3.x) is a reactive read, not a literal; it must be silently skipped (no swatch), same as a theme token, rather than panicking on the `$` sigil.
    let src = "[view]\nbox fill:$accent stroke:$accent\n";
    let doc = parse(src).unwrap();
    let infos = document_colors(&doc, src);
    assert!(
        color_at(&infos, src, "$accent").is_none(),
        "a signal reference must not get a swatch"
    );
}

#[test]
fn presentation_round_trips_to_hex() {
    let opaque = color_presentations(rgba(67, 97, 238, 255));
    assert_eq!(opaque[0].label, "#4361ee");
    let translucent = color_presentations(rgba(0, 0, 0, 128));
    assert_eq!(translucent[0].label, "#00000080");
}

#[test]
fn class_prop_hex_gets_a_swatch() {
    let src = "[style]\n@card\n    fill: #ff8800\n    radius: 8\n[view]\ncol @card\n";
    let doc = parse(src).unwrap();
    let infos = document_colors(&doc, src);
    assert!(
        color_at(&infos, src, "#ff8800").is_some(),
        "class-prop fill should get a swatch"
    );
    assert_eq!(
        infos.len(),
        1,
        "non-color props must not get swatches: {infos:?}"
    );
}

#[test]
fn short_hex_expands() {
    let src = "[style]\n@card\n    fill: #f0a\n[view]\ncol @card\n";
    let doc = parse(src).unwrap();
    let infos = document_colors(&doc, src);
    let color = color_at(&infos, src, "#f0a").expect("short hex swatch");
    assert!((color.red - 1.0).abs() < 1e-6, "{}", color.red);
    assert_eq!(color.green, 0.0);
    assert!((color.blue - 170.0 / 255.0).abs() < 1e-6, "{}", color.blue);
}
