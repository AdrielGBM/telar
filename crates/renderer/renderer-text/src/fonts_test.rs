use super::*;

// One test rather than four: the installed database is process-wide, so separate tests would race.
#[test]
fn installing_adds_to_the_faces_in_force_and_never_takes_any_away() {
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
        extra_font_paths: vec![PathBuf::from("/nonexistent/one.ttf")],
        ..named.clone()
    };
    let two = FontConfig {
        extra_font_paths: vec![PathBuf::from("/nonexistent/two.ttf")],
        ..named
    };
    install(one.clone());
    let both = install(two);
    assert!(
        Arc::ptr_eq(&both, &install(one)),
        "a shaper naming faces already read must find them, and must not cost another shaper the ones it added"
    );
}
