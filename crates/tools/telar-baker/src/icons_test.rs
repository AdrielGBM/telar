use std::path::PathBuf;

use telar_project::{IconLicensesSection, asset_kind_for_id};

use super::*;

const OWN_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect width="16" height="16" fill="currentColor"/></svg>"#;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/iconify")
}

fn package(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("telar_baker_icons_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("icons/app")).unwrap();
    std::fs::write(root.join("icons/app/logo.svg"), OWN_SVG).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo-app\"\n").unwrap();
    root
}

fn resolve(package_dir: &Path, section: &IconsSection, refs: &[IdRef]) -> ResolvedIcons {
    resolve_with(package_dir, section, refs, Recording::default())
}

fn section() -> IconsSection {
    IconsSection {
        iconify: Some(fixtures().display().to_string()),
        svg: Some("icons".to_string()),
        ..IconsSection::default()
    }
}

fn literal(id: &str, line: usize) -> IdRef {
    IdRef {
        kind: asset_kind_for_id("icon").unwrap(),
        literal: Some(id.to_string()),
        written: format!("\"{id}\""),
        file: PathBuf::from("src/app.rsx"),
        line,
    }
}

fn baked_ids(resolved: &ResolvedIcons) -> Vec<String> {
    resolved
        .icons
        .iter()
        .map(|icon| icon.id.to_string())
        .collect()
}

#[test]
fn own_svgs_and_iconify_sets_resolve_and_are_recorded() {
    let root = package("record");
    let resolved = resolve(
        &root,
        &section(),
        &[
            literal("demo:home", 1),
            literal("app:logo", 2),
            literal("demo:home", 3),
        ],
    );
    assert!(resolved.errors.is_empty(), "{:?}", resolved.errors);
    assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);
    assert_eq!(baked_ids(&resolved), vec!["app:logo", "demo:home"]);

    let record = read_icon_record(&root).expect("a record is written");
    assert!(record.sets["app"].own);
    assert_eq!(
        record.sets["demo"]
            .info
            .as_ref()
            .unwrap()
            .license
            .as_ref()
            .unwrap()
            .spdx
            .as_deref(),
        Some("MIT")
    );
    assert!(record.icons["demo:home"].origin.ends_with("demo.json"));

    let notice = std::fs::read_to_string(root.join(".telar").join(ICONS_NOTICE_FILENAME)).unwrap();
    assert!(notice.starts_with("Icons in demo-app"), "{notice}");
    assert!(
        notice.contains("demo — Demo Icons\n  Licence: MIT"),
        "{notice}"
    );
    assert!(
        notice.contains("app\n  demo-app's own artwork.\n  Icons: logo"),
        "{notice}"
    );
}

#[test]
fn an_own_svg_redraws_a_sets_icon_under_its_name() {
    let root = package("override");
    std::fs::create_dir_all(root.join("icons/demo")).unwrap();
    std::fs::write(root.join("icons/demo/home.svg"), OWN_SVG).unwrap();
    let resolved = resolve(&root, &section(), &[literal("demo:home", 1)]);
    assert_eq!(resolved.icons[0].svg, OWN_SVG.as_bytes());
}

#[test]
fn an_id_no_source_has_is_an_error_naming_the_sources() {
    let root = package("missing");
    let resolved = resolve(&root, &section(), &[literal("demo:nothing", 7)]);
    assert!(resolved.icons.is_empty());
    let error = &resolved.errors[0];
    assert!(error.starts_with("src/app.rsx:7:"), "{error}");
    assert!(error.contains("`demo:nothing`"), "{error}");
    assert!(error.contains("the SVG folder"), "{error}");
    assert!(error.contains("the Iconify sets in"), "{error}");
}

#[test]
fn a_bare_name_without_a_default_set_is_an_error() {
    let root = package("invalid");
    let resolved = resolve(&root, &section(), &[literal("home", 4)]);
    assert!(
        resolved.errors[0].contains("src/app.rsx:4"),
        "{:?}",
        resolved.errors
    );
    assert!(
        resolved.errors[0].contains("set:name"),
        "{:?}",
        resolved.errors
    );
    assert!(
        resolved.errors[0].contains("[telar.icons] default_set"),
        "{:?}",
        resolved.errors
    );
    assert!(resolved.icons.is_empty());
}

#[test]
fn a_bare_name_resolves_in_the_default_set_and_is_baked_under_its_full_id() {
    let root = package("default_set");
    let with_default = IconsSection {
        default_set: Some("demo".to_string()),
        ..section()
    };
    let resolved = resolve(
        &root,
        &with_default,
        &[
            literal("home", 1),
            literal("demo:home", 2),
            literal("app:logo", 3),
        ],
    );
    assert!(resolved.errors.is_empty(), "{:?}", resolved.errors);
    assert_eq!(baked_ids(&resolved), vec!["app:logo", "demo:home"]);
    assert_eq!(
        read_icon_record(&root)
            .unwrap()
            .icons
            .keys()
            .collect::<Vec<_>>(),
        vec!["app:logo", "demo:home"]
    );
}

