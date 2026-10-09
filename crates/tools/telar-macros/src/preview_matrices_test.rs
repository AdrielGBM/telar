use telar_project::{BuildFlavour, MANIFEST_FILENAME, TelarManifest};

use super::*;

const MATRICES_TOML: &str = r#"
[telar.previews]
viewports = { phone = "390x844" }

[telar.previews.matrices]
themes = { mode = ["paper", "ink"] }
layouts = { dir = ["ltr", "rtl"], viewport = ["phone", "1280x800"], control_size = ["small"], locale = ["ar"], size = [12, 1.5], label = ['"Save"'] }
"#;

fn package(name: &str, telar_toml: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "telar_macros_matrices_{name}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join(MANIFEST_FILENAME), telar_toml).unwrap();
    root
}

fn previews(name: &str, telar_toml: &str) -> PreviewsSection {
    let root = package(name, telar_toml);
    let previews = TelarManifest::load(&root).unwrap().telar.previews;
    let _ = std::fs::remove_dir_all(&root);
    previews
}

fn wired(name: &str, telar_toml: &str, flavour: BuildFlavour, invocation: Invocation) -> String {
    let root = package(name, telar_toml);
    telar_project::ModuleTree::discover(
        &root.join("src"),
        &telar_project::generated_dir(&root, flavour),
        flavour.site_file_name(),
    )
    .write()
    .unwrap();
    let wired = crate::wire_package(root.clone(), "kit".into(), true, flavour, invocation, None)
        .unwrap_or_else(|e| panic!("{e}"))
        .include_stmts
        .to_string();
    let _ = std::fs::remove_dir_all(&root);
    wired
}

#[test]
fn each_named_matrix_becomes_a_constant_table() {
    let tokens = matrices_install(&previews("tokens", MATRICES_TOML), Invocation::App)
        .unwrap()
        .to_string();
    for expected in [
        "static MATRICES : & [:: telar :: preview :: NamedMatrix] = & [",
        ":: telar :: preview :: NamedMatrix :: new (\"layouts\" , & [:: telar :: preview :: Axis :: Locale (& [\"ar\"]) , :: telar :: preview :: Axis :: Dir (& [:: telar :: Direction :: Ltr , :: telar :: Direction :: Rtl]) , :: telar :: preview :: Axis :: Viewport (& [\"phone\" , \"1280x800\"]) , :: telar :: preview :: Axis :: ControlSize (& [:: telar :: ControlSize :: Small]) , :: telar :: preview :: Axis :: Arg (\"label\" , & [\"\\\"Save\\\"\"]) , :: telar :: preview :: Axis :: Arg (\"size\" , & [\"12\" , \"1.5\"])])",
        ":: telar :: preview :: NamedMatrix :: new (\"themes\" , & [:: telar :: preview :: Axis :: Mode (& [\"paper\" , \"ink\"])])",
        ":: telar :: preview :: ViewportPreset :: new (\"phone\" , 390f32 , 844f32)",
        ":: telar :: preview :: __install_matrices (MATRICES , VIEWPORTS)",
    ] {
        assert!(tokens.contains(expected), "{expected}\n\n{tokens}");
    }
    assert!(tokens.starts_with(":: telar :: __previews !"), "{tokens}");
}

#[test]
fn a_package_naming_neither_matrices_nor_viewports_installs_nothing() {
    let tokens = matrices_install(&PreviewsSection::default(), Invocation::App).unwrap();
    assert!(tokens.is_empty(), "{tokens}");
}

#[test]
fn rsx_modules_only_fills_an_empty_slot() {
    let tokens = matrices_install(&previews("modules", MATRICES_TOML), Invocation::Modules)
        .unwrap()
        .to_string();
    assert!(
        tokens.contains("__install_matrices_if_unset (MATRICES , VIEWPORTS)"),
        "{tokens}"
    );
}

#[test]
fn an_application_installs_its_matrices_only_in_a_build_with_previews() {
    let previewing = wired(
        "app_preview",
        MATRICES_TOML,
        BuildFlavour::Preview,
        Invocation::App,
    );
    assert!(
        previewing.contains("__install_matrices (MATRICES"),
        "{previewing}"
    );
    let plain = wired(
        "app_plain",
        MATRICES_TOML,
        BuildFlavour::Plain,
        Invocation::App,
    );
    assert!(!plain.contains("__install_matrices"), "{plain}");
}

#[test]
fn a_library_never_installs_matrices() {
    let library = format!("[telar]\nlibrary = true\n{MATRICES_TOML}");
    let wired = wired(
        "library",
        &library,
        BuildFlavour::Preview,
        Invocation::Modules,
    );
    assert!(!wired.contains("__install_matrices"), "{wired}");
}
