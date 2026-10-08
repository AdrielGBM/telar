use super::*;

fn package(name: &str, telar_toml: &str, icons: &[&str]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_baked_id_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("telar.toml"), telar_toml).unwrap();
    let assets: Vec<telar_project::BakedAsset> = icons
        .iter()
        .map(|id| telar_project::BakedAsset {
            kind: "icon".to_string(),
            path: id.to_string(),
            content: id.as_bytes().to_vec(),
            init_expr: format!("SvgData::from_baked_vector((24.0f32, 24.0f32), vec![]) /* {id} */"),
            monochrome: true,
        })
        .collect();
    let generated =
        telar_project::generate_assets(&assets, "test", crate::ARTIFACT_TELAR_VERSION).unwrap();
    telar_project::write_generated(&root.join(".telar"), &generated).unwrap();
    root
}

fn input(tokens: &str) -> BakedIdInput {
    syn::parse_str(tokens).unwrap()
}

#[test]
fn a_baked_id_inlines_its_data_and_names_its_tint() {
    let root = package("found", "[telar.icons]\nsvg = \"icons\"\n", &["mdi:home"]);
    let expanded = expand_in(input("icon, \"mdi:home\", ::telar"), &root)
        .unwrap()
        .to_string();
    assert!(
        expanded.contains("SvgData :: from_baked_vector"),
        "{expanded}"
    );
    assert!(expanded.contains(":: telar :: SvgData"), "{expanded}");
    assert!(expanded.contains("use :: telar :: *"), "{expanded}");
    assert!(
        expanded.contains("(\"mdi:home\" , :: std :: sync :: Arc :: clone"),
        "{expanded}"
    );
    assert!(expanded.contains(", true)"), "{expanded}");
    assert!(
        expanded.contains("assets.json"),
        "the artifact's index is tracked: {expanded}"
    );
}

#[test]
fn a_bare_name_arrives_spelled_in_full() {
    let root = package(
        "bare",
        "[telar.icons]\nsvg = \"icons\"\ndefault_set = \"mdi\"\n",
        &["mdi:home"],
    );
    let expanded = expand_in(input("icon, \"home\", ::telar"), &root)
        .unwrap()
        .to_string();
    assert!(expanded.contains("(\"mdi:home\" ,"), "{expanded}");
}

#[test]
fn an_id_the_artifact_lacks_is_an_error_on_the_literal_naming_the_bake() {
    let root = package("missing", "[telar.icons]\nsvg = \"icons\"\n", &["mdi:home"]);
    let error = expand_in(input("icon, \"mdi:gear\", ::telar"), &root).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("`icon!(\"mdi:gear\")`"), "{message}");
    assert!(message.contains("cargo telar bake"), "{message}");
}

#[test]
fn a_kind_named_by_path_is_refused() {
    let root = package("path_kind", "", &[]);
    let message = expand_in(input("svg, \"logo.svg\", ::telar"), &root)
        .unwrap_err()
        .to_string();
    assert!(message.contains("`svg`"), "{message}");
}

#[test]
fn a_rebake_is_seen_by_a_process_that_outlives_it() {
    let root = package("rebake", "[telar.icons]\nsvg = \"icons\"\n", &["mdi:home"]);
    assert!(expand_in(input("icon, \"mdi:gear\", ::telar"), &root).is_err());
    std::thread::sleep(std::time::Duration::from_millis(20));
    let generated = telar_project::generate_assets(
        &[telar_project::BakedAsset {
            kind: "icon".to_string(),
            path: "mdi:gear".to_string(),
            content: b"gear".to_vec(),
            init_expr: "SvgData::from_baked_vector((24.0f32, 24.0f32), vec![])".to_string(),
            monochrome: false,
        }],
        "test",
        crate::ARTIFACT_TELAR_VERSION,
    )
    .unwrap();
    telar_project::write_generated(&root.join(".telar"), &generated).unwrap();
    assert!(expand_in(input("icon, \"mdi:gear\", ::telar"), &root).is_ok());
}
