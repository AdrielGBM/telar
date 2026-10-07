use std::path::PathBuf;

use super::*;

const MDI: &str = r#"{
    "prefix": "mdi",
    "width": 24,
    "height": 24,
    "info": { "name": "Material Design Icons", "license": { "title": "Apache 2.0", "spdx": "Apache-2.0" } },
    "icons": { "home": { "body": "<path d=\"M10 20v-6h4v6\"/>" } },
    "aliases": { "house": { "parent": "home" } }
}"#;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("telar_iconify_dir_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: PathBuf, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn id(text: &str) -> IconId {
    IconId::parse(text).unwrap()
}

#[test]
fn reads_a_set_saved_on_its_own() {
    let root = scratch("flat");
    write(root.join("mdi.json"), MDI);
    let source = IconifyDir::new(&root);
    let icon = source.icon(&id("mdi:home")).unwrap().unwrap();
    assert!(icon.svg.contains("viewBox=\"0 0 24 24\""), "{}", icon.svg);
    assert!(icon.svg.contains("M10 20v-6h4v6"), "{}", icon.svg);
    assert!(!icon.own);
    assert_eq!(
        icon.set.unwrap().license.unwrap().spdx.as_deref(),
        Some("Apache-2.0")
    );
    assert!(icon.origin.ends_with("mdi.json"));
}

#[test]
fn reads_every_installed_layout() {
    for (name, relative) in [
        ("package", "json/mdi.json"),
        ("scoped", "@iconify-json/mdi/icons.json"),
        ("node", "@iconify/json/json/mdi.json"),
    ] {
        let root = scratch(name);
        write(root.join(relative), MDI);
        let source = IconifyDir::new(&root);
        assert!(
            source.icon(&id("mdi:home")).unwrap().is_some(),
            "{relative} should be found"
        );
    }
}

#[test]
fn follows_an_alias() {
    let root = scratch("alias");
    write(root.join("mdi.json"), MDI);
    let source = IconifyDir::new(&root);
    assert_eq!(
        source.icon(&id("mdi:house")).unwrap().unwrap().svg,
        source.icon(&id("mdi:home")).unwrap().unwrap().svg
    );
}

#[test]
fn a_missing_set_or_icon_is_none() {
    let root = scratch("missing");
    write(root.join("mdi.json"), MDI);
    let source = IconifyDir::new(&root);
    assert!(source.icon(&id("tabler:home")).unwrap().is_none());
    assert!(source.icon(&id("mdi:nothing")).unwrap().is_none());
}

#[test]
fn a_set_that_does_not_parse_is_an_error() {
    let root = scratch("broken");
    write(root.join("mdi.json"), "{ not json");
    let error = IconifyDir::new(&root).icon(&id("mdi:home")).unwrap_err();
    assert!(matches!(error, IconError::Read { .. }), "{error}");
}

#[test]
fn a_file_holding_another_set_is_an_error() {
    let root = scratch("mismatch");
    write(root.join("tabler.json"), MDI);
    let error = IconifyDir::new(&root).icon(&id("tabler:home")).unwrap_err();
    assert!(error.to_string().contains("holds the set `mdi`"), "{error}");
}

#[test]
fn a_set_is_read_once() {
    let root = scratch("once");
    write(root.join("mdi.json"), MDI);
    let source = IconifyDir::new(&root);
    let first = source.set("mdi").unwrap().unwrap();
    std::fs::remove_file(root.join("mdi.json")).unwrap();
    let second = source.set("mdi").unwrap().unwrap();
    assert!(Arc::ptr_eq(&first, &second));
}
