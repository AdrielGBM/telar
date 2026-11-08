use super::*;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use telar_diagnostics::Severity;

const ICON_SVG: &[u8] = include_bytes!("../../telar-baker/fixtures/icon.svg");

/// A crate laid out the way an editor finds one, with an asset referenced but never baked.
fn unbaked_crate(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_analyzer_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"probe\"\n").unwrap();
    std::fs::write(root.join("assets/badge.svg"), ICON_SVG).unwrap();
    root
}

/// A `src:"…"` the editor sees before any build has must not become a `compile_error!` telling the user to go and run a command.
#[test]
fn a_never_baked_asset_is_baked_rather_than_mirrored_as_an_error() {
    let root = unbaked_crate("never_baked");
    let source = "[view]\ncol\n    svg src:\"badge.svg\"\n";
    let rsx = root.join("src/view.rsx");
    std::fs::write(&rsx, source).unwrap();
    let document = telar_parser::parse(source).unwrap();

    Bakes::editor().bake_if_unanswered(&rsx, source, &document);

    assert!(
        root.join(".telar/assets.json").is_file(),
        "the editor should have baked the artifact it found missing"
    );
    let mirrored = generated_target(&rsx, source, None).unwrap().code;
    assert!(
        mirrored.contains("crate::__rsx_assets::ASSET_"),
        "the mirror should reference the baked static:\n{mirrored}"
    );
    assert!(
        !mirrored.contains("compile_error!"),
        "the mirror should carry no asset error:\n{mirrored}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// A path naming nothing is the common editor state — it is what a half-typed one looks like — and no amount of baking resolves it, so it must not trigger one on every keystroke.
#[test]
fn a_src_naming_no_file_bakes_nothing() {
    let root = unbaked_crate("no_such_file");
    let source = "[view]\ncol\n    svg src:\"nope.svg\"\n";
    let rsx = root.join("src/view.rsx");
    std::fs::write(&rsx, source).unwrap();
    let document = telar_parser::parse(source).unwrap();

    Bakes::editor().bake_if_unanswered(&rsx, source, &document);

    assert!(
        !root.join(".telar/assets.json").exists(),
        "a missing file is not something a bake can supply"
    );
    let mirrored = generated_target(&rsx, source, None).unwrap().code;
    assert!(mirrored.contains("compile_error!"), "{mirrored}");

    let _ = std::fs::remove_dir_all(&root);
}

/// The counterpart: once the file exists, the very next sync must pick it up. Suppressing the bake for a missing file must not also suppress it for one that has just appeared.
#[test]
fn an_asset_created_after_a_failed_sync_is_picked_up() {
    let root = unbaked_crate("appears_later");
    let source = "[view]\ncol\n    svg src:\"later.svg\"\n";
    let rsx = root.join("src/view.rsx");
    std::fs::write(&rsx, source).unwrap();
    let document = telar_parser::parse(source).unwrap();
    Bakes::editor().bake_if_unanswered(&rsx, source, &document);
    assert!(
        !root.join(".telar/assets.json").exists(),
        "the failed sync must leave no index behind"
    );

    std::fs::write(root.join("assets/later.svg"), ICON_SVG).unwrap();
    Bakes::editor().bake_if_unanswered(&rsx, source, &document);

    let mirrored = generated_target(&rsx, source, None).unwrap().code;
    assert!(
        mirrored.contains("crate::__rsx_assets::ASSET_"),
        "{mirrored}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// An edit to the asset itself, which leaves the index listing it at an old hash.
#[test]
fn an_asset_edited_since_the_bake_is_rebaked() {
    let root = unbaked_crate("stale");
    let source = "[view]\ncol\n    svg src:\"badge.svg\"\n";
    let rsx = root.join("src/view.rsx");
    std::fs::write(&rsx, source).unwrap();
    let document = telar_parser::parse(source).unwrap();
    Bakes::editor().bake_if_unanswered(&rsx, source, &document);
    let first = std::fs::read_to_string(root.join(".telar/assets.json")).unwrap();

    std::fs::write(root.join("assets/badge.svg"), b"<svg viewBox=\"0 0 9 9\"/>").unwrap();
    Bakes::editor().bake_if_unanswered(&rsx, source, &document);

    let second = std::fs::read_to_string(root.join(".telar/assets.json")).unwrap();
    assert_ne!(first, second, "the hash should have been rewritten");
    let mirrored = generated_target(&rsx, source, None).unwrap().code;
    assert!(!mirrored.contains("compile_error!"), "{mirrored}");

    let _ = std::fs::remove_dir_all(&root);
}

/// The single-writer rule for the map `cargo telar check` reads: the transpiler's `.rs.map` pins diagnostics to the Rust it compiled, so a keystroke must not replace it, and the buffer's own map is returned for the analyzer to keep in memory rather than written anywhere.
#[test]
fn an_open_buffers_map_is_returned_and_never_written_to_disk() {
    let root = unbaked_crate("live_map");
    let rsx = root.join("src/view.rsx");
    let built = "[view]\ntext \"what a build produced\"\n";
    std::fs::write(&rsx, built).unwrap();
    sync_build_file(&rsx, built, None);
    let transpilers = root.join(".telar/build/view.rs.map");
    std::fs::write(&transpilers, "what the transpiler wrote").unwrap();

    let unsaved = "[view]\ntext \"what the editor is showing\"\n\ntext \"more\"\n";
    let (build_path, live) = sync_build_file(&rsx, unsaved, None).unwrap();

    assert_eq!(build_path, root.join(".telar/build/view.rs"));
    assert_eq!(
        std::fs::read_to_string(&transpilers).unwrap(),
        "what the transpiler wrote"
    );
    assert!(!root.join(".telar/live").exists());
    let expected = generated_target(&rsx, unsaved, None).unwrap().map.to_json();
    assert_eq!(live.to_json(), expected);
    let (source, map) = rsx_source_and_map(&build_path, Some(&live)).unwrap();
    assert_eq!(source, rsx);
    assert_eq!(map.to_json(), expected);

    let _ = std::fs::remove_dir_all(&root);
}

/// A component with no open buffer has no live map; reverse-mapping it falls back to the transpiler's, so a map left stale by a closed buffer is never consulted.
#[test]
fn a_component_never_opened_maps_through_the_transpilers_map() {
    let root = unbaked_crate("fallback_map");
    let rsx = root.join("src/view.rsx");
    let source = "[view]\ntext \"a\"\n";
    std::fs::write(&rsx, source).unwrap();
    let map = generated_target(&rsx, source, None).unwrap().map;
    let build = root.join(".telar/build");
    std::fs::create_dir_all(&build).unwrap();
    std::fs::write(build.join("view.rs.map"), map.to_json()).unwrap();

    let (found, read) = rsx_source_and_map(&build.join("view.rs"), None).unwrap();

    assert_eq!(found, rsx);
    assert_eq!(read.to_json(), map.to_json());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_telar_toml_that_does_not_load_is_not_mirrored_against_the_defaults() {
    let root = unbaked_crate("broken_manifest");
    std::fs::write(
        root.join(telar_project::MANIFEST_FILENAME),
        "[telar]\nasets = \"art\"\n",
    )
    .unwrap();
    let source = "[view]\ncol\n    svg src:\"badge.svg\"\n";
    let rsx = root.join("src/view.rsx");
    std::fs::write(&rsx, source).unwrap();

    let synced = sync_build_file(&rsx, source, None);
    let mirrored = generated_target(&rsx, source, None);

    assert!(synced.is_none());
    assert!(mirrored.is_none());
    assert!(
        !root.join(".telar").exists(),
        "nothing is baked or written from a manifest that does not load"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_buffer_the_transpiler_refuses_is_told_apart_from_one_with_nothing_to_mirror() {
    let root = unbaked_crate("refused");
    let rsx = root.join("src/view.rsx");

    let Err(NoTarget::Refused(message)) = target_or_refusal(&rsx, "[logic]\nlet x = 1;\n", None)
    else {
        panic!("a `.rsx` with no view is refused");
    };
    assert!(message.contains("view.preview.rsx"), "{message}");

    assert!(target_or_refusal(&rsx, "[view]\nbox\n", None).is_ok());
    assert!(matches!(
        target_or_refusal(&root.join("outside/view.rsx"), "[view]\nbox\n", None),
        Err(NoTarget::Unavailable)
    ));

    let _ = std::fs::remove_dir_all(&root);
}

const BROKEN_SVG: &[u8] = b"this is not an svg";

fn counting_bakes() -> (Bakes, Arc<AtomicUsize>) {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let bakes = Bakes::new(move |package_dir, version| {
        seen.fetch_add(1, Ordering::SeqCst);
        telar_baker::bake_package(package_dir, "test", version)
    });
    (bakes, count)
}

#[test]
fn an_undecodable_asset_is_reported_on_the_line_that_references_it() {
    let root = unbaked_crate("undecodable");
    std::fs::write(root.join("assets/badge.svg"), BROKEN_SVG).unwrap();
    let source = "[view]\ncol\n    text \"hi\"\n    svg src:\"badge.svg\"\n";
    let rsx = root.join("src/view.rsx");
    std::fs::write(&rsx, source).unwrap();
    let document = telar_parser::parse(source).unwrap();

    let diagnostics = Bakes::editor().bake_if_unanswered(&rsx, source, &document);

    let [diagnostic] = diagnostics.as_slice() else {
        panic!("one problem expected: {diagnostics:?}");
    };
    assert_eq!(diagnostic.span.line, 4);
    assert!(matches!(diagnostic.severity, Severity::Error));
    assert!(
        diagnostic.message.contains("cannot bake") && diagnostic.message.contains("badge.svg"),
        "{}",
        diagnostic.message
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_failed_bake_is_not_repeated_while_the_bytes_stay_the_same() {
    let root = unbaked_crate("failed_once");
    std::fs::write(root.join("assets/badge.svg"), BROKEN_SVG).unwrap();
    let source = "[view]\ncol\n    svg src:\"badge.svg\"\n";
    let rsx = root.join("src/view.rsx");
    std::fs::write(&rsx, source).unwrap();
    let document = telar_parser::parse(source).unwrap();
    let (bakes, count) = counting_bakes();

    let first = bakes.bake_if_unanswered(&rsx, source, &document);
    let second = bakes.bake_if_unanswered(&rsx, source, &document);

    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(!first.is_empty());
    assert_eq!(
        first
            .iter()
            .map(|d| (&d.message, d.span.line))
            .collect::<Vec<_>>(),
        second
            .iter()
            .map(|d| (&d.message, d.span.line))
            .collect::<Vec<_>>(),
        "the remembered failure is reported again"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn changed_bytes_are_baked_again_and_a_fixed_asset_reports_nothing() {
    let root = unbaked_crate("failed_then_fixed");
    std::fs::write(root.join("assets/badge.svg"), BROKEN_SVG).unwrap();
    let source = "[view]\ncol\n    svg src:\"badge.svg\"\n";
    let rsx = root.join("src/view.rsx");
    std::fs::write(&rsx, source).unwrap();
    let document = telar_parser::parse(source).unwrap();
    let (bakes, count) = counting_bakes();
    bakes.bake_if_unanswered(&rsx, source, &document);

    std::fs::write(root.join("assets/badge.svg"), b"<svg viewBox=\"0 0 9 9\"/>").unwrap();
    let diagnostics = bakes.bake_if_unanswered(&rsx, source, &document);

    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(root.join(".telar/assets.json").is_file());
    let _ = std::fs::remove_dir_all(&root);
}
