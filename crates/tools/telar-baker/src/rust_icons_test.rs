//! A crate that names its icons from Rust, `icon!("set:name")`, with no `.rsx` at all, as a widget library does: the bake finds them in its `.rs` files and in the Rust its `.rsx` writes, bakes them into its artifact and ships their record, and a call naming an id no source has fails the bake on its line.

use std::path::{Path, PathBuf};

use telar_baker::{bake_package, read_library_icons};
use telar_project::{AssetContext, asset_kind_for_id, read_index};

const VERSION: &str = "0.2.1";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/iconify")
}

fn write(root: &Path, file: &str, contents: &str) {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn library(name: &str, telar_toml: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_rust_icons_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(
        &root,
        "Cargo.toml",
        "[package]\nname = \"widget-lib\"\nversion = \"0.1.0\"\n",
    );
    write(
        &root,
        "telar.toml",
        &format!(
            "[telar]\nlibrary = true\n\n[telar.icons]\niconify = \"{}\"\n{telar_toml}",
            fixtures().display()
        ),
    );
    root
}

fn baked_icons(root: &Path) -> Vec<String> {
    read_index(&root.join(".telar"))
        .unwrap()
        .expect("the bake wrote an artifact")
        .entries
        .into_iter()
        .filter(|entry| entry.kind == "icon")
        .map(|entry| entry.path)
        .collect()
}

#[test]
fn a_crate_with_no_rsx_bakes_the_icons_its_rust_names() {
    let root = library("plain", "");
    write(
        &root,
        "src/lib.rs",
        "pub fn search() -> telar_icons::IconName {\n    telar_icons::icon!(\"demo:home\")\n}\n// icon!(\"demo:badge\")\n",
    );
    write(
        &root,
        "src/toolbar_test.rs",
        "#[test]\nfn draws() { let _ = icon!(\"demo:star\"); }\n",
    );
    let report = bake_package(&root, "test", VERSION).expect("the crate names icons");
    assert!(report.errors.is_empty(), "{:?}", report.errors);

    assert_eq!(baked_icons(&root), ["demo:home", "demo:star"]);
    let shipped = read_library_icons(&root)
        .expect("the record answers for the artifact")
        .expect("the library baked icons");
    assert_eq!(
        shipped.icons.into_iter().collect::<Vec<_>>(),
        ["demo:home", "demo:star"]
    );

    let baked = AssetContext::load(&root, VERSION)
        .baked_id(asset_kind_for_id("icon").unwrap(), "demo:home")
        .expect("the call resolves against the artifact");
    assert!(
        baked.init_expr.starts_with("SvgData::from_baked_vector("),
        "{}",
        baked.init_expr
    );
}

#[test]
fn a_bare_name_from_rust_is_baked_in_the_default_set() {
    let root = library("bare", "default_set = \"demo\"\n");
    write(&root, "src/lib.rs", "fn a() { icon!(\"gear\"); }\n");
    let report = bake_package(&root, "test", VERSION).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(baked_icons(&root), ["demo:gear"]);
}

#[test]
fn an_id_no_source_has_fails_the_bake_on_its_line() {
    let root = library("missing", "");
    write(
        &root,
        "src/widgets/bar.rs",
        "fn a() {\n\n    icon!(\"demo:nope\");\n}\n",
    );
    let report = bake_package(&root, "test", VERSION).unwrap();
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
    assert!(
        report.errors[0].starts_with("src/widgets/bar.rs:3: no icon source has `demo:nope`"),
        "{}",
        report.errors[0]
    );
}

#[test]
fn the_rust_an_rsx_writes_names_icons_too() {
    let root = library("rsx", "");
    write(
        &root,
        "src/panel.rsx",
        "[logic]\nfn gear() -> telar_icons::IconName {\n    icon!(\"demo:gear\")\n}\n\n[view]\ncol\n    let home = icon!(\"demo:home\")\n    icon_button icon:icon!(\"demo:star\") label:\"Star\"\n",
    );
    let report = bake_package(&root, "test", VERSION).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(baked_icons(&root), ["demo:gear", "demo:home", "demo:star"]);
}

#[test]
fn a_crate_that_stops_naming_icons_empties_its_artifact() {
    let root = library("stopped", "");
    write(&root, "src/lib.rs", "fn a() { icon!(\"demo:home\"); }\n");
    bake_package(&root, "test", VERSION).unwrap();
    assert_eq!(baked_icons(&root), ["demo:home"]);

    write(&root, "src/lib.rs", "fn a() {}\n");
    let report = bake_package(&root, "test", VERSION).expect("the old artifact is rewritten");
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(baked_icons(&root).is_empty());
    assert_eq!(read_library_icons(&root), Ok(None));
}

#[test]
fn a_crate_whose_icons_resolve_at_run_time_names_nothing_to_bake() {
    let root = library("runtime", "mode = \"runtime\"\n");
    write(&root, "src/lib.rs", "fn a() { icon!(\"demo:home\"); }\n");
    assert!(bake_package(&root, "test", VERSION).is_none());
}
