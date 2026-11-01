use super::*;

fn declared(toml: &str) -> FontDeclaration {
    toml::from_str::<telar_project::TelarManifest>(toml)
        .unwrap()
        .telar
        .fonts
        .remove(0)
}

#[test]
fn a_static_face_writes_one_weight_and_leaves_out_what_it_did_not_declare() {
    let font = declared("[[telar.fonts]]\nfamily = \"Mono\"\nsrc = \"m.woff2\"\nweight = 500\n");
    assert_eq!(
        font_face(&font, "./fonts/m-0.woff2"),
        "@font-face {\n  font-family: \"Mono\";\n  src: url(\"./fonts/m-0.woff2\") format(\"woff2\");\n  font-weight: 500;\n  font-style: normal;\n  font-display: swap;\n}\n"
    );
}

#[test]
fn a_variable_face_writes_its_ranges() {
    let font = declared(
        "[[telar.fonts]]\nfamily = \"V\"\nsrc = \"v.otf\"\naxes = { wght = [200, 800], wdth = [75, 100] }\ndisplay = \"optional\"\nsize_adjust = 0.9\n",
    );
    let rule = font_face(&font, "./v.otf");
    assert!(rule.contains("format(\"opentype\")"), "{rule}");
    assert!(rule.contains("font-weight: 200 800;"), "{rule}");
    assert!(rule.contains("font-stretch: 75% 100%;"), "{rule}");
    assert!(rule.contains("font-display: optional;"), "{rule}");
    assert!(rule.contains("size-adjust: 90%;"), "{rule}");
}

#[test]
fn a_family_cannot_close_the_style_element_or_the_string() {
    assert_eq!(css_string("a\"b\\c</style>"), "\"a\\\"b\\\\c\\3c /style>\"");
}