fn monochrome(resolved: &ResolvedIcons, id: &str) -> bool {
    resolved
        .icons
        .iter()
        .find(|icon| icon.id.to_string() == id)
        .unwrap_or_else(|| panic!("`{id}` was not baked"))
        .monochrome
}

#[test]
fn the_sets_palette_decides_whether_an_icon_is_tinted() {
    let root = package("palette");
    let resolved = resolve(
        &root,
        &section(),
        &[
            literal("brand:acme", 1),
            literal("mono:dot", 2),
            literal("demo:home", 3),
            literal("demo:badge", 4),
            literal("app:logo", 5),
        ],
    );
    assert!(resolved.errors.is_empty(), "{:?}", resolved.errors);
    assert!(
        !monochrome(&resolved, "brand:acme"),
        "a one-colour logo of a palette set keeps its colour"
    );
    assert!(
        monochrome(&resolved, "mono:dot"),
        "a set declaring `palette: false` is tinted whatever its markup"
    );
    assert!(
        monochrome(&resolved, "demo:home"),
        "a set declaring no palette is tinted where it draws in currentColor"
    );
    assert!(
        !monochrome(&resolved, "demo:badge"),
        "a set declaring no palette keeps a fixed colour"
    );
    assert!(
        monochrome(&resolved, "app:logo"),
        "an own SVG in currentColor is tinted"
    );
    let record = read_icon_record(&root).unwrap();
    assert_eq!(
        record.sets["brand"].info.as_ref().unwrap().palette,
        Some(true)
    );
}

#[test]
fn an_attribution_set_warns_and_still_bakes() {
    let root = package("attrib_warn");
    let resolved = resolve(&root, &section(), &[literal("attrib:dot", 1)]);
    assert_eq!(baked_ids(&resolved), vec!["attrib:dot"]);
    assert!(
        resolved.warnings[0].contains("CC-BY-4.0"),
        "{:?}",
        resolved.warnings
    );
}

#[test]
fn a_refused_set_is_an_error_and_is_not_baked() {
    let root = package("attrib_fail");
    let strict = IconsSection {
        licenses: IconLicensesSection {
            allow: Vec::new(),
            unlisted: Some(UnlistedLicense::Fail),
        },
        ..section()
    };
    let resolved = resolve(
        &root,
        &strict,
        &[literal("attrib:dot", 1), literal("demo:home", 2)],
    );
    assert_eq!(baked_ids(&resolved), vec!["demo:home"]);
    assert!(
        resolved.errors[0].contains("unlisted = \"fail\""),
        "{:?}",
        resolved.errors
    );
    let record = read_icon_record(&root).unwrap();
    assert!(!record.sets.contains_key("attrib"));
}

#[test]
fn the_allowlist_accepts_an_attribution_set() {
    let root = package("attrib_allowed");
    let allowed = IconsSection {
        licenses: IconLicensesSection {
            allow: vec!["CC-BY-4.0".to_string()],
            unlisted: Some(UnlistedLicense::Fail),
        },
        ..section()
    };
    let resolved = resolve(&root, &allowed, &[literal("attrib:dot", 1)]);
    assert!(resolved.errors.is_empty() && resolved.warnings.is_empty());
}

#[test]
fn a_provider_icon_already_fetched_is_reused_without_the_network() {
    let root = package("cached");
    let provider = "http://127.0.0.1:9";
    let id = IconId::parse("remote:bell").unwrap();
    std::fs::create_dir_all(root.join(".telar/icons/remote")).unwrap();
    std::fs::write(root.join(".telar/icons/remote/bell.svg"), OWN_SVG).unwrap();
    let record = IconRecord {
        format: RECORD_FORMAT,
        provider: Some(provider.to_string()),
        sets: [(
            "remote".to_string(),
            RecordedSet {
                own: false,
                info: Some(SetInfo {
                    license: Some(icons_core::License {
                        spdx: Some("MIT".to_string()),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            },
        )]
        .into(),
        icons: [(
            id.to_string(),
            RecordedIcon {
                origin: format!("{provider}/remote.json?icons=bell"),
                from_provider: true,
            },
        )]
        .into(),
        dependencies: BTreeMap::new(),
    };
    write_record(&root, &record, false).unwrap();
    let from_provider = IconsSection {
        provider: Some(provider.to_string()),
        ..IconsSection::default()
    };
    let resolved = resolve(&root, &from_provider, &[literal("remote:bell", 1)]);
    assert!(resolved.errors.is_empty(), "{:?}", resolved.errors);
    assert_eq!(
        resolved.icons,
        vec![BakedIcon {
            id,
            svg: OWN_SVG.as_bytes().to_vec(),
            monochrome: true,
        }]
    );
}

#[test]
fn nothing_to_bake_clears_the_record_the_notice_and_the_cache() {
    let root = package("cleared");
    resolve(&root, &section(), &[literal("demo:home", 1)]);
    std::fs::create_dir_all(root.join(".telar/icons/remote")).unwrap();
    std::fs::write(root.join(".telar/icons/remote/bell.svg"), OWN_SVG).unwrap();
    resolve(&root, &section(), &[]);
    assert!(read_icon_record(&root).is_none());
    assert!(!root.join(".telar").join(ICONS_NOTICE_FILENAME).exists());
    assert!(!root.join(".telar/icons").exists());
}
