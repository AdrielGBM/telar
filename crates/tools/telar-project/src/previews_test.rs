use std::path::PathBuf;

use super::*;
use crate::{MANIFEST_FILENAME, TelarManifest};

fn package(name: &str, telar_toml: &str, cargo_toml: Option<&str>) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_previews_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(MANIFEST_FILENAME), telar_toml).unwrap();
    if let Some(cargo_toml) = cargo_toml {
        std::fs::write(root.join("Cargo.toml"), cargo_toml).unwrap();
    }
    root
}

fn load_error(name: &str, telar_toml: &str) -> String {
    let root = package(name, telar_toml, None);
    TelarManifest::load(&root)
        .expect_err("the table is refused")
        .to_string()
}

#[test]
fn a_table_with_every_key_reads_back() {
    let root = package(
        "full",
        r#"
[telar.previews]
include = ["telar-components", "my_plugin"]
viewports = { phone = "390x844", desktop = "1280X800" }

[telar.previews.matrices]
themes = { mode = ["light", "dark"] }
sizes = { viewport = ["phone", "1024x768"], control_size = ["small", "large"], dir = ["ltr", "rtl"], ghost = [true, false], size = [12, 16] }
"#,
        None,
    );
    let previews = TelarManifest::load(&root)
        .expect("a valid table")
        .telar
        .previews;
    let include: Vec<&str> = previews
        .include()
        .iter()
        .map(PreviewsInclude::crate_name)
        .collect();
    assert_eq!(include, ["telar_components", "my_plugin"]);
    let viewports = previews.viewports.unwrap();
    assert_eq!(
        viewports["phone"],
        Viewport {
            width: 390,
            height: 844
        }
    );
    assert_eq!(
        viewports["desktop"],
        Viewport {
            width: 1280,
            height: 800
        }
    );
    let matrices = previews.matrices.unwrap();
    assert_eq!(
        matrices["themes"]["mode"],
        [
            MatrixValue::Text("light".into()),
            MatrixValue::Text("dark".into())
        ]
    );
    assert_eq!(
        matrices["sizes"]["ghost"],
        [MatrixValue::Bool(true), MatrixValue::Bool(false)]
    );
    assert_eq!(
        matrices["sizes"]["size"],
        [MatrixValue::Int(12), MatrixValue::Int(16)]
    );
}

#[test]
fn a_package_without_the_table_includes_nothing() {
    let root = package("absent", "[telar]\nbackend = \"auto\"\n", None);
    let previews = TelarManifest::load(&root).unwrap().telar.previews;
    assert!(previews.include().is_empty());
    assert_eq!(previews, PreviewsSection::default());
}

#[test]
fn a_misspelled_key_in_the_table_is_named() {
    let error = load_error("typo", "[telar.previews]\nincludes = [\"a\"]\n");
    assert!(error.contains("includes"), "{error}");
}

#[test]
fn an_include_entry_that_is_not_a_crate_name_is_an_error_on_the_value() {
    for (label, entry) in [
        ("path", "telar_components::previews"),
        ("space", "my plugin"),
        ("empty", ""),
    ] {
        let error = load_error(
            &format!("bad_include_{label}"),
            &format!("[telar]\nbackend = \"auto\"\n\n[telar.previews]\ninclude = [\"{entry}\"]\n"),
        );
        assert!(error.contains("line 5"), "{error}");
        assert!(error.contains("is not a crate name"), "{error}");
    }
}

#[test]
fn an_include_naming_one_crate_twice_is_an_error() {
    let error = load_error(
        "twice",
        "[telar.previews]\ninclude = [\"telar-components\", \"telar_components\"]\n",
    );
    assert!(
        error.contains("lists `telar_components` more than once"),
        "{error}"
    );
}

#[test]
fn a_viewport_that_is_not_a_size_is_an_error() {
    let error = load_error(
        "bad_viewport",
        "[telar.previews]\nviewports = { phone = \"tall\" }\n",
    );
    assert!(error.contains("viewport \"tall\" is not a size"), "{error}");
    let error = load_error(
        "zero_viewport",
        "[telar.previews]\nviewports = { phone = \"0x844\" }\n",
    );
    assert!(error.contains("is not a size"), "{error}");
}

#[test]
fn a_matrix_is_checked_against_the_axes_it_may_vary() {
    for (label, matrix, expected) in [
        ("empty", "themes = {}", "varies nothing"),
        ("no_values", "themes = { mode = [] }", "lists no values"),
        (
            "repeated",
            "themes = { mode = [\"dark\", \"dark\"] }",
            "lists `dark` more than once",
        ),
        (
            "dir",
            "themes = { dir = [\"up\"] }",
            "\"up\" is not one of ltr, rtl",
        ),
        (
            "size",
            "themes = { control_size = [\"huge\"] }",
            "\"huge\" is not one of mini",
        ),
        (
            "mode_type",
            "themes = { mode = [1] }",
            "`1` is not a string",
        ),
        (
            "viewport",
            "themes = { viewport = [\"watch\"] }",
            "\"watch\" is neither a size",
        ),
        (
            "arg",
            "themes = { \"not an arg\" = [1] }",
            "is neither one of mode",
        ),
        (
            "name",
            "\"two words\" = { mode = [\"dark\"] }",
            "is not a name",
        ),
    ] {
        let error = load_error(
            &format!("matrix_{label}"),
            &format!("[telar.previews.matrices]\n{matrix}\n"),
        );
        assert!(error.contains(expected), "{label}: {error}");
    }
}

