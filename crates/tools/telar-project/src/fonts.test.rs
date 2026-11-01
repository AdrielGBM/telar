use crate::TelarManifest;

use super::*;

fn parse(toml: &str) -> Vec<FontDeclaration> {
    toml::from_str::<TelarManifest>(toml).unwrap().telar.fonts
}

fn declared(extra: &str) -> FontDeclaration {
    parse(&format!(
        "[[telar.fonts]]\nfamily = \"Display\"\nsrc = \"assets/fonts/Display.ttf\"\n{extra}"
    ))
    .remove(0)
}

#[test]
fn every_key_of_a_face_round_trips() {
    let fonts = parse(
        r#"
        [[telar.fonts]]
        family = "Display"
        src = "assets/fonts/Display.ttf"
        weight = [100, 900]
        style = "italic"
        axes = { wght = [100, 900], wdth = [75, 125] }
        display = "optional"
        size_adjust = 1.05

        [[telar.fonts]]
        family = "Mono"
        src = "assets/fonts/Mono.woff2"
        weight = 500
        "#,
    );
    assert_eq!(fonts.len(), 2);
    let display = &fonts[0];
    assert_eq!(display.weight_range(), (100, 900));
    assert_eq!(display.style, FontStyleDeclaration::Italic);
    assert_eq!(display.stretch_range(), Some((75.0, 125.0)));
    assert_eq!(display.display, FontDisplay::Optional);
    assert_eq!(display.size_adjust, Some(1.05));
    assert_eq!(display.format(), FontFormat::TrueType);
    assert_eq!(
        display.axis_tags().collect::<Vec<_>>(),
        vec![(*b"wdth", 75.0, 125.0), (*b"wght", 100.0, 900.0)]
    );
    assert!(display.problems().is_empty());

    let mono = &fonts[1];
    assert_eq!(mono.weight_range(), (500, 500));
    assert_eq!(mono.style, FontStyleDeclaration::Normal);
    assert_eq!(mono.display, FontDisplay::Swap);
    assert!(!mono.format().is_sfnt());
}

#[test]
fn the_weight_defaults_to_the_wght_axis_and_then_to_regular() {
    assert_eq!(
        declared("axes = { wght = [300, 700] }").weight_range(),
        (300, 700)
    );
    assert_eq!(declared("").weight_range(), (400, 400));
}

#[test]
fn a_misspelled_key_in_a_face_is_refused() {
    let error = toml::from_str::<TelarManifest>(
        "[[telar.fonts]]\nfamily = \"A\"\nsrc = \"a.ttf\"\nwieght = 400\n",
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("wieght"), "{error}");
}

#[test]
fn what_parses_but_cannot_be_a_face_is_named() {
    assert!(declared("weight = [900, 100]").problems()[0].contains("weight"));
    assert!(declared("weight = 0").problems()[0].contains("weight"));
    assert!(declared("axes = { weight = [1, 2] }").problems()[0].contains("axis tag"));
    assert!(declared("axes = { wdth = [120, 80] }").problems()[0].contains("low to high"));
    assert!(declared("size_adjust = 0.0").problems()[0].contains("size_adjust"));
    let unknown = parse("[[telar.fonts]]\nfamily = \"A\"\nsrc = \"a.svg\"\n").remove(0);
    assert!(unknown.problems()[0].contains(".woff2"));
    let nameless = parse("[[telar.fonts]]\nfamily = \" \"\nsrc = \"a.ttf\"\n").remove(0);
    assert!(nameless.problems()[0].contains("empty `family`"));
}

#[test]
fn a_manifest_declaring_a_broken_face_does_not_load() {
    let root = std::env::temp_dir().join(format!("telar_fonts_broken_{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("telar.toml"),
        "[[telar.fonts]]\nfamily = \"A\"\nsrc = \"a.ttf\"\nweight = [700, 400]\n",
    )
    .unwrap();
    let error = TelarManifest::load(&root).unwrap_err().to_string();
    let _ = std::fs::remove_dir_all(&root);
    assert!(error.contains("700..400"), "{error}");
}
