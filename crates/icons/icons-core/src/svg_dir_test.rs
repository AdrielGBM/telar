use std::path::PathBuf;

use super::*;

const LOGO: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M0 0h16v16H0z" fill="currentColor"/></svg>"#;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("telar_svg_dir_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn id(text: &str) -> IconId {
    IconId::parse(text).unwrap()
}

#[test]
fn a_file_per_icon_is_its_own_artwork() {
    let root = scratch("own");
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(root.join("app/logo.svg"), LOGO).unwrap();
    let icon = SvgDir::new(&root).icon(&id("app:logo")).unwrap().unwrap();
    assert_eq!(icon.svg, LOGO);
    assert!(icon.own);
    assert!(icon.set.is_none());
    assert!(icon.origin.ends_with("logo.svg"));
}

#[test]
fn a_set_with_info_is_judged_like_a_published_one() {
    let root = scratch("info");
    std::fs::create_dir_all(root.join("vendor")).unwrap();
    std::fs::write(root.join("vendor/mark.svg"), LOGO).unwrap();
    std::fs::write(
        root.join("vendor/info.json"),
        r#"{"name":"Vendor Marks","license":{"title":"CC BY 4.0","spdx":"CC-BY-4.0"}}"#,
    )
    .unwrap();
    let icon = SvgDir::new(&root)
        .icon(&id("vendor:mark"))
        .unwrap()
        .unwrap();
    assert!(!icon.own);
    let set = icon.set.unwrap();
    assert_eq!(set.name.as_deref(), Some("Vendor Marks"));
    assert_eq!(set.license.unwrap().spdx.as_deref(), Some("CC-BY-4.0"));
}

#[test]
fn a_missing_file_is_none() {
    let root = scratch("missing");
    assert!(SvgDir::new(&root).icon(&id("app:logo")).unwrap().is_none());
}

#[test]
fn a_broken_info_is_an_error() {
    let root = scratch("broken");
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(root.join("app/logo.svg"), LOGO).unwrap();
    std::fs::write(root.join("app/info.json"), "[1, 2]").unwrap();
    assert!(SvgDir::new(&root).icon(&id("app:logo")).is_err());
}
