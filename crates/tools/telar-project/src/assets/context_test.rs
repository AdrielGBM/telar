use std::path::{Path, PathBuf};

use super::*;
use crate::{
    BakedAsset, MANIFEST_FILENAME, asset_kind_for_component, generate_assets, write_generated,
};

const VERSION: &str = "0.2.1";

fn package(name: &str, manifest: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("telar_asset_context_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join(MANIFEST_FILENAME), manifest).unwrap();
    root
}

fn bake(root: &Path, assets: &[BakedAsset]) {
    let generated = generate_assets(assets, "test", VERSION).unwrap();
    write_generated(&root.join(".telar"), &generated).unwrap();
}

fn icon(id: &str) -> BakedAsset {
    BakedAsset {
        kind: "icon".to_string(),
        path: id.to_string(),
        content: b"<svg/>".to_vec(),
        init_expr: "SvgData::from_baked_vector((24.0, 24.0), vec![])".to_string(),
        monochrome: false,
    }
}

fn icon_kind() -> &'static AssetKind {
    asset_kind_for_component("icon", "name").expect("icon is a registered component kind")
}

#[test]
fn the_icon_tags_name_prop_is_a_component_kind() {
    assert_eq!(icon_kind().id, "icon");
    assert!(asset_kind_for_component("icon", "size").is_none());
    assert_eq!(
        asset_kind_for_component("icon_button", "icon").map(|kind| kind.id),
        Some("icon"),
        "an icon button carries an icon id to the `icon` it draws"
    );
    assert!(asset_kind_for_component("icon_button", "name").is_none());
    assert!(asset_kind_for_component("icon_button", "label").is_none());
    assert!(
        super::super::asset_kind_for_tag("icon").is_none(),
        "`icon` is not a built-in tag"
    );
}

#[test]
fn the_section_decides_how_ids_are_baked() {
    for (mode, expected) in [
        ("baked", Some(IdBaking::Required)),
        ("both", Some(IdBaking::Literals)),
        ("runtime", None),
    ] {
        let root = package(
            mode,
            &format!("[telar.icons]\nmode = \"{mode}\"\nsvg = \"icons\"\n"),
        );
        let context = AssetContext::load(&root, VERSION);
        assert_eq!(
            context.id_baking("icon", "name").map(|(_, baking)| baking),
            expected,
            "{mode}"
        );
    }
    let root = package("absent", "[telar]\n");
    assert!(
        AssetContext::load(&root, VERSION)
            .id_baking("icon", "name")
            .is_none()
    );
}

#[test]
fn a_baked_id_becomes_the_pair_its_prop_accepts() {
    let root = package("resolved", "[telar.icons]\nsvg = \"icons\"\n");
    bake(&root, &[icon("mdi:home")]);
    let context = AssetContext::load(&root, VERSION);
    let pair = context.resolve_id(icon_kind(), "mdi:home").unwrap();
    assert_eq!(
        pair,
        format!(
            "(\"mdi:home\", ::std::sync::Arc::clone(&crate::__rsx_assets::{}), false)",
            crate::static_name_for_path("mdi:home")
        )
    );
}

#[test]
fn an_id_missing_from_the_artifact_names_the_bake() {
    let root = package("missing", "[telar.icons]\nsvg = \"icons\"\n");
    bake(&root, &[icon("mdi:home")]);
    let message = AssetContext::load(&root, VERSION)
        .resolve_id(icon_kind(), "mdi:account")
        .unwrap_err();
    assert!(
        message.contains("holds no icon name:\"mdi:account\""),
        "{message}"
    );
    assert!(message.contains("cargo telar bake"), "{message}");
    assert!(message.contains("[telar.icons]"), "{message}");
}

#[test]
fn an_unbaked_package_names_the_bake() {
    let root = package("unbaked", "[telar.icons]\nsvg = \"icons\"\n");
    let message = AssetContext::load(&root, VERSION)
        .resolve_id(icon_kind(), "mdi:home")
        .unwrap_err();
    assert!(message.contains("no baked artifact"), "{message}");
}

#[test]
fn the_dynamic_id_message_names_runtime_mode() {
    let message = AssetContext::dynamic_id_message(icon_kind(), "icon", "name");
    assert!(
        message.contains("`icon name:` takes a literal id"),
        "{message}"
    );
    let carried = AssetContext::dynamic_id_message(icon_kind(), "icon_button", "icon");
    assert!(
        carried.contains("`icon_button icon:` takes a literal id"),
        "{carried}"
    );
    assert!(carried.contains("crate that provides `icon`"), "{carried}");
    assert!(message.contains("mode = \"both\""), "{message}");
    assert!(message.contains("mode = \"runtime\""), "{message}");
}

#[test]
fn an_icon_is_not_a_file_to_track() {
    let root = package("tracked", "[telar.icons]\nsvg = \"icons\"\n");
    std::fs::write(root.join("assets/mark.svg"), "<svg/>").unwrap();
    bake(
        &root,
        &[
            icon("mdi:home"),
            BakedAsset {
                kind: "svg".to_string(),
                path: "mark.svg".to_string(),
                content: b"<svg/>".to_vec(),
                init_expr: "SvgData::from_baked_vector((1.0, 1.0), vec![])".to_string(),
                monochrome: false,
            },
        ],
    );
    let tracked = AssetContext::load(&root, VERSION).tracked_files();
    assert_eq!(tracked, vec![root.join("assets/mark.svg")]);
}

