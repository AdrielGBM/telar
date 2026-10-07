use std::path::PathBuf;

use crate::{MANIFEST_FILENAME, TelarManifest};

use super::*;

fn package(name: &str, manifest: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_icons_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(MANIFEST_FILENAME), manifest).unwrap();
    root
}

fn load_error(name: &str, manifest: &str) -> String {
    TelarManifest::load(&package(name, manifest))
        .expect_err("the manifest should be refused")
        .to_string()
}

#[test]
fn every_key_reads() {
    let root = package(
        "full",
        r#"
[telar.icons]
mode = "both"
iconify = "node_modules"
svg = "icons"
provider = "https://icons.example.com"

[telar.icons.licenses]
allow = ["CC-BY-4.0", "mystery"]
unlisted = "fail"
"#,
    );
    let icons = TelarManifest::load(&root).unwrap().telar.icons.unwrap();
    assert_eq!(icons.mode(), IconMode::Both);
    assert_eq!(icons.iconify_dir(&root), Some(root.join("node_modules")));
    assert_eq!(icons.svg_dir(&root), Some(root.join("icons")));
    assert_eq!(icons.provider.as_deref(), Some("https://icons.example.com"));
    assert_eq!(icons.licenses.allow, vec!["CC-BY-4.0", "mystery"]);
    assert_eq!(icons.unlisted_license(), UnlistedLicense::Fail);
}

#[test]
fn baked_and_warn_are_the_defaults() {
    let root = package("defaults", "[telar.icons]\nsvg = \"icons\"\n");
    let icons = TelarManifest::load(&root).unwrap().telar.icons.unwrap();
    assert_eq!(icons.mode(), IconMode::Baked);
    assert_eq!(icons.unlisted_license(), UnlistedLicense::Warn);
}

#[test]
fn a_misspelled_key_is_refused() {
    let message = load_error("typo", "[telar.icons]\nsvgs = \"icons\"\n");
    assert!(message.contains("svgs"), "{message}");
}

#[test]
fn baking_with_no_source_is_refused() {
    let message = load_error("sourceless", "[telar.icons]\nmode = \"baked\"\n");
    assert!(
        message.contains("names nowhere to read them from"),
        "{message}"
    );
}

#[test]
fn runtime_mode_needs_no_source() {
    let root = package("runtime", "[telar.icons]\nmode = \"runtime\"\n");
    let icons = TelarManifest::load(&root).unwrap().telar.icons.unwrap();
    assert_eq!(icons.mode(), IconMode::Runtime);
}

#[test]
fn a_provider_must_be_a_url() {
    let message = load_error(
        "provider",
        "[telar.icons]\nprovider = \"icons.example.com\"\n",
    );
    assert!(message.contains("is not an http(s) URL"), "{message}");
}

#[test]
fn an_unknown_mode_is_refused() {
    let message = load_error("mode", "[telar.icons]\nmode = \"lazy\"\nsvg = \"icons\"\n");
    assert!(message.contains("lazy"), "{message}");
}

#[test]
fn a_package_inherits_the_workspaces_section_whole() {
    let workspace = std::env::temp_dir().join(format!("telar_icons_ws_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&workspace);
    let member = workspace.join("app");
    std::fs::create_dir_all(member.join("src")).unwrap();
    std::fs::write(
        workspace.join("Cargo.toml"),
        "[workspace]\nmembers = [\"app\"]\n",
    )
    .unwrap();
    std::fs::write(
        workspace.join(MANIFEST_FILENAME),
        "[telar.icons]\nsvg = \"icons\"\n",
    )
    .unwrap();
    std::fs::write(
        member.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(member.join(MANIFEST_FILENAME), "[telar]\n").unwrap();
    let icons = TelarManifest::load(&member).unwrap().telar.icons.unwrap();
    assert_eq!(icons.svg_dir(&member), Some(member.join("icons")));
}
