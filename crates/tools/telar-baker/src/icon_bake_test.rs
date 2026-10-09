//! A package that draws icons from a local Iconify set, baked the way `cargo telar bake` bakes it: only the ids its `.rsx` writes reach the artifact, and an id it computes is refused while every id must be baked.

use std::path::{Path, PathBuf};

use telar_baker::{ICONS_NOTICE_FILENAME, bake_package, read_icon_record};
use telar_project::{AssetContext, read_index, static_name_for_path};

const VERSION: &str = "0.2.1";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/iconify")
}

fn package(name: &str, mode: &str, view: &str) -> PathBuf {
    package_with(name, mode, "", view)
}

fn package_with(name: &str, mode: &str, extra: &str, view: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_icon_bake_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"icon-app\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("telar.toml"),
        format!(
            "[telar.icons]\nmode = \"{mode}\"\niconify = \"{}\"\n{extra}",
            fixtures().display()
        ),
    )
    .unwrap();
    std::fs::write(root.join("src/app.rsx"), view).unwrap();
    root
}

fn baked_icons(root: &Path) -> Vec<String> {
    read_index(&root.join(".telar"))
        .unwrap()
        .unwrap()
        .entries
        .into_iter()
        .filter(|entry| entry.kind == "icon")
        .map(|entry| entry.path)
        .collect()
}

#[test]
fn only_the_icons_the_view_names_are_baked() {
    let root = package(
        "used",
        "baked",
        "[view]\ncol\n    icon name:\"demo:home\"\n    if true\n        icon name:\"demo:house\" size:32\n    icon name:\"demo:home\"\n",
    );
    let report = bake_package(&root, "test", VERSION).expect("the package has .rsx");
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);

    assert_eq!(baked_icons(&root), vec!["demo:home", "demo:house"]);
    let source = std::fs::read_to_string(root.join(".telar/assets.rs")).unwrap();
    assert_eq!(source.matches("pub static ").count(), 2, "{source}");
    assert!(
        source.contains(&static_name_for_path("demo:home")),
        "{source}"
    );
    assert!(
        !source.contains(&static_name_for_path("demo:gear")),
        "{source}"
    );
    assert!(source.contains("LazyLock<Arc<SvgData>>"), "{source}");

    let record = read_icon_record(&root).unwrap();
    assert_eq!(
        record.icons.keys().collect::<Vec<_>>(),
        vec!["demo:home", "demo:house"]
    );
    let notice = std::fs::read_to_string(root.join(".telar").join(ICONS_NOTICE_FILENAME)).unwrap();
    assert!(notice.contains("Icons: home, house"), "{notice}");

    let assets = AssetContext::load(&root, VERSION);
    let kind = telar_project::asset_kind_for_id("icon").unwrap();
    assert!(assets.resolve_id(kind, "demo:house").is_ok());
    assert!(assets.resolve_id(kind, "demo:gear").is_err());
}

#[test]
fn a_rebake_with_nothing_changed_rewrites_nothing() {
    let root = package("stable", "baked", "[view]\nicon name:\"demo:star\"\n");
    assert!(bake_package(&root, "test", VERSION).unwrap().changed);
    let again = bake_package(&root, "test", VERSION).unwrap();
    assert!(!again.changed);
    assert_eq!(baked_icons(&root), vec!["demo:star"]);
}

#[test]
fn a_computed_id_is_an_error_naming_runtime_mode() {
    let root = package(
        "computed",
        "baked",
        "[logic]\nlet which = signal(String::from(\"demo:home\"));\n\n[view]\ncol\n    icon name:\"demo:gear\"\n    icon name:$which\n",
    );
    let report = bake_package(&root, "test", VERSION).unwrap();
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
    let error = &report.errors[0];
    assert!(error.starts_with("src/app.rsx:"), "{error}");
    assert!(error.contains("name:$which"), "{error}");
    assert!(error.contains("mode = \"runtime\""), "{error}");
    assert_eq!(
        baked_icons(&root),
        vec!["demo:gear"],
        "the literal still bakes"
    );
}