#[test]
fn a_matrix_may_name_a_viewport_the_workspace_declares() {
    let root = std::env::temp_dir().join(format!("telar_previews_inherit_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let app = root.join("apps/app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    std::fs::write(
        root.join(MANIFEST_FILENAME),
        "[telar.previews]\nviewports = { phone = \"390x844\" }\ninclude = [\"shared\"]\n",
    )
    .unwrap();
    std::fs::write(
        app.join(MANIFEST_FILENAME),
        "[telar.previews]\ninclude = []\n\n[telar.previews.matrices]\nphones = { viewport = [\"phone\"] }\n",
    )
    .unwrap();
    let previews = TelarManifest::load(&app)
        .expect("the name resolves")
        .telar
        .previews;
    assert!(
        previews.include().is_empty(),
        "an empty list is a declaration"
    );
    assert!(
        previews.viewports.unwrap().contains_key("phone"),
        "inherited"
    );
}

const APP_CARGO_TOML: &str = r#"
[package]
name = "app"
version = "0.1.0"

[dependencies]
telar-components = "1"

[dev-dependencies]
dev_only = "1"
"#;

#[test]
fn an_include_naming_a_dependency_has_no_problem() {
    let root = package(
        "reachable",
        "[telar.previews]\ninclude = [\"telar-components\"]\n",
        Some(APP_CARGO_TOML),
    );
    assert!(previews_include_problems(&root).unwrap().is_empty());
}

#[test]
fn an_include_naming_a_crate_the_package_does_not_depend_on_is_said_on_its_line() {
    let root = package(
        "unreachable",
        "[telar]\nbackend = \"auto\"\n\n[telar.previews]\ninclude = [\"telar-components\", \"missing-lib\", \"dev_only\", \"app\"]\n",
        Some(APP_CARGO_TOML),
    );
    let problems = previews_include_problems(&root).unwrap();
    let named: Vec<&str> = problems
        .iter()
        .map(|problem| problem.declaration.entry.crate_name())
        .collect();
    assert_eq!(named, ["missing_lib", "dev_only", "app"]);

    let missing = &problems[0];
    assert_eq!(missing.declaration.line, 5);
    assert_eq!(missing.declaration.columns, (31, 44));
    assert!(
        missing.message.contains("is not a dependency of `app`"),
        "{}",
        missing.message
    );
    assert!(
        missing.help.contains("cargo add -p app missing-lib"),
        "{}",
        missing.help
    );
    assert!(
        problems[1].help.contains("`[dev-dependencies]`"),
        "{}",
        problems[1].help
    );
    assert!(
        problems[2].message.contains("is `app` itself"),
        "{}",
        problems[2].message
    );
}

#[test]
fn a_named_matrix_reads_into_axes_globals_first_then_args_by_name() {
    let root = package(
        "named_axes",
        r#"
[telar.previews]
viewports = { phone = "390x844" }

[telar.previews.matrices]
everything = { size = [12, 1.5], viewport = ["phone", " 1280x800 "], control_size = ["mini", "large"], dir = ["rtl"], mode = ["dark"], locale = ["ar"], label = ['"Save"', "Primary"], ghost = [true] }
"#,
        None,
    );
    let matrices = TelarManifest::load(&root)
        .unwrap()
        .telar
        .previews
        .named_matrices()
        .expect("a valid table");
    assert_eq!(
        matrices["everything"],
        [
            MatrixAxis::Mode(vec!["dark".into()]),
            MatrixAxis::Locale(vec!["ar".into()]),
            MatrixAxis::Dir(vec![MatrixDirection::Rtl]),
            MatrixAxis::Viewport(vec!["phone".into(), "1280x800".into()]),
            MatrixAxis::ControlSize(vec![MatrixControlSize::Mini, MatrixControlSize::Large]),
            MatrixAxis::Arg {
                name: "ghost".into(),
                values: vec!["true".into()],
            },
            MatrixAxis::Arg {
                name: "label".into(),
                values: vec!["\"Save\"".into(), "Primary".into()],
            },
            MatrixAxis::Arg {
                name: "size".into(),
                values: vec!["12".into(), "1.5".into()],
            },
        ]
    );
    let names: Vec<&str> = matrices["everything"]
        .iter()
        .map(MatrixAxis::name)
        .collect();
    assert_eq!(
        names,
        [
            "mode",
            "locale",
            "dir",
            "viewport",
            "control_size",
            "ghost",
            "label",
            "size"
        ]
    );
}

#[test]
fn a_float_arg_value_keeps_the_form_that_reads_back_as_a_float() {
    let root = package(
        "float_axis",
        "[telar.previews.matrices]\nscale = { scale = [2.0, 1e-7, inf, -inf, nan] }\n",
        None,
    );
    let matrices = TelarManifest::load(&root)
        .unwrap()
        .telar
        .previews
        .named_matrices()
        .unwrap();
    assert_eq!(
        matrices["scale"],
        [MatrixAxis::Arg {
            name: "scale".into(),
            values: vec![
                "2.0".into(),
                "1e-7".into(),
                "+inf".into(),
                "-inf".into(),
                "+NaN".into()
            ],
        }]
    );
}

#[test]
fn a_package_without_matrices_names_none() {
    assert_eq!(
        PreviewsSection::default().named_matrices(),
        Ok(BTreeMap::new())
    );
}

#[test]
fn every_problem_in_a_matrix_is_said_at_once() {
    let error = load_error(
        "matrix_many",
        "[telar.previews.matrices]\nbroken = { dir = [\"up\"], mode = [1], \"not an arg\" = [true] }\n",
    );
    assert!(error.contains("\"up\" is not one of ltr, rtl"), "{error}");
    assert!(error.contains("`1` is not a string"), "{error}");
    assert!(error.contains("is neither one of mode"), "{error}");
}
