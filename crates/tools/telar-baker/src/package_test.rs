use super::*;
use std::path::PathBuf;

const ICON_SVG: &[u8] = include_bytes!("../fixtures/icon.svg");
const DOT_PNG: &[u8] = include_bytes!("../fixtures/dot.png");

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("telar_baker_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn init_expr_round_trips_a_real_baked_entry() {
    let expected = crate::baker_for_id("svg").unwrap().bake(ICON_SVG).unwrap();
    let baked = BakedAsset {
        kind: "svg".to_string(),
        path: "badge.svg".to_string(),
        content: ICON_SVG.to_vec(),
        init_expr: expected.clone(),
    };
    let generated =
        telar_project::generate_assets(std::slice::from_ref(&baked), "p", "1.0.0").unwrap();
    let static_name = telar_project::static_name_for_path("badge.svg");

    assert_eq!(
        init_expr_for_static(&generated.source, &static_name).as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn init_expr_is_none_for_a_static_the_source_never_declared() {
    assert!(
        init_expr_for_static("", "ASSET_MISSING").is_none(),
        "a static the source never declared has no initialiser"
    );
}

#[test]
fn collects_static_refs_including_previews_and_skips_dynamic_or_empty_ones() {
    let src = "[view]\ncol\n    \
               svg src:\"badge.svg\"\n    \
               img src:$dynamic\n    \
               img src:\"\"\n\
               [preview \"demo\"]\ncol\n    img src:\"dot.png\"\n";
    let doc = telar_parser::parse(src).unwrap();
    let mut refs = Vec::new();
    collect_asset_refs(&doc, &mut refs);
    let paths: Vec<_> = refs
        .iter()
        .map(|(kind, path)| (kind.id, path.as_str()))
        .collect();
    assert_eq!(paths, vec![("svg", "badge.svg"), ("image", "dot.png")]);
}

#[test]
fn bakes_only_the_assets_a_document_actually_references() {
    let root = temp_dir("bake_pkg");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/badge.svg"), ICON_SVG).unwrap();
    std::fs::write(root.join("assets/dot.png"), DOT_PNG).unwrap();
    std::fs::write(root.join("assets/unused.svg"), ICON_SVG).unwrap();
    std::fs::write(
        root.join("src/view.rsx"),
        "[view]\ncol\n    svg src:\"badge.svg\"\n    img src:\"dot.png\"\n",
    )
    .unwrap();

    let report = bake_package(&root, "test-producer", "9.9.9").expect("the package has .rsx");
    assert_eq!(report.baked, 2);
    assert!(report.changed, "a first bake is always a change");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);

    let index = telar_project::read_index(&root.join(".telar"))
        .unwrap()
        .expect("a package with references bakes an index");
    assert_eq!(index.producer, "test-producer");
    assert_eq!(index.telar_version, "9.9.9");
    let mut paths: Vec<_> = index.entries.iter().map(|e| e.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(paths, vec!["badge.svg", "dot.png"]);

    let source = std::fs::read_to_string(root.join(".telar/assets.rs")).unwrap();
    assert!(source.contains("SvgData"), "{source}");
    assert!(source.contains("ImageData"), "{source}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_missing_asset_file_is_skipped_with_a_warning_not_a_panic() {
    let root = temp_dir("bake_missing");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(
        root.join("src/view.rsx"),
        "[view]\ncol\n    img src:\"missing.png\"\n",
    )
    .unwrap();

    let report = bake_package(&root, "p", "1.0.0").expect("the package has .rsx");
    assert_eq!(report.baked, 0);
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(
        report.warnings[0].contains("missing.png"),
        "{:?}",
        report.warnings
    );

    let index = telar_project::read_index(&root.join(".telar")).unwrap();
    assert!(
        index.is_none_or(|i| i.entries.is_empty()),
        "a missing file leaves no entry behind"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_package_with_no_rsx_is_left_untouched() {
    let root = temp_dir("bake_no_rsx");
    std::fs::create_dir_all(&root).unwrap();

    assert!(
        bake_package(&root, "p", "1.0.0").is_none(),
        "a package with no .rsx has nothing to bake"
    );
    assert!(
        !root.join(".telar").exists(),
        "so it must not even create the output directory"
    );
    let _ = std::fs::remove_dir_all(&root);
}