#[test]
fn a_monochrome_icon_carries_its_tint_in_the_triple() {
    let root = package("monochrome", "[telar.icons]\nsvg = \"icons\"\n");
    bake(
        &root,
        &[BakedAsset {
            monochrome: true,
            ..icon("mdi:home")
        }],
    );
    let triple = AssetContext::load(&root, VERSION)
        .resolve_id(icon_kind(), "mdi:home")
        .unwrap();
    assert!(triple.ends_with(", true)"), "{triple}");
}

#[test]
fn a_bare_name_resolves_in_the_default_set() {
    let root = package(
        "default_set",
        "[telar.icons]\nsvg = \"icons\"\ndefault_set = \"mdi\"\n",
    );
    bake(&root, &[icon("mdi:home")]);
    let context = AssetContext::load(&root, VERSION);
    let triple = context.resolve_id(icon_kind(), "home").unwrap();
    assert!(triple.starts_with("(\"mdi:home\", "), "{triple}");
    assert_eq!(context.literal_id("icon", "name", "home"), Some(Ok(triple)));
}

#[test]
fn a_bare_name_without_a_default_set_names_the_key() {
    let root = package("no_default_set", "[telar.icons]\nsvg = \"icons\"\n");
    bake(&root, &[icon("mdi:home")]);
    let message = AssetContext::load(&root, VERSION)
        .resolve_id(icon_kind(), "home")
        .unwrap_err();
    assert!(message.contains("`home` names no icon set"), "{message}");
    assert!(message.contains("default_set"), "{message}");
}

#[test]
fn a_literal_is_spelled_in_full_where_icons_resolve_at_run_time() {
    let root = package(
        "runtime_literal",
        "[telar.icons]\nmode = \"runtime\"\ndefault_set = \"mdi\"\n",
    );
    let context = AssetContext::load(&root, VERSION);
    assert_eq!(
        context.literal_id("icon", "name", "home"),
        Some(Ok("\"mdi:home\"".to_string()))
    );
    assert_eq!(
        context.literal_id("icon", "name", "lucide:home"),
        Some(Ok("\"lucide:home\"".to_string()))
    );
}

#[test]
fn a_literal_reaches_the_component_as_written_where_icons_are_not_configured() {
    let root = package("unconfigured", "[telar]\n");
    let context = AssetContext::load(&root, VERSION);
    assert_eq!(context.literal_id("icon", "name", "home"), None);
    assert_eq!(context.literal_id("icon", "size", "24"), None);
}

#[test]
fn a_baked_id_named_from_rust_carries_its_initializer() {
    let root = package("rust_id", "[telar.icons]\nsvg = \"icons\"\n");
    let mut home = icon("mdi:home");
    home.monochrome = true;
    bake(&root, &[home, icon("mdi:gear")]);
    let baked = AssetContext::load(&root, VERSION)
        .baked_id(icon_kind(), "mdi:home")
        .unwrap();
    assert_eq!(
        baked,
        BakedId {
            id: "mdi:home".to_string(),
            monochrome: true,
            data_ty: "SvgData",
            init_expr: "SvgData::from_baked_vector((24.0, 24.0), vec![])".to_string(),
        }
    );
}

#[test]
fn a_bare_name_named_from_rust_is_read_in_the_default_set() {
    let root = package(
        "rust_bare",
        "[telar.icons]\nsvg = \"icons\"\ndefault_set = \"mdi\"\n",
    );
    bake(&root, &[icon("mdi:home")]);
    let baked = AssetContext::load(&root, VERSION)
        .baked_id(icon_kind(), "home")
        .unwrap();
    assert_eq!(baked.id, "mdi:home");
}

#[test]
fn an_id_named_from_rust_and_missing_from_the_artifact_names_the_call_and_the_bake() {
    let root = package("rust_missing", "[telar.icons]\nsvg = \"icons\"\n");
    bake(&root, &[icon("mdi:home")]);
    let message = AssetContext::load(&root, VERSION)
        .baked_id(icon_kind(), "mdi:account")
        .unwrap_err();
    assert!(message.contains("`icon!(\"mdi:account\")`"), "{message}");
    assert!(message.contains("cargo telar bake"), "{message}");
    assert!(!message.starts_with("rsx:"), "{message}");
}

#[test]
fn an_id_named_from_rust_where_nothing_is_baked_names_the_section() {
    for manifest in ["", "[telar.icons]\nmode = \"runtime\"\nsvg = \"icons\"\n"] {
        let root = package("rust_unconfigured", manifest);
        let message = AssetContext::load(&root, VERSION)
            .baked_id(icon_kind(), "mdi:home")
            .unwrap_err();
        assert!(message.contains("[telar.icons]"), "{message}");
    }
}