/// An icon button hands its `icon` to the `icon` it draws, so a literal id it is given in `.rsx` bakes like one written on `icon` itself, in a view and in a preview alike. Anything else it is given is left as written, since it also takes what `icon!` builds.
#[test]
fn a_literal_id_given_to_a_component_that_carries_icons_is_baked() {
    let root = package(
        "carried",
        "baked",
        "[logic]\nlet which = signal(String::new());\n\n[view]\ncol\n    icon_button icon:\"demo:home\" label:\"Home\"\n    icon_button icon:$which label:\"Which\"\n    icon_button icon:icon!(\"demo:gear\") label:\"Gear\"\n\n[preview \"Starred\"]\nicon_button icon:\"demo:star\" label:\"Star\"\n",
    );
    let report = bake_package(&root, "test", VERSION).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(
        baked_icons(&root),
        vec!["demo:gear", "demo:home", "demo:star"]
    );
}

#[test]
fn a_computed_id_is_left_for_the_runtime_when_only_literals_are_baked() {
    let root = package(
        "both",
        "both",
        "[logic]\nlet which = signal(String::new());\n\n[view]\ncol\n    icon name:\"demo:gear\"\n    icon name:$which\n",
    );
    let report = bake_package(&root, "test", VERSION).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(baked_icons(&root), vec!["demo:gear"]);
}

#[test]
fn runtime_mode_bakes_nothing_and_ships_no_notice() {
    let root = package("runtime", "baked", "[view]\nicon name:\"demo:home\"\n");
    bake_package(&root, "test", VERSION).unwrap();
    std::fs::write(
        root.join("telar.toml"),
        "[telar.icons]\nmode = \"runtime\"\n",
    )
    .unwrap();
    let report = bake_package(&root, "test", VERSION).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(baked_icons(&root).is_empty());
    assert!(read_icon_record(&root).is_none());
    assert!(!root.join(".telar").join(ICONS_NOTICE_FILENAME).exists());
}

#[test]
fn a_bare_name_bakes_under_the_default_sets_id_and_resolves_from_the_rsx() {
    let root = package_with(
        "default_set",
        "baked",
        "default_set = \"demo\"\n",
        "[view]\ncol\n    icon name:\"home\"\n    icon name:\"demo:star\"\n",
    );
    let report = bake_package(&root, "test", VERSION).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(baked_icons(&root), vec!["demo:home", "demo:star"]);

    let assets = AssetContext::load(&root, VERSION);
    let kind = telar_project::asset_kind_for_id("icon").unwrap();
    let triple = assets.resolve_id(kind, "home").unwrap();
    assert!(triple.starts_with("(\"demo:home\", "), "{triple}");
}

#[test]
fn a_bare_name_without_a_default_set_fails_the_bake_on_its_line() {
    let root = package(
        "no_default_set",
        "baked",
        "[view]\ncol\n    icon name:\"demo:star\"\n    icon name:\"home\"\n",
    );
    let report = bake_package(&root, "test", VERSION).unwrap();
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
    let error = &report.errors[0];
    assert!(error.starts_with("src/app.rsx:4:"), "{error}");
    assert!(error.contains("default_set"), "{error}");
    assert_eq!(baked_icons(&root), vec!["demo:star"]);
}

#[test]
fn the_tint_decision_is_carried_into_the_artifact() {
    let root = package(
        "tint",
        "baked",
        "[view]\ncol\n    icon name:\"demo:home\"\n    icon name:\"brand:acme\"\n",
    );
    let report = bake_package(&root, "test", VERSION).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let monochrome: Vec<(String, bool)> = read_index(&root.join(".telar"))
        .unwrap()
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| (entry.path, entry.monochrome))
        .collect();
    assert_eq!(
        monochrome,
        vec![
            ("brand:acme".to_string(), false),
            ("demo:home".to_string(), true)
        ]
    );

    let assets = AssetContext::load(&root, VERSION);
    let kind = telar_project::asset_kind_for_id("icon").unwrap();
    assert!(
        assets
            .resolve_id(kind, "demo:home")
            .unwrap()
            .ends_with(", true)")
    );
    assert!(
        assets
            .resolve_id(kind, "brand:acme")
            .unwrap()
            .ends_with(", false)")
    );
}
