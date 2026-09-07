use super::*;

/// A workspace search must find every crate's `.rsx` and must not walk `target/` — the second half is what makes it usable on every keystroke, since a workspace's build directory dwarfs its sources.
#[test]
fn a_tree_search_crosses_crates_and_skips_what_a_build_wrote() {
    let root = std::env::temp_dir().join(format!("telar_tree_search_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in [
        root.join("crates/ui/src"),
        root.join("crates/modules/src/clock"),
        root.join("crates/modules/.telar/build"),
        root.join("target/debug"),
    ] {
        std::fs::create_dir_all(&dir).unwrap();
    }
    std::fs::write(root.join("crates/ui/src/card.rsx"), "[view]\n").unwrap();
    std::fs::write(root.join("crates/modules/src/clock/clock.rsx"), "[view]\n").unwrap();
    std::fs::write(
        root.join("crates/modules/.telar/build/stale.rsx"),
        "[view]\n",
    )
    .unwrap();
    std::fs::write(root.join("target/debug/vendored.rsx"), "[view]\n").unwrap();

    let found = find_rsx_files_in_tree(&root);
    let names: Vec<_> = found
        .iter()
        .filter_map(|p| p.file_name()?.to_str())
        .collect();
    assert_eq!(
        names,
        vec!["clock.rsx", "card.rsx"],
        "a sibling crate's component is found; target/ and .telar/ are not"
    );

    assert_eq!(find_rsx_files(&root).len(), 4);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn assets_root_default_and_configured() {
    let root = std::env::temp_dir().join(format!("rsx_assets_root_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    assert_eq!(assets_root(&root), root.join("assets"));

    std::fs::write(
        root.join("telar.toml"),
        "[telar]\nassets = \"src/shared/assets\"\n",
    )
    .unwrap();
    assert_eq!(assets_root(&root), root.join("src/shared/assets"));

    std::fs::write(root.join("telar.toml"), "[telar]\nauto_modules = true\n").unwrap();
    assert_eq!(assets_root(&root), root.join("assets"));
    assert!(
        auto_modules_enabled(&root),
        "auto_modules is set in this fixture's telar.toml"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// With `auto_modules` off the crate declares its own `.rs` modules, so the tree must place the `.rsx` files and nothing else — declaring a hand-written one is a redefinition, and declaring a directory whose `mod.rs` the crate already names is one too.
#[test]
fn without_auto_modules_only_the_rsx_files_are_placed() {
    let root = std::env::temp_dir().join(format!("rsx_opt_in_modules_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for d in ["editor", "loose"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    std::fs::write(root.join("editor/mod.rs"), "").unwrap();
    std::fs::write(root.join("editor/act.rs"), "").unwrap();
    std::fs::write(root.join("editor/top_bar.rsx"), "[view]\ncol\n").unwrap();
    std::fs::write(root.join("loose/panel.rsx"), "[view]\ncol\n").unwrap();
    std::fs::write(root.join("helper.rs"), "").unwrap();

    let modtree = root.join("__modules");
    std::fs::create_dir_all(&modtree).unwrap();
    let generated = root.join("build");
    let (out, _) = discover_rust_modules(&root, &root, &modtree, &generated, false).unwrap();

    assert!(
        !out.contains("pub mod helper;"),
        "a hand-written module: {out}"
    );
    assert!(
        !out.contains("pub mod editor;"),
        "a directory the crate declares, and whose own file places its `.rsx`: {out}"
    );
    assert!(
        out.contains("pub mod loose;"),
        "a directory with no `mod.rs` still has to be created to hold a `.rsx`: {out}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A `.rsx` beside a hand-written `mod.rs` can only be placed by that file, because an outside module cannot add items to one. Saying so beats the "cannot find `drawer_panel` in `drawer`" a reader gets about a file that is plainly there.
#[test]
fn a_module_that_must_place_its_own_rsx_is_told_to() {
    let root = std::env::temp_dir().join(format!("rsx_owns_modules_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("drawer")).unwrap();
    std::fs::write(root.join("drawer/mod.rs"), "// no macro here\n").unwrap();
    std::fs::write(root.join("drawer/panel.rsx"), "[view]\ncol\n").unwrap();

    let modtree = root.join("__modules");
    std::fs::create_dir_all(&modtree).unwrap();
    let generated = root.join("build");
    let (out, _) = discover_rust_modules(&root, &root, &modtree, &generated, true).unwrap();
    assert!(out.contains("compile_error!"), "{out}");
    assert!(out.contains("telar::rsx_modules!();"), "{out}");

    std::fs::write(root.join("drawer/mod.rs"), "telar::rsx_modules!();\n").unwrap();
    let (quiet, _) = discover_rust_modules(&root, &root, &modtree, &generated, true).unwrap();
    assert!(!quiet.contains("compile_error!"), "{quiet}");
    let _ = std::fs::remove_dir_all(&root);
}

/// A nested `rsx_modules!()` places its own directory. Rooting the walk at `src/` instead would declare an ancestor of the file doing the declaring, which rustc reads as a circular module.
#[test]
fn a_nested_invocation_places_its_own_directory() {
    let root = std::env::temp_dir().join(format!("rsx_nested_modules_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("app/editor")).unwrap();
    std::fs::write(root.join("app/mod.rs"), "").unwrap();
    std::fs::write(root.join("app/editor/mod.rs"), "").unwrap();
    std::fs::write(root.join("app/editor/act.rs"), "").unwrap();
    std::fs::write(root.join("app/editor/top_bar.rsx"), "[view]\ncol\n").unwrap();

    let modtree = root.join("__modules");
    std::fs::create_dir_all(&modtree).unwrap();
    let generated = root.join("build");
    let (out, _) =
        discover_rust_modules(&root, &root.join("app/editor"), &modtree, &generated, false)
            .unwrap();

    assert!(
        !out.contains("pub mod app;"),
        "no ancestor is declared: {out}"
    );
    assert!(out.contains("pub mod top_bar;"), "{out}");
    let mirrored = generated.join("app").join("editor").join("top_bar.rs");
    assert!(
        out.contains(&format!("{:?}", mirrored.to_string_lossy())),
        "the generated path still mirrors the whole src tree: {out}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn discover_rust_modules_mirrors_tree() {
    let root = std::env::temp_dir().join(format!("rsx_discover_modules_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for d in [
        "core",
        "shared/components",
        "features/assets",
        "hand/nested",
    ] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    for (p, body) in [
        ("lib.rs", ""),                     // crate root: skipped
        ("main.rs", ""),                    // crate root: skipped
        ("util.rs", ""),                    // top-level module
        ("core/app.rs", ""),                // nested module
        ("core/app_test.rs", ""),           // `core/app.rs` declares it: not a sibling
        ("util_test.rs", ""),               // same, at the top level
        ("core/theme.rs", ""),              // nested module
        ("core/sidebar.rsx", ""),           // markup: a module where it sits
        ("shared/demo.rs", ""),             // nested module
        ("shared/components/card.rsx", ""), // markup-only subdir: still a module
        ("features/home.rsx", ""),          // markup-only tree: still a module
        ("features/assets/x.png", ""),      // asset: pruned
        ("hand/mod.rs", ""),                // hand-managed: declared, not descended
        ("hand/nested/inner.rs", ""),
    ] {
        std::fs::write(root.join(p), body).unwrap();
    }

    let modtree = root.join("__modules");
    std::fs::create_dir_all(&modtree).unwrap();
    let generated = root.join("__generated");
    let (out, written) = discover_rust_modules(&root, &root, &modtree, &generated, true).unwrap();
    let core_rs = std::fs::read_to_string(modtree.join("core.rs")).unwrap_or_default();
    let shared_rs = std::fs::read_to_string(modtree.join("shared.rs")).unwrap_or_default();
    let features_rs = std::fs::read_to_string(modtree.join("features.rs")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(
        written,
        vec![
            modtree.join("core.rs"),
            modtree.join("features.rs"),
            modtree.join("shared__components.rs"),
            modtree.join("shared.rs")
        ],
        "{written:?}"
    );
    assert!(
        features_rs.contains("pub mod home;"),
        "the markup is the module:\n{features_rs}"
    );

    assert!(!out.contains('{'), "no inline module blocks:\n{out}");
    assert!(out.contains("pub mod util;"), "{out}");
    assert!(out.contains("pub mod core;"), "{out}");
    assert!(core_rs.contains("pub mod app;"), "{core_rs}");
    assert!(core_rs.contains("pub mod theme;"), "{core_rs}");
    assert!(out.contains("pub mod shared;"), "{out}");
    assert!(shared_rs.contains("pub mod demo;"), "{shared_rs}");
    assert!(out.contains("pub mod hand;"), "{out}");
    assert!(
        !out.contains("nested") && !out.contains("inner") && !core_rs.contains("nested"),
        "{out}"
    );
    assert!(
        core_rs.contains("pub mod sidebar;"),
        "{out}\n---\n{core_rs}"
    );
    assert!(!features_rs.contains("assets"), "{features_rs}");
    assert!(
        !out.contains("pub mod lib") && !out.contains("pub mod main"),
        "{out}"
    );
    // Declared here, a `*_test.rs` would be a sibling of the module whose private items it tests rather than its child, and every one of them would read as inaccessible.
    assert!(
        !out.contains("util_test") && !core_rs.contains("app_test"),
        "a `*_test.rs` belongs to the module beside it:\n{out}\n---\n{core_rs}"
    );
}
