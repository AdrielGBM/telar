use super::*;

fn svg_asset(path: &str, content: &[u8]) -> BakedAsset {
    BakedAsset {
        kind: "svg".to_string(),
        path: path.to_string(),
        content: content.to_vec(),
        init_expr: "SvgData::from_baked_vector(&[])".to_string(),
    }
}

fn image_asset(path: &str, content: &[u8]) -> BakedAsset {
    BakedAsset {
        kind: "image".to_string(),
        path: path.to_string(),
        content: content.to_vec(),
        init_expr: "ImageData::new(vec![0; 4], 1, 1)".to_string(),
    }
}

#[test]
fn index_round_trips_through_json() {
    let index = AssetIndex {
        format: ASSET_ARTIFACT_FORMAT,
        producer: "cargo-telar 0.1.8".to_string(),
        telar_version: "0.1.8".to_string(),
        entries: vec![AssetEntry {
            kind: "svg".to_string(),
            path: "badge.svg".to_string(),
            hash: content_hash(b"<svg/>"),
            static_name: static_name_for_path("badge.svg"),
        }],
    };
    let parsed = AssetIndex::from_json(&index.to_json()).unwrap();
    assert_eq!(parsed, index);
}

#[test]
fn from_json_reports_a_malformed_index() {
    let err = AssetIndex::from_json("{ not json").unwrap_err();
    assert!(err.contains("malformed asset index"), "{err}");
}

#[test]
fn content_hash_reflects_bytes_not_path() {
    assert_eq!(content_hash(b"hello"), content_hash(b"hello"));
    assert_ne!(content_hash(b"hello"), content_hash(b"world"));
}

#[test]
fn static_name_is_stable_across_slash_styles() {
    assert_eq!(
        static_name_for_path("icons/badge.svg"),
        static_name_for_path("icons\\badge.svg")
    );
}

#[test]
fn generates_one_static_per_asset_with_the_right_wrapper_type() {
    let asset = svg_asset("icons/badge.svg", b"<svg/>");
    let generated =
        generate_assets(std::slice::from_ref(&asset), "cargo-telar 0.1.8", "0.1.8").unwrap();
    let name = static_name_for_path("icons/badge.svg");

    assert!(
        generated
            .source
            .contains(&format!("pub static {name}: LazyLock<Arc<SvgData>>")),
        "{}",
        generated.source
    );
    assert!(
        generated.source.contains("use telar::*;"),
        "{}",
        generated.source
    );
    assert_eq!(generated.index.entries.len(), 1);
    assert_eq!(generated.index.entries[0].static_name, name);
    assert_eq!(generated.index.entries[0].hash, content_hash(b"<svg/>"));
    assert_eq!(generated.index.format, ASSET_ARTIFACT_FORMAT);
}

/// A baker chooses the types its `init_expr` names, and nothing about the asset kind predicts them — an SVG bakes into `VectorCommand`/`PathData`/`Point`, none of which is its `data_ty`. Importing only the kinds in play left the module referring to types it had not imported.
#[test]
fn the_generated_module_imports_telar_wholesale() {
    let asset = BakedAsset {
        kind: "svg".to_string(),
        path: "a.svg".to_string(),
        content: b"1".to_vec(),
        init_expr: "SvgData::from_baked_vector((1.0, 1.0), vec![VectorCommand::Path { data: PathData::new() }])"
            .to_string(),
    };
    let generated = generate_assets(&[asset], "test", "0.1.8").unwrap();
    assert!(
        generated.source.contains("use telar::*;"),
        "{}",
        generated.source
    );
}

#[test]
fn static_names_are_independent_of_entry_order() {
    let badge = svg_asset("badge.svg", b"<svg/>");
    let logo = image_asset("logo.png", b"PNGDATA");

    let forward = generate_assets(&[badge.clone(), logo.clone()], "t", "0.1.8").unwrap();
    let backward = generate_assets(&[logo, badge], "t", "0.1.8").unwrap();

    assert_eq!(forward.source, backward.source);
    assert_eq!(forward.index.entries, backward.index.entries);
}

#[test]
fn adding_an_asset_does_not_rename_the_others() {
    let badge = svg_asset("badge.svg", b"<svg/>");
    let logo = image_asset("logo.png", b"PNGDATA");
    let before = generate_assets(&[badge.clone(), logo.clone()], "t", "0.1.8").unwrap();
    let badge_name = |g: &GeneratedAssets| {
        g.index
            .entries
            .iter()
            .find(|e| e.path == "badge.svg")
            .unwrap()
            .static_name
            .clone()
    };
    let logo_name = |g: &GeneratedAssets| {
        g.index
            .entries
            .iter()
            .find(|e| e.path == "logo.png")
            .unwrap()
            .static_name
            .clone()
    };

    let icon = svg_asset("icon.svg", b"<svg id=\"x\"/>");
    let after = generate_assets(&[badge, logo, icon], "t", "0.1.8").unwrap();

    assert_eq!(badge_name(&before), badge_name(&after));
    assert_eq!(logo_name(&before), logo_name(&after));
    assert_eq!(after.index.entries.len(), 3);
}

#[test]
fn duplicate_paths_are_rejected() {
    let err = generate_assets(
        &[svg_asset("badge.svg", b"1"), svg_asset("badge.svg", b"2")],
        "t",
        "0.1.8",
    )
    .unwrap_err();
    assert!(err.contains("badge.svg"), "{err}");
}

