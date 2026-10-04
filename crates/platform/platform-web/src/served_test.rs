use platform_core::{ColorScheme, SystemPreferences};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use super::{ServedState, parse_served_state};

wasm_bindgen_test_configure!(run_in_browser);

/// The state a page carries, as `cargo telar build --prerender` writes it; `telar`'s own tests hold the packager to the same file.
const FIXTURE: &str = include_str!("served_state_fixture.json");

#[wasm_bindgen_test]
fn a_page_s_state_reads_back_as_the_inputs_it_was_written_from() {
    assert_eq!(
        parse_served_state(FIXTURE),
        Some(ServedState {
            version: 1,
            locale: Some("es".to_string()),
            preferences: SystemPreferences {
                color_scheme: Some(ColorScheme::Dark),
                reduced_motion: Some(true),
                high_contrast: None,
                locales: vec!["es-CL".to_string(), "en".to_string()],
            },
            surface: (1280, 800),
            signals: vec![
                ("@telar/theme.scheme".to_string(), "system".to_string()),
                ("site::visits".to_string(), "3".to_string()),
            ],
        })
    );
}

#[wasm_bindgen_test]
fn a_state_that_cannot_be_read_is_none() {
    assert_eq!(parse_served_state("{\"version\":"), None);
    assert_eq!(parse_served_state("{\"version\":1}"), None);
}
