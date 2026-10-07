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
    }
}

fn icon_kind() -> &'static AssetKind {
    asset_kind_for_component("icon", "name").expect("icon is a registered component kind")
}

#[test]
fn the_icon_tags_name_prop_is_a_component_kind() {
    assert_eq!(icon_kind().id, "icon");
    assert!(asset_kind_for_component("icon", "size").is_none());
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
            "(\"mdi:home\", ::std::sync::Arc::clone(&crate::__rsx_assets::{}))",
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
    let message = AssetContext::dynamic_id_message(icon_kind());
    assert!(
        message.contains("`icon name:` takes a literal id"),
        "{message}"
    );
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
            },
        ],
    );
    let tracked = AssetContext::load(&root, VERSION).tracked_files();
    assert_eq!(tracked, vec![root.join("assets/mark.svg")]);
}
