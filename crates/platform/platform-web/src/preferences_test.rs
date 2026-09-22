//! The browser's answers as a `SystemPreferences`, against a real page. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use platform_core::ColorScheme;
use telar_platform_web::{MediaAnswers, read_system_preferences};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn the_page_reports_its_languages_in_order() {
    let languages: Vec<String> = web_sys::window()
        .unwrap()
        .navigator()
        .languages()
        .iter()
        .filter_map(|language| language.as_string())
        .collect();
    let preferences = read_system_preferences();
    if !languages.is_empty() {
        assert_eq!(preferences.locales, languages);
    }
    assert!(
        !preferences.locales.is_empty(),
        "a browser always has a language"
    );
}

#[wasm_bindgen_test]
fn a_modern_browser_answers_every_appearance_query() {
    let preferences = read_system_preferences();
    assert!(preferences.color_scheme.is_some());
    assert!(preferences.reduced_motion.is_some());
    assert!(preferences.high_contrast.is_some());
}

#[wasm_bindgen_test]
fn the_snapshot_agrees_with_the_dark_query() {
    let dark = web_sys::window()
        .unwrap()
        .match_media("(prefers-color-scheme: dark)")
        .unwrap()
        .unwrap()
        .matches();
    assert_eq!(
        read_system_preferences().color_scheme == Some(ColorScheme::Dark),
        dark
    );
}

#[wasm_bindgen_test]
fn unsupported_features_stay_unknown() {
    let preferences = MediaAnswers::default().into_preferences(Vec::new());
    assert_eq!(preferences.color_scheme, None);
    assert_eq!(preferences.reduced_motion, None);
    assert_eq!(preferences.high_contrast, None);
}

#[wasm_bindgen_test]
fn forced_colors_count_as_more_contrast() {
    let preferences = MediaAnswers {
        more_contrast: Some(false),
        forced_colors: Some(true),
        ..MediaAnswers::default()
    }
    .into_preferences(Vec::new());
    assert_eq!(preferences.high_contrast, Some(true));

    let preferences = MediaAnswers {
        more_contrast: Some(false),
        forced_colors: None,
        ..MediaAnswers::default()
    }
    .into_preferences(Vec::new());
    assert_eq!(preferences.high_contrast, Some(false));
}

#[wasm_bindgen_test]
fn a_light_or_dark_answer_names_the_scheme() {
    let dark = MediaAnswers {
        dark: Some(true),
        light: Some(false),
        ..MediaAnswers::default()
    };
    let light = MediaAnswers {
        dark: Some(false),
        light: Some(true),
        ..MediaAnswers::default()
    };
    assert_eq!(
        dark.into_preferences(Vec::new()).color_scheme,
        Some(ColorScheme::Dark)
    );
    assert_eq!(
        light.into_preferences(Vec::new()).color_scheme,
        Some(ColorScheme::Light)
    );
}
