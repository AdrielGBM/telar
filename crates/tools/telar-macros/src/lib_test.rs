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

fn package(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("telar_macros_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    root
}

fn wire(root: &std::path::Path) -> Result<Vec<super::WiredFile>, proc_macro2::TokenStream> {
    wire_with(root, &[])
}

fn wire_with(
    root: &std::path::Path,
    prelude: &[telar_project::PreludeEntry],
) -> Result<Vec<super::WiredFile>, proc_macro2::TokenStream> {
    let assets = telar_project::AssetContext::load(root, "0.0.0");
    let flavour = telar_project::BuildFlavour::Plain;
    super::wire_sources(
        root,
        &root.join("src"),
        &telar_project::generated_dir(root, flavour),
        None,
        prelude,
        flavour,
        &assets,
    )
}

/// The macro has no transpiler, so the reason the CLI could not produce the artifact is the only thing that can tell the author what is wrong with their markup — "re-run the command" is a loop when the command is what failed.
#[test]
fn a_recorded_transpile_error_is_what_the_build_reports() {
    let root = package("reports");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"oops\n").unwrap();
    let failure = telar_project::BuildFailure {
        source: "home.rsx".to_string(),
        hash: telar_project::content_hash(b"[view]\ntext \"oops\n"),
        message: "src/home.rsx:2: unterminated string literal".to_string(),
    };
    telar_project::write_build_failure(&root, telar_project::BuildFlavour::Plain, &failure)
        .unwrap();

    let message = wire(&root).err().unwrap().to_string();
    assert!(
        message.contains("src/home.rsx:2: unterminated string literal"),
        "{message}"
    );
    assert!(!message.contains("no longer answers"), "{message}");

    let _ = std::fs::remove_dir_all(&root);
}

/// Once the author has edited the file the record names, it may be describing an error that is gone, so the generic advice is true again.
#[test]
fn a_record_for_an_edited_source_is_not_reported() {
    let root = package("edited");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"fixed\"\n").unwrap();
    let failure = telar_project::BuildFailure {
        source: "home.rsx".to_string(),
        hash: telar_project::content_hash(b"[view]\ntext \"oops\n"),
        message: "src/home.rsx:2: unterminated string literal".to_string(),
    };
    telar_project::write_build_failure(&root, telar_project::BuildFlavour::Plain, &failure)
        .unwrap();

    let message = wire(&root).err().unwrap().to_string();
    assert!(!message.contains("unterminated"), "{message}");

    let _ = std::fs::remove_dir_all(&root);
}

/// An artifact exactly as `cargo telar transpile` would leave it for one `home.rsx`, recorded under `prelude`.
fn transpiled_under(name: &str, prelude: &[&str]) -> std::path::PathBuf {
    let root = package(name);
    let source = "[view]\ntext \"a\"\n";
    let output = "// generated\n";
    std::fs::write(root.join("src/home.rsx"), source).unwrap();
    let flavour = telar_project::BuildFlavour::Plain;
    let generated = telar_project::generated_dir(&root, flavour);
    std::fs::create_dir_all(&generated).unwrap();
    std::fs::write(generated.join("home.rs"), output).unwrap();
    let index = telar_project::BuildIndex {
        format: telar_project::BUILD_ARTIFACT_FORMAT,
        producer: "test".to_string(),
        telar_version: env!("CARGO_PKG_VERSION").to_string(),
        theme: None,
        prelude: prelude.iter().map(|path| path.to_string()).collect(),
        uses_assets: false,
        entries: vec![telar_project::BuildEntry {
            source: "home.rsx".to_string(),
            hash: telar_project::content_hash(source.as_bytes()),
            output_hash: telar_project::content_hash(output.as_bytes()),
            previews: false,
        }],
    };
    telar_project::write_build_index(&root, flavour, &index).unwrap();
    root
}

fn entries(declared: &[&str]) -> Vec<telar_project::PreludeEntry> {
    declared
        .iter()
        .map(|entry| telar_project::PreludeEntry::parse(entry).unwrap())
        .collect()
}

#[test]
fn an_artifact_transpiled_under_the_declared_prelude_is_wired() {
    let root = transpiled_under("prelude_agrees", &["telar_components"]);
    let wired = wire_with(&root, &entries(&["telar-components"]));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(wired.map(|files| files.len()).ok(), Some(1));
}

/// Nothing under `src/` changed, so the message has to point at `telar.toml` and at the command that brings the artifact back in line with it.
#[test]
fn an_artifact_transpiled_under_another_prelude_is_refused_naming_both() {
    let one: &[&str] = &["telar_components"];
    let two: &[&str] = &["telar_components", "other"];
    for (label, recorded, declared) in [
        ("added", one, two),
        ("removed", one, &[][..]),
        ("reordered", two, &["other", "telar-components"][..]),
    ] {
        let root = transpiled_under(&format!("prelude_{label}"), recorded);
        let message = wire_with(&root, &entries(declared))
            .err()
            .unwrap_or_else(|| panic!("a prelude {label} since the transpile is refused"))
            .to_string();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            message.contains("telar.toml declares `prelude"),
            "{message}"
        );
        assert!(
            message.contains("was transpiled with `prelude"),
            "{message}"
        );
        assert!(message.contains("cargo telar transpile"), "{message}");
    }
}

#[test]
fn a_source_edited_under_the_same_prelude_gets_the_generic_message() {
    let root = transpiled_under("prelude_same_edited", &["telar_components"]);
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"b\"\n").unwrap();
    let message = wire_with(&root, &entries(&["telar_components"]))
        .err()
        .unwrap()
        .to_string();
    let _ = std::fs::remove_dir_all(&root);
    assert!(message.contains("no longer answers"), "{message}");
    assert!(!message.contains("prelude"), "{message}");
}
