use super::{font_asset_tokens, stale_cli_message};

/// The failure this catches was silent: an installed `cargo-telar` older than the `telar` it drives asks for a hot-reload build the way it used to — an environment variable — the macro builds a plain application instead, and the only thing said about it is that the app never connected to a channel it was never compiled to open.
#[test]
fn an_older_cli_asking_the_old_way_is_named() {
    let message = stale_cli_message(true).expect("this test binary has no `hot-reload` feature");

    assert!(
        message.contains("cargo install cargo-telar"),
        "the message has to name what fixes it: {message}"
    );
    assert!(
        message.contains(env!("CARGO_PKG_VERSION")),
        "and which version to install: {message}"
    );
}

/// A current CLI names the feature and sets no such variable, which is every build that is not the mismatch above.
#[test]
fn a_build_that_never_asked_is_left_alone() {
    assert_eq!(stale_cli_message(false), None);
}

/// A declared face reaches a native shaper embedded and answering to what the manifest said, weight range and axes included.
#[test]
fn a_declared_face_is_embedded_as_the_manifest_declared_it() {
    let font = telar_project::FontDeclaration {
        family: "Display".to_string(),
        src: "assets/Display.ttf".to_string(),
        weight: None,
        style: telar_project::FontStyleDeclaration::Italic,
        axes: [("wght".to_string(), [100.0, 900.0])].into_iter().collect(),
        display: telar_project::FontDisplay::Swap,
        size_adjust: None,
    };
    let tokens = font_asset_tokens(&font, std::path::Path::new("/pkg/assets/Display.ttf"))
        .to_string()
        .replace(' ', "");
    assert!(
        tokens.contains("include_bytes!(\"/pkg/assets/Display.ttf\")"),
        "{tokens}"
    );
    assert!(tokens.contains(".named(\"Display\")"), "{tokens}");
    assert!(
        tokens.contains("FontWeight::range(100u16,900u16)"),
        "{tokens}"
    );
    assert!(tokens.contains("FontStyle::Italic"), "{tokens}");
    assert!(
        tokens.contains("FontAxis::new([119u8,103u8,104u8,116u8],100f32,900f32)"),
        "{tokens}"
    );
}
