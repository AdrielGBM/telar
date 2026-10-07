//! `cargo telar test` rendering a component that contains text.
//!
//! The harness exits the process when it is done, so the test runs it in a child: this same binary, started again with `TELAR_TEST` set, which is how `cargo telar test` reaches the app it builds. A fresh process is also what makes the test honest — a measurer or a face installed by an earlier test in the same process would hide the harness not installing them. The face is declared in the config and checked while the preview is built, which is how `[[telar.fonts]]` reaches the shaper.

use std::process::Command;

use telar::{
    AppConfig, Color, FontAsset, LayoutError, LayoutItem, LayoutStyle, PreviewEntry, Text,
    TextStyle, font_families, font_family_available, try_run_test,
};

const CHILD_MARKER: &str = "TELAR_PREVIEW_HARNESS_CHILD";
const DECLARED_FAMILY: &str = "Preview Harness Face";
const DECLARED_FACE: &[u8] =
    include_bytes!("../../renderer/renderer-text/test-fonts/TelarTest.ttf");

fn labelled() -> Result<Box<dyn LayoutItem>, LayoutError> {
    assert!(
        font_family_available(DECLARED_FAMILY),
        "the face the app config declares was not loaded before the preview was built"
    );
    assert!(
        font_families()
            .iter()
            .any(|family| family == DECLARED_FAMILY),
        "and is offered under the family it declares"
    );
    let text = Text::new(
        || String::from("A preview with some text to measure"),
        LayoutStyle::new(),
        || TextStyle::new(14.0, Color::rgba(0.1, 0.1, 0.1, 1.0)),
    )?;
    Ok(Box::new(text))
}

#[test]
fn a_preview_with_text_renders_without_an_installed_measurer() {
    if std::env::var_os(CHILD_MARKER).is_some() {
        let entry = PreviewEntry {
            component_name: "Labelled",
            preview_name: "default",
            build: labelled,
            surface: None,
        };
        let config = AppConfig::default()
            .with_fonts([FontAsset::embedded(DECLARED_FACE).named(DECLARED_FAMILY)]);
        try_run_test(vec![entry], config);
    }

    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "a_preview_with_text_renders_without_an_installed_measurer",
            "--nocapture",
        ])
        .env(CHILD_MARKER, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed, 0 failed"),
        "the harness failed a preview with text:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
