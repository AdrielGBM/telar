use renderer_core::{Color, FontFamily, FontWeight, TextStyle};

use super::*;
use crate::TextShaper;

const TEST_FACE: &[u8] = include_bytes!("../test-fonts/TelarTest.ttf");

// One test rather than several: the installed database is process-wide, so separate tests would race.
#[test]
fn the_database_only_grows_and_every_shaper_takes_a_face_that_arrives() {
    // A family nothing resolves to and faces nothing can read, so every install changes which `Fonts` is in force without changing a face — the tests measuring text beside this keep measuring the same widths.
    let named = FontConfig {
        sans_serif_family_candidates: vec!["a family no system has".to_string()],
        ..FontConfig::default()
    };
    let installed = install(named.clone());
    assert!(
        Arc::ptr_eq(&installed, &install(named.clone())),
        "the same configuration twice must load nothing and replace nothing"
    );
    assert!(
        Arc::ptr_eq(&installed, &install(FontConfig::default())),
        "a default-configured shaper built after the application's must shape in the application's fonts rather than throw them away"
    );

    let one = FontConfig {
        faces: vec![FontAsset::file("/nonexistent/one.ttf")],
        ..named.clone()
    };
    let two = FontConfig {
        faces: vec![FontAsset::file("/nonexistent/two.ttf")],
        ..named.clone()
    };
    install(one.clone());
    let both = install(two);
    assert!(
        Arc::ptr_eq(&both, &install(one)),
        "a shaper naming faces already read must find them, and must not cost another shaper the ones it added"
    );

    let stack = TextStyle::new(16.0, Color::BLACK).with_font_family(FontFamily::stack([
        FontFamily::Named("Telar Declared".into()),
        FontFamily::Monospace,
    ]));
    let mut drawing = TextShaper::new();
    assert!(!drawing.family_available("Telar Declared"));
    let fallback = drawing.measure_text("Wide Words", None, 1000.0, &stack);

    let generation = renderer_core::text_metrics_generation();
    let arrived = add_faces(vec![
        FontAsset::embedded(TEST_FACE)
            .named("Telar Declared")
            .with_weight(FontWeight::range(100, 900)),
    ]);
    assert_eq!(
        arrived.families(),
        both.families(),
        "a face that arrives must not take the default family away from the configuration that chose it"
    );
    assert!(
        renderer_core::text_metrics_generation() > generation,
        "layout must hear that its measurements may be stale"
    );
    assert!(
        drawing.family_available("Telar Declared"),
        "a shaper built before the face arrived must take it, answering to the family its declaration named"
    );
    assert!(
        drawing.family_available("Telar Test"),
        "and to the family the file itself declares"
    );
    assert!(crate::font_family_available("Telar Declared"));
    let resolved = drawing.measure_text("Wide Words", None, 1000.0, &stack);
    assert_ne!(
        resolved, fallback,
        "the stack must re-resolve to the face that arrived rather than keep the fallback it cached"
    );

    let generation = renderer_core::text_metrics_generation();
    let again = add_faces(vec![
        FontAsset::embedded(TEST_FACE)
            .named("Telar Declared")
            .with_weight(FontWeight::range(100, 900)),
    ]);
    assert!(Arc::ptr_eq(&arrived, &again));
    assert_eq!(
        renderer_core::text_metrics_generation(),
        generation,
        "a face already loaded changes no measurement"
    );
}

#[test]
fn a_file_that_is_not_there_loads_nothing_rather_than_failing() {
    let mut db = fontdb::Database::new();
    assert!(load_asset(&mut db, &FontAsset::file("/nonexistent/face.ttf")).is_empty());
}

#[test]
fn a_declared_family_style_and_weight_are_what_the_face_answers_to() {
    let mut db = fontdb::Database::new();
    let asset = FontAsset::embedded(TEST_FACE)
        .named("Display")
        .with_style(renderer_core::FontStyle::Italic)
        .with_weight(FontWeight::fixed(700));
    let ids = load_asset(&mut db, &asset);
    let face = db.face(ids[0]).unwrap();
    assert_eq!(face.families[0].0, "Display");
    assert!(face.families.iter().any(|(name, _)| name == "Telar Test"));
    assert_eq!(face.style, fontdb::Style::Italic);
    assert_eq!(face.weight, fontdb::Weight(700));
}