#[test]
fn unknown_kinds_are_rejected() {
    let asset = BakedAsset {
        kind: "font".to_string(),
        path: "sans.ttf".to_string(),
        content: b"1".to_vec(),
        init_expr: "x".to_string(),
    };
    let err = generate_assets(&[asset], "t", "0.1.8").unwrap_err();
    assert!(err.contains("font"), "{err}");
}

#[test]
fn read_index_of_a_never_baked_package_is_none() {
    let dir = std::env::temp_dir().join(format!(
        "rsx_assets_artifact_missing_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    assert!(
        read_index(&dir).unwrap().is_none(),
        "a package that was never baked has no index, which is not an error"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn write_then_read_round_trips() {
    let dir = std::env::temp_dir().join(format!("rsx_assets_artifact_rw_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let telar_dir = dir.join(".telar");

    let generated = generate_assets(
        &[svg_asset("badge.svg", b"<svg/>")],
        "cargo-telar 0.1.8",
        "0.1.8",
    )
    .unwrap();
    write_generated(&telar_dir, &generated).unwrap();

    let read_back = read_index(&telar_dir).unwrap().unwrap();
    assert_eq!(read_back, generated.index);
    let source_on_disk = std::fs::read_to_string(telar_dir.join(ASSETS_SOURCE_FILENAME)).unwrap();
    assert_eq!(source_on_disk, generated.source);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_artifact_of_a_never_baked_package_is_not_baked() {
    let dir = std::env::temp_dir().join(format!(
        "rsx_assets_handshake_missing_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    assert_eq!(
        check_artifact(&dir, "0.1.8").unwrap(),
        ArtifactHandshake::NotBaked
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_artifact_of_a_matching_index_is_up_to_date() {
    let dir = std::env::temp_dir().join(format!("rsx_assets_handshake_ok_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let telar_dir = dir.join(".telar");

    let generated = generate_assets(
        &[svg_asset("badge.svg", b"<svg/>")],
        "cargo-telar 0.1.8",
        "0.1.8",
    )
    .unwrap();
    write_generated(&telar_dir, &generated).unwrap();

    assert_eq!(
        check_artifact(&telar_dir, "0.1.8").unwrap(),
        ArtifactHandshake::UpToDate(generated.index)
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_artifact_reports_a_telar_version_mismatch() {
    let dir = std::env::temp_dir().join(format!(
        "rsx_assets_handshake_version_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let telar_dir = dir.join(".telar");

    let generated = generate_assets(
        &[svg_asset("badge.svg", b"<svg/>")],
        "cargo-telar 0.1.8",
        "0.1.8",
    )
    .unwrap();
    write_generated(&telar_dir, &generated).unwrap();

    assert_eq!(
        check_artifact(&telar_dir, "0.2.0").unwrap(),
        ArtifactHandshake::VersionMismatch {
            found: "0.1.8".to_string(),
            expected: "0.2.0".to_string(),
        }
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_artifact_reports_a_format_mismatch_without_regard_to_version() {
    let dir = std::env::temp_dir().join(format!(
        "rsx_assets_handshake_format_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let telar_dir = dir.join(".telar");
    std::fs::create_dir_all(&telar_dir).unwrap();

    let stale_index = AssetIndex {
        format: ASSET_ARTIFACT_FORMAT + 1,
        producer: "cargo-telar 0.1.8".to_string(),
        telar_version: "0.1.8".to_string(),
        entries: vec![],
    };
    std::fs::write(telar_dir.join(ASSETS_INDEX_FILENAME), stale_index.to_json()).unwrap();

    assert_eq!(
        check_artifact(&telar_dir, "0.1.8").unwrap(),
        ArtifactHandshake::FormatMismatch {
            found: ASSET_ARTIFACT_FORMAT + 1,
            expected: ASSET_ARTIFACT_FORMAT,
        }
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_artifact_surfaces_a_corrupt_index_as_an_error_not_a_variant() {
    let dir = std::env::temp_dir().join(format!(
        "rsx_assets_handshake_corrupt_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let telar_dir = dir.join(".telar");
    std::fs::create_dir_all(&telar_dir).unwrap();
    std::fs::write(telar_dir.join(ASSETS_INDEX_FILENAME), "{ not json").unwrap();

    let err = check_artifact(&telar_dir, "0.1.8").unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn write_generated_skips_disk_io_when_content_is_unchanged() {
    use std::os::unix::fs::PermissionsExt;

    let dir = std::env::temp_dir().join(format!(
        "rsx_assets_artifact_unchanged_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let telar_dir = dir.join(".telar");

    let generated = generate_assets(
        &[svg_asset("badge.svg", b"<svg/>")],
        "cargo-telar 0.1.8",
        "0.1.8",
    )
    .unwrap();
    write_generated(&telar_dir, &generated).unwrap();

    // A second write with identical content would fail with "permission denied" instead of silently succeeding, proving the skip without racing on mtime.
    for name in [ASSETS_INDEX_FILENAME, ASSETS_SOURCE_FILENAME] {
        let path = telar_dir.join(name);
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o444);
        std::fs::set_permissions(&path, perms).unwrap();
    }

    write_generated(&telar_dir, &generated).expect("unchanged content must not be rewritten");

    for name in [ASSETS_INDEX_FILENAME, ASSETS_SOURCE_FILENAME] {
        let path = telar_dir.join(name);
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&path, perms).unwrap();
    }

    let _ = std::fs::remove_dir_all(&dir);
}
