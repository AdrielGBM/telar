use std::path::{Path, PathBuf};

use super::{resolve_theme_type, theme_type_in_config};

fn package(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_theme_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    root
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

/// The bug this exists for: `apps/sandbox` names its theme in `app!` and nowhere else, so reading only `telar.toml` answered `None` and the editor mirrored every themed component as `Theme::<>` over the build's own output.
#[test]
fn the_invocation_answers_when_the_config_does_not() {
    let root = package("from_source");
    write(
        &root.join("src/lib.rs"),
        "telar::app!(\n    core::theme::SandboxTheme,\n    {},\n    telar::AppConfig::default(),\n    core::app::Root\n);\n",
    );

    assert_eq!(theme_type_in_config(&root), None);
    assert_eq!(
        resolve_theme_type(&root).as_deref(),
        Some("core::theme::SandboxTheme")
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// The declaration outranks the source, so a project that wants the answer written down gets to write it down.
#[test]
fn the_config_key_wins() {
    let root = package("config_wins");
    write(
        &root.join("telar.toml"),
        "[telar]\ntheme = \"app::Declared\"\n",
    );
    write(
        &root.join("src/lib.rs"),
        "telar::app!(other::Scanned, {}, c, R);\n",
    );

    assert_eq!(resolve_theme_type(&root).as_deref(), Some("app::Declared"));

    let _ = std::fs::remove_dir_all(&root);
}

/// `rsx_modules!()` is how a crate with no theme places its `.rsx`; it must not be read as naming one.
#[test]
fn no_argument_is_no_theme() {
    let root = package("themeless");
    write(&root.join("src/lib.rs"), "telar::rsx_modules!();\n");

    assert_eq!(resolve_theme_type(&root), None);

    let _ = std::fs::remove_dir_all(&root);
}

/// A `mod.rs` places its own directory's `.rsx`, so it is one of the few files allowed to carry the invocation — and the only one a nested crate has.
#[test]
fn a_nested_invocation_is_found() {
    let root = package("nested");
    write(&root.join("src/lib.rs"), "pub mod editor;\n");
    write(
        &root.join("src/editor/mod.rs"),
        "telar::rsx_modules!(editor::EditorTheme);\n",
    );

    assert_eq!(
        resolve_theme_type(&root).as_deref(),
        Some("editor::EditorTheme")
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// A file that mentions neither macro is never parsed, and one that mentions them in prose yields nothing rather than a wrong answer.
#[test]
fn prose_about_the_macro_names_no_theme() {
    let root = package("prose");
    write(
        &root.join("src/lib.rs"),
        "//! Call `telar::rsx_modules!()` from the module that owns the files.\npub struct Nothing;\n",
    );

    assert_eq!(resolve_theme_type(&root), None);

    let _ = std::fs::remove_dir_all(&root);
}
