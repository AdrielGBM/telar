use super::*;

fn package(name: &str, telar_toml: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "telar_analyzer_project_{name}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"probe\"\n").unwrap();
    std::fs::write(root.join(telar_project::MANIFEST_FILENAME), telar_toml).unwrap();
    root
}

#[test]
fn a_telar_toml_that_does_not_load_is_reported_rather_than_read_as_the_defaults() {
    let root = package("broken", "[telar]\nlibary = true\n");
    let rsx = root.join("src/home.rsx");

    let loaded = ProjectInfo::load(&rsx).expect("the file is inside a package");
    let discovered = ProjectInfo::discover(&rsx);
    let _ = std::fs::remove_dir_all(&root);

    let Err(error) = loaded else {
        panic!("a misspelled key loaded as a project");
    };
    let diagnostic = manifest_diagnostic(&error);
    assert_eq!(diagnostic.severity, telar_diagnostics::Severity::Error);
    assert_eq!(diagnostic.span.line, 1);
    assert!(
        diagnostic.message.contains("libary"),
        "{}",
        diagnostic.message
    );
    assert!(
        diagnostic
            .message
            .contains(telar_project::MANIFEST_FILENAME),
        "{}",
        diagnostic.message
    );
    assert!(discovered.is_none());
}

#[test]
fn a_telar_toml_that_loads_is_the_projects() {
    let root = package("library", "[telar]\nlibrary = true\n");
    let loaded = ProjectInfo::load(&root.join("src/home.rsx"));
    let _ = std::fs::remove_dir_all(&root);

    let project = loaded.unwrap().unwrap_or_else(|e| panic!("{e}"));
    assert!(project.library);
}
