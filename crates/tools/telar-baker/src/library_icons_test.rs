//! A `[telar] library` that bakes icons ships the record of them, and an application built with it lists them in its own notice, judged against its own licence policy, without the library's bake ever running again.

use std::path::{Path, PathBuf};

use telar_baker::{
    ICONS_NOTICE_FILENAME, IconDependency, bake_package, bake_package_with, read_icon_record,
    read_library_icons,
};
use telar_project::ICONS_LIBRARY_RECORD_FILENAME;

const VERSION: &str = "0.2.1";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/iconify")
}

fn write(root: &Path, file: &str, contents: &str) {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn crate_root(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("telar_library_icons_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// A library drawing `demo:star` and `attrib:dot`, baked the way the workspace bakes it.
fn library(name: &str) -> PathBuf {
    let root = crate_root(&format!("lib_{name}"));
    write(
        &root,
        "Cargo.toml",
        "[package]\nname = \"icon-lib\"\nversion = \"0.1.0\"\n",
    );
    write(
        &root,
        "telar.toml",
        &format!(
            "[telar]\nlibrary = true\nprelude = []\n\n[telar.icons]\niconify = \"{}\"\n\n[telar.icons.licenses]\nallow = [\"attrib\"]\n",
            fixtures().display()
        ),
    );
    write(
        &root,
        "src/badge.rsx",
        "[view]\nrow\n    icon name:\"demo:star\"\n    icon name:\"attrib:dot\"\n",
    );
    let report = bake_package(&root, "test", VERSION).expect("the library has .rsx");
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    root
}

fn application(name: &str, telar_toml: &str, view: Option<&str>) -> PathBuf {
    let root = crate_root(&format!("app_{name}"));
    write(
        &root,
        "Cargo.toml",
        "[package]\nname = \"icon-app\"\nversion = \"0.1.0\"\n",
    );
    write(&root, "telar.toml", telar_toml);
    if let Some(view) = view {
        write(&root, "src/app.rsx", view);
    }
    root
}

fn built_with(library: &Path) -> Vec<IconDependency> {
    vec![IconDependency {
        name: "icon-lib".to_string(),
        dir: library.to_path_buf(),
        library: true,
    }]
}

fn notice(root: &Path) -> String {
    std::fs::read_to_string(root.join(".telar").join(ICONS_NOTICE_FILENAME)).unwrap()
}

#[test]
fn a_library_ships_the_record_of_its_own_icons_and_an_application_does_not() {
    let library = library("shipped");
    let icons = read_library_icons(&library)
        .unwrap()
        .expect("the library baked icons");
    assert_eq!(
        icons.icons.into_iter().collect::<Vec<_>>(),
        vec!["attrib:dot", "demo:star"]
    );
    assert!(icons.sets["demo"].info.is_some());
    let shipped =
        std::fs::read_to_string(library.join(".telar").join(ICONS_LIBRARY_RECORD_FILENAME))
            .unwrap();
    assert!(
        !shipped.contains(&fixtures().display().to_string()),
        "the shipped record names no path of the machine that baked it:\n{shipped}"
    );

    let app = application(
        "not_a_library",
        &format!("[telar.icons]\niconify = \"{}\"\n", fixtures().display()),
        Some("[view]\nicon name:\"demo:home\"\n"),
    );
    bake_package(&app, "test", VERSION).unwrap();
    assert!(
        !app.join(".telar")
            .join(ICONS_LIBRARY_RECORD_FILENAME)
            .exists()
    );
}

#[test]
fn the_application_notice_lists_its_own_icons_then_each_librarys() {
    let library = library("merged");
    let app = application(
        "merged",
        &format!(
            "[telar.icons]\niconify = \"{}\"\n\n[telar.icons.licenses]\nallow = [\"CC-BY-4.0\"]\n",
            fixtures().display()
        ),
        Some("[view]\nicon name:\"demo:home\"\n"),
    );
    let report = bake_package_with(&app, "test", VERSION, Some(&built_with(&library))).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);

    let text = notice(&app);
    let (own, from_library) = text
        .split_once("Icons in icon-lib")
        .unwrap_or_else(|| panic!("the library has a section of its own:\n{text}"));
    assert!(own.starts_with("Icons in icon-app"), "{text}");
    assert!(own.contains("Icons: home\n"), "{text}");
    assert!(!own.contains("star"), "{text}");
    assert!(from_library.contains("Icons: star\n"), "{text}");
    assert!(from_library.contains("CC BY 4.0"), "{text}");
    assert!(from_library.contains("Icons: dot\n"), "{text}");

    let record = read_icon_record(&app).unwrap();
    assert_eq!(record.icons.keys().collect::<Vec<_>>(), vec!["demo:home"]);
    assert_eq!(
        record.dependencies["icon-lib"]
            .icons
            .iter()
            .collect::<Vec<_>>(),
        vec!["attrib:dot", "demo:star"]
    );

    let again = bake_package(&app, "test", VERSION).unwrap();
    assert!(again.errors.is_empty(), "{:?}", again.errors);
    assert_eq!(
        notice(&app),
        text,
        "a bake that is not told the dependencies keeps the ones recorded"
    );
}

#[test]
fn the_applications_policy_judges_a_librarys_sets() {
    let library = library("policy");
    let warned = application("warned", "", Some("[view]\ntext \"hi\"\n"));
    let report = bake_package_with(&warned, "test", VERSION, Some(&built_with(&library))).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(
        report.warnings[0].contains("`icon-lib`"),
        "{}",
        report.warnings[0]
    );
    assert!(
        report.warnings[0].contains("attrib"),
        "{}",
        report.warnings[0]
    );

    let refused = application(
        "refused",
        "[telar.icons.licenses]\nunlisted = \"fail\"\n",
        Some("[view]\ntext \"hi\"\n"),
    );
    let report = bake_package_with(&refused, "test", VERSION, Some(&built_with(&library))).unwrap();
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
    assert!(
        report.errors[0].contains("`icon-lib`"),
        "{}",
        report.errors[0]
    );

    let accepted = application(
        "accepted",
        "[telar.icons.licenses]\nallow = [\"CC-BY-4.0\"]\nunlisted = \"fail\"\n",
        Some("[view]\ntext \"hi\"\n"),
    );
    let report =
        bake_package_with(&accepted, "test", VERSION, Some(&built_with(&library))).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert!(
        notice(&accepted).starts_with("Icons in icon-lib"),
        "{}",
        notice(&accepted)
    );
}

#[test]
fn an_application_without_rsx_still_ships_its_libraries_notice() {
    let library = library("no_rsx");
    let app = application("no_rsx", "", None);
    assert!(bake_package_with(&app, "test", VERSION, Some(&[])).is_none());
    let report = bake_package_with(&app, "test", VERSION, Some(&built_with(&library))).unwrap();
    assert_eq!(report.baked, 0);
    assert!(notice(&app).contains("Icons: star"), "{}", notice(&app));
    assert!(!app.join(".telar/assets.json").exists());

    assert!(
        bake_package_with(&app, "test", VERSION, Some(&[])).is_none(),
        "once no dependency ships an icon, nothing is left to report"
    );
    assert!(!app.join(".telar").join(ICONS_NOTICE_FILENAME).exists());
}

#[test]
fn a_library_whose_record_does_not_answer_for_its_artifact_is_named() {
    let library = library("stale");
    let record = library.join(".telar").join(ICONS_LIBRARY_RECORD_FILENAME);
    let original = std::fs::read_to_string(&record).unwrap();

    std::fs::write(&record, original.replace("demo:star", "demo:gear")).unwrap();
    let problem = read_library_icons(&library).unwrap_err();
    assert!(problem.contains("lists other icons"), "{problem}");

    std::fs::remove_file(&record).unwrap();
    let problem = read_library_icons(&library).unwrap_err();
    assert!(problem.contains("attrib:dot, demo:star"), "{problem}");
    assert!(problem.contains(ICONS_LIBRARY_RECORD_FILENAME), "{problem}");

    let app = application("stale", "", Some("[view]\ntext \"hi\"\n"));
    let report = bake_package_with(&app, "test", VERSION, Some(&built_with(&library))).unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("left out of this package's licence notice")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn a_library_that_stops_drawing_icons_stops_shipping_a_record() {
    let library = library("stopped");
    write(&library, "src/badge.rsx", "[view]\ntext \"no icons\"\n");
    bake_package(&library, "test", VERSION).unwrap();
    assert!(
        !library
            .join(".telar")
            .join(ICONS_LIBRARY_RECORD_FILENAME)
            .exists()
    );
    assert_eq!(read_library_icons(&library), Ok(None));
}
