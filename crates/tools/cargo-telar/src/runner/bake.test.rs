use super::*;

fn package_with_problems() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let write = |file: &str, contents: &str| {
        let path = root.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    );
    write(
        "telar.toml",
        "[telar.icons]\nmode = \"baked\"\nsvg = \"icons\"\n",
    );
    write(".telar/icons.json", "{ not json");
    write(
        "src/app.rsx",
        "[logic]\nlet which = signal(String::from(\"demo:home\"));\n\n[view]\ncol\n    img src:\"missing.png\"\n    icon name:$which\n",
    );
    root
}

#[test]
fn a_bake_prints_its_warnings_what_it_baked_and_its_errors_with_their_source_lines() {
    let root = package_with_problems();
    let report = telar_baker::bake_package(root.path(), "test", "0.0.0").unwrap();

    let lines = asset_lines(&report, "app", root.path());

    let [warning, baked, missing, dynamic] = lines.as_slice() else {
        panic!("four lines expected: {lines:#?}");
    };
    assert!(
        warning.starts_with(&format!(
            "[cargo-telar] warning: in {}, `.telar/icons.json` cannot be read: ",
            root.path().display()
        )),
        "{warning}"
    );
    assert!(
        warning.ends_with("; this bake replaces it with only what it resolves itself"),
        "{warning}"
    );
    assert_eq!(baked, "[cargo-telar] Baked 0 asset(s) for app");
    assert!(
        missing.starts_with(&format!(
            "[cargo-telar] error: app: cannot bake image asset `missing.png`: not found at {} (",
            root.path().join("assets").join("missing.png").display()
        )),
        "{missing}"
    );
    assert!(
        dynamic.starts_with(&format!(
            "[cargo-telar] error: app: {}:7: `name:$which` — `icon name:` takes a literal id",
            Path::new("src").join("app.rsx").display()
        )),
        "{dynamic}"
    );
}

#[test]
fn a_catalog_bake_prints_the_file_and_line_that_does_not_parse() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("locales")).unwrap();
    std::fs::write(root.path().join("locales/en.toml"), "title = \"Settings\n").unwrap();

    let report = telar_baker::bake_catalog(root.path(), "test", "0.0.0")
        .unwrap()
        .unwrap();
    let lines = catalog_lines(&report, "app", root.path());

    let [error] = lines.as_slice() else {
        panic!("one line expected: {lines:#?}");
    };
    assert!(
        error.starts_with(&format!(
            "[cargo-telar] error: app: {}:1: catalog: ",
            Path::new("locales").join("en.toml").display()
        )),
        "{error}"
    );
}

#[test]
fn a_catalog_bake_that_changed_says_how_many_keys_it_baked() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("locales")).unwrap();
    std::fs::write(
        root.path().join("locales/en.toml"),
        "title = \"Settings\"\n",
    )
    .unwrap();

    let report = telar_baker::bake_catalog(root.path(), "test", "0.0.0")
        .unwrap()
        .unwrap();

    assert_eq!(
        catalog_lines(&report, "app", root.path()),
        ["[cargo-telar] Baked 1 translation key(s) for app"]
    );
}

fn member_with_broken_manifest() -> tempfile::TempDir {
    let member = tempfile::tempdir().unwrap();
    std::fs::write(member.path().join("telar.toml"), "[telar]\nlibary = true\n").unwrap();
    member
}

#[test]
fn a_members_broken_manifest_is_reported_with_its_file() {
    let member = member_with_broken_manifest();

    let MemberManifest::Broken(problem) = member_manifest(member.path(), None) else {
        panic!("a broken manifest expected");
    };

    assert_eq!(problem.file, member.path().join("telar.toml"));
    let line = render(&problem, member.path());
    assert!(
        line.starts_with(&member.path().join("telar.toml").display().to_string()),
        "{line}"
    );
    assert!(line.contains("libary"), "{line}");
}

#[test]
fn a_members_broken_manifest_the_dependency_graph_reported_is_not_reported_again() {
    let member = member_with_broken_manifest();
    let metadata = serde_json::json!({
        "packages": [{
            "id": "app",
            "name": "app",
            "manifest_path": member.path().join("Cargo.toml"),
            "source": null,
        }],
        "resolve": { "nodes": [] },
    });
    let graph_problem = telar_baker::DependencyGraph::from_metadata(&metadata).unwrap_err();
    let other = member_with_broken_manifest();

    assert!(matches!(
        member_manifest(member.path(), Some(&graph_problem)),
        MemberManifest::ReportedByGraph
    ));
    assert!(matches!(
        member_manifest(other.path(), Some(&graph_problem)),
        MemberManifest::Broken(_)
    ));
}
