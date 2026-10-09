use super::*;

fn content<'a>(files: &'a [ModuleTreeFile], path: &Path) -> &'a str {
    files
        .iter()
        .find(|file| file.path == path)
        .map(|file| file.content.as_str())
        .unwrap_or_else(|| panic!("no {} among {files:?}", path.display()))
}

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

    std::fs::write(root.join("telar.toml"), "[telar]\nbackend = \"software\"\n").unwrap();
    assert_eq!(assets_root(&root), root.join("assets"));

    let _ = std::fs::remove_dir_all(&root);
}

/// A name the site declares itself is left alone. Declaring it again is `E0428`, and a `mod menu;` written by hand redeclared as `pub mod menu;` would publish a module its author kept private.
#[test]
fn a_name_the_site_declares_itself_is_left_alone() {
    let root = std::env::temp_dir().join(format!("rsx_hand_written_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for d in ["editor", "loose"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    std::fs::write(
        root.join("lib.rs"),
        "mod helper;\npub mod editor;\n#[cfg(test)]\nmod tests { }\n",
    )
    .unwrap();
    std::fs::write(root.join("editor/mod.rs"), "").unwrap();
    std::fs::write(root.join("editor/act.rs"), "").unwrap();
    std::fs::write(root.join("loose/panel.rsx"), "[view]\ncol\n").unwrap();
    std::fs::write(root.join("helper.rs"), "").unwrap();
    std::fs::write(root.join("tests.rs"), "").unwrap();

    let generated = root.join("build");
    let (out, _) = discover_rust_modules(&root, &root, &generated);

    assert!(!out.contains("mod helper;"), "declared by hand: {out}");
    assert!(!out.contains("mod editor;"), "declared by hand: {out}");
    assert!(
        !out.contains("mod tests;"),
        "an inline `mod tests {{ }}` is a declaration too: {out}"
    );
    assert!(
        out.contains("pub mod loose;"),
        "what the site does not declare is still placed: {out}"
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

    let generated = root.join("build");
    let (out, _) = discover_rust_modules(&root, &root, &generated);
    assert!(out.contains("compile_error!"), "{out}");
    assert!(out.contains("telar::rsx_modules!();"), "{out}");

    std::fs::write(root.join("drawer/mod.rs"), "telar::rsx_modules!();\n").unwrap();
    let (quiet, _) = discover_rust_modules(&root, &root, &generated);
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

    let generated = root.join("build");
    let (out, _) = discover_rust_modules(&root, &root.join("app/editor"), &generated);

    assert!(
        !out.contains("pub mod app;"),
        "no ancestor is declared: {out}"
    );
    assert!(out.contains("pub mod top_bar;"), "{out}");
    assert!(
        out.contains("\"../../../build/app/editor/top_bar.rs\""),
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

    let generated = root.join("__generated");
    let modtree = generated.join(MODULE_TREE_DIR);
    let (out, files) = discover_rust_modules(&root, &root, &generated);
    let core_rs = content(&files, &modtree.join("core.rs"));
    let shared_rs = content(&files, &modtree.join("shared.rs"));
    let features_rs = content(&files, &modtree.join("features.rs"));
    let written: Vec<PathBuf> = files.iter().map(|file| file.path.clone()).collect();
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

/// The inverse is the forward mapping read backwards, so a `.rsx` that goes out and comes back has to be itself — for every flavour, which is the half the language server's own copy of this got wrong.
#[test]
fn a_source_survives_the_round_trip_through_every_flavour() {
    let package = Path::new("/proj");
    let src = package.join("src");
    for rsx in [
        src.join("home.rsx"),
        src.join("shared/components/card.rsx"),
        src.join("a/b/c/deep.rsx"),
    ] {
        for flavour in crate::BuildFlavour::ALL {
            let rel =
                relative_output_path(&rsx, &src).expect("a file under src has an output path");
            let generated = crate::generated_dir(package, flavour).join(rel);
            assert!(
                is_generated_output(&generated),
                "{flavour:?} output should be recognised: {}",
                generated.display()
            );
            assert_eq!(
                source_for_generated(&generated).as_deref(),
                Some(rsx.as_path()),
                "{flavour:?} should map back to the source it came from"
            );
        }
    }
}

/// Anything else is somebody's own file, and mistaking one for generated output would reverse-map it onto a `.rsx` that does not exist.
#[test]
fn a_hand_written_file_is_not_generated_output() {
    for path in [
        "/proj/src/main.rs",
        "/proj/.telar/assets.rs",
        "/proj/.telar/build",
        "/proj/build/thing.rs",
        "/proj/.telar/other/x.rs",
        "/proj/src/home.rsx",
    ] {
        assert!(
            !is_generated_output(Path::new(path)),
            "{path} is not generated output"
        );
        assert_eq!(source_for_generated(Path::new(path)), None, "{path}");
    }
}

/// The cycle this scheme exists to make impossible: a site's declarations must never name the file that includes them. The macro cannot see which module it was expanded in, so a site that guessed the crate root wrote `pub mod media;` into `media/mod.rs` itself, and rust-analyzer walked that until it exhausted the machine's memory.
#[test]
fn no_site_declares_the_file_that_includes_it() {
    let root = std::env::temp_dir().join(format!("rsx_sites_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("media")).unwrap();
    std::fs::write(root.join("lib.rs"), "telar::rsx_modules!();\n").unwrap();
    std::fs::write(root.join("media/mod.rs"), "telar::rsx_modules!();\n").unwrap();
    std::fs::write(root.join("media/player.rsx"), "[view]\ncol\n").unwrap();

    let generated = root.join("build");
    let written = ModuleTree::discover(&root, &generated, "modules.rs")
        .write()
        .unwrap();

    let sites = placement_sites(&root);
    assert_eq!(sites, vec![root.clone(), root.join("media")]);
    for site in &sites {
        let file = site.join(SITE_DIR).join("modules.rs");
        assert!(
            written.contains(&file),
            "every site is written: {written:?}"
        );
        let declarations = std::fs::read_to_string(&file).unwrap();
        assert!(
            !declarations.contains("\"modules.rs\"") && !declarations.contains("\"mod.rs\""),
            "a site declaring its own file is the circular module: {declarations}"
        );
    }
    let nested =
        std::fs::read_to_string(root.join("media").join(SITE_DIR).join("modules.rs")).unwrap();
    assert!(nested.contains("pub mod player;"), "{nested}");
    assert!(!nested.contains("pub mod media;"), "{nested}");
    let _ = std::fs::remove_dir_all(&root);
}

/// An `include!` resolves against the file holding the call, and a module declares its children under the directory named after that file. Only a crate root and a `mod.rs` make those the same directory, so anywhere else the invocation would pull in its parent's declarations.
#[test]
fn an_invocation_that_cannot_place_is_named() {
    let root = std::env::temp_dir().join(format!("rsx_strays_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("media")).unwrap();
    std::fs::write(root.join("lib.rs"), "telar::rsx_modules!();\n").unwrap();
    std::fs::write(root.join("media/mod.rs"), "telar::rsx_modules!();\n").unwrap();
    assert!(stray_placement_files(&root).is_empty());

    std::fs::write(root.join("media/player.rs"), "telar::rsx_modules!();\n").unwrap();
    assert_eq!(
        stray_placement_files(&root),
        vec![root.join("media/player.rs")]
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Naming the macro in prose is not invoking it. telar's own sandbox has two files whose comments mention `app!`, and reading them as placement sites failed the build of a crate that was correct.
#[test]
fn a_comment_naming_the_macro_is_not_an_invocation() {
    let root = std::env::temp_dir().join(format!("rsx_prose_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("core")).unwrap();
    std::fs::write(root.join("lib.rs"), "telar::app!(Theme);\n").unwrap();
    std::fs::write(
        root.join("core/theme.rs"),
        "/// Called from the `app!` setup closure (and any test that switches themes).\nfn register() {}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("core/mod.rs"),
        "// placed by `rsx_modules!` upstairs\n",
    )
    .unwrap();

    assert!(stray_placement_files(&root).is_empty());
    assert_eq!(placement_sites(&root), vec![root.clone()]);
    let _ = std::fs::remove_dir_all(&root);
}

/// A `mod.rsx` makes the directory telar's: the parent declares it at the transpiled module, and the children the module cannot know about reach it through the file its `include!` names.
#[test]
fn a_mod_rsx_owns_its_directory() {
    let root = std::env::temp_dir().join(format!("rsx_module_root_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("media")).unwrap();
    std::fs::write(root.join("lib.rs"), "telar::rsx_modules!();\n").unwrap();
    std::fs::write(root.join("media/mod.rsx"), "[logic]\npub fn helper() {}\n").unwrap();
    std::fs::write(root.join("media/panel.rsx"), "[view]\ncol\n").unwrap();
    std::fs::write(root.join("media/state.rs"), "").unwrap();

    let generated = root.join("build");
    let (out, files) = discover_rust_modules(&root, &root, &generated);

    assert!(
        out.contains("#[path = \"../build/media/mod.rs\"] pub mod media;"),
        "the parent declares the directory at its transpiled module: {out}"
    );
    assert!(
        placement_sites(&root) == vec![root.clone()],
        "a directory telar owns is not a site that has to invoke anything"
    );
    let children = content(
        &files,
        &generated.join("media").join(MODULE_CHILDREN_FILENAME),
    );
    assert!(children.contains("pub mod panel;"), "{children}");
    assert!(children.contains("pub mod state;"), "{children}");
    assert!(!children.contains("mod mod;"), "{children}");
    assert!(
        files
            .iter()
            .any(|file| file.path.ends_with(MODULE_CHILDREN_FILENAME)),
        "written, so the stale sweep knows it is live"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Combining keeps the Rust where its author put it. What cannot come along is an inner attribute: rustc refuses one in an included file, so it is named here rather than reported against generated code.
#[test]
fn a_mod_rs_beside_a_mod_rsx_is_included_unless_it_opens_with_an_inner_attribute() {
    let root = std::env::temp_dir().join(format!("rsx_module_combine_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("media")).unwrap();
    std::fs::write(root.join("media/mod.rsx"), "[logic]\n").unwrap();
    std::fs::write(root.join("media/mod.rs"), "pub fn helper() {}\n").unwrap();

    let generated = root.join("build");
    let children_file = generated.join("media").join(MODULE_CHILDREN_FILENAME);

    let (_, files) = discover_rust_modules(&root, &root, &generated);
    let combined = content(&files, &children_file);
    assert!(combined.contains("include!("), "{combined}");
    assert!(
        combined.contains("include!(\"../../media/mod.rs\");"),
        "relative to the file holding it: {combined}"
    );

    std::fs::write(
        root.join("media/mod.rs"),
        "//! The bar.\npub fn helper() {}\n",
    )
    .unwrap();
    let (_, files) = discover_rust_modules(&root, &root, &generated);
    let refused = content(&files, &children_file);
    assert!(refused.contains("compile_error!"), "{refused}");
    assert!(!refused.contains("include!("), "{refused}");
    let _ = std::fs::remove_dir_all(&root);
}

/// `src/bin/` belongs to cargo, and only there: every file in it is a crate root of its own, so declaring it as a module would compile each binary's `fn main` into the library. A `bin/` further down is an ordinary directory.
#[test]
fn the_crate_roots_bin_directory_is_left_to_cargo() {
    let root = std::env::temp_dir().join(format!("rsx_bin_dir_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::create_dir_all(root.join("app/bin")).unwrap();
    std::fs::write(root.join("lib.rs"), "telar::rsx_modules!();\n").unwrap();
    std::fs::write(root.join("bin/extra.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("app/bin/helper.rs"), "").unwrap();

    let generated = root.join("build");
    let (out, files) = discover_rust_modules(&root, &root, &generated);
    assert!(!out.contains("pub mod bin;"), "{out}");

    let nested = content(&files, &generated.join(MODULE_TREE_DIR).join("app.rs"));
    assert!(nested.contains("pub mod bin;"), "{nested}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_relative_path_climbs_to_the_common_ancestor() {
    let package = Path::new("/pkg");
    for (from, to, expected) in [
        ("/pkg/src/.telar", "/pkg/src/util.rs", "../util.rs"),
        (
            "/pkg/src/.telar",
            "/pkg/.telar/build/home.rs",
            "../../.telar/build/home.rs",
        ),
        (
            "/pkg/.telar/build/__modules",
            "/pkg/src/core/theme.rs",
            "../../../src/core/theme.rs",
        ),
        (
            "/pkg/.telar/build/media",
            "/pkg/.telar/build/media/panel.rs",
            "panel.rs",
        ),
        (
            "/pkg/.telar/build/__modules",
            "/pkg/.telar/build/core/sidebar.rs",
            "../core/sidebar.rs",
        ),
    ] {
        assert_eq!(
            relative_path(&package.join(from), &package.join(to)),
            expected,
            "{from} -> {to}"
        );
    }
    assert_eq!(
        relative_path(Path::new("a/b"), Path::new("a/c.rs")),
        "../c.rs"
    );
    assert_eq!(
        relative_path(Path::new("a"), Path::new("b/c.rs")),
        "../b/c.rs"
    );
}

/// The macro writes nothing, so a tree that no longer matches the sources is something it can only notice. Adding a module is the common way to get there.
#[test]
fn a_tree_written_before_a_module_was_added_differs() {
    let root = std::env::temp_dir().join(format!("rsx_tree_difference_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let src = root.join("src");
    std::fs::create_dir_all(src.join("core")).unwrap();
    std::fs::write(src.join("lib.rs"), "telar::rsx_modules!();\n").unwrap();
    std::fs::write(src.join("core/theme.rs"), "").unwrap();
    let generated = root.join(".telar/build");

    let tree = ModuleTree::discover(&src, &generated, "modules.rs");
    assert!(tree.first_difference().is_some(), "nothing written yet");
    tree.write().unwrap();
    assert_eq!(tree.first_difference(), None);

    std::fs::write(src.join("core/menu.rs"), "").unwrap();
    let grown = ModuleTree::discover(&src, &generated, "modules.rs");
    let stale = grown.first_difference().map(Path::to_path_buf);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(stale, Some(generated.join(MODULE_TREE_DIR).join("core.rs")));
}

#[test]
fn only_a_package_that_invokes_the_macro_is_placed() {
    let root = std::env::temp_dir().join(format!("rsx_invokes_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("media")).unwrap();
    std::fs::write(root.join("lib.rs"), "pub mod media;\n").unwrap();
    std::fs::write(root.join("media/mod.rs"), "").unwrap();
    let none = invokes_placement_macro(&root);
    std::fs::write(root.join("media/mod.rs"), "telar::rsx_modules!();\n").unwrap();
    let nested = invokes_placement_macro(&root);
    std::fs::write(root.join("media/mod.rs"), "").unwrap();
    std::fs::write(root.join("main.rs"), "telar::app!(Theme);\n").unwrap();
    let root_site = invokes_placement_macro(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert!(!none);
    assert!(nested);
    assert!(root_site);
}

/// What a published library ships is the tree `cargo telar transpile` wrote in another directory. Compiled from a read-only copy somewhere else, every `#[path]` and `include!` in it has to resolve against the file holding it — through an `include!`d site file, a module-tree file, a nested site and a `mod.rsx`'s children — and the tree has to read as current where it landed.
#[test]
fn a_tree_compiles_from_a_read_only_copy_in_another_directory() {
    let base = std::env::temp_dir().join(format!("rsx_relocated_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let original = base.join("written");
    let src = original.join("src");
    let generated = original.join(".telar/build");
    let leaf = "pub fn f() {}\n";
    for (path, body) in [
        (
            "src/lib.rs",
            "macro_rules! rsx_modules { () => { include!(\".telar/modules.rs\"); } }\nrsx_modules!();\npub fn reach() { util::f(); home::f(); core::theme::f(); core::sidebar::f(); media::helper(); media::panel::f(); media::state::f(); app::inner::f(); app::view::f(); }\n",
        ),
        ("src/util.rs", leaf),
        ("src/home.rsx", ""),
        ("src/core/theme.rs", leaf),
        ("src/core/sidebar.rsx", ""),
        ("src/media/mod.rsx", ""),
        ("src/media/mod.rs", "pub fn helper() {}\n"),
        ("src/media/panel.rsx", ""),
        ("src/media/state.rs", leaf),
        ("src/app/mod.rs", "rsx_modules!();\n"),
        ("src/app/inner.rs", leaf),
        ("src/app/view.rsx", ""),
        (".telar/build/home.rs", leaf),
        (".telar/build/core/sidebar.rs", leaf),
        (
            ".telar/build/media/mod.rs",
            "include!(\"__children.rs\");\n",
        ),
        (".telar/build/media/panel.rs", leaf),
        (".telar/build/app/view.rs", leaf),
    ] {
        let path = original.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    ModuleTree::discover(&src, &generated, "modules.rs")
        .write()
        .unwrap();

    let moved = base.join("elsewhere/unpacked");
    copy_tree(&original, &moved);
    std::fs::remove_dir_all(&original).unwrap();
    set_read_only(&moved, true);

    let still_current = ModuleTree::discover(
        &moved.join("src"),
        &moved.join(".telar/build"),
        "modules.rs",
    )
    .first_difference()
    .map(Path::to_path_buf);
    let out_dir = base.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    let compiled = std::process::Command::new(std::env::var("RUSTC").unwrap_or("rustc".into()))
        .args([
            "--edition",
            "2021",
            "--crate-type",
            "lib",
            "--emit",
            "metadata",
        ])
        .args(["--crate-name", "relocated", "--out-dir"])
        .arg(&out_dir)
        .arg(moved.join("src/lib.rs"))
        .output()
        .expect("rustc runs");

    set_read_only(&moved, false);
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(
        still_current, None,
        "the tree reads the same wherever it is"
    );
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
}

/// A deleted `.rsx` leaves neither its `.rs` nor `.rs.map` behind after pruning.
#[test]
fn a_deleted_rsx_leaves_no_rs_or_rs_map() {
    let root = std::env::temp_dir().join(format!("rsx_prune_deleted_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let rs_file = root.join("component.rs");
    let map_file = root.join("component.rs.map");

    std::fs::write(&rs_file, "// generated").unwrap();
    std::fs::write(&map_file, "{}").unwrap();

    assert!(rs_file.exists());
    assert!(map_file.exists());

    let written = std::collections::HashSet::new();
    prune_stale_generated(&root, &written);

    assert!(!rs_file.exists(), "stale .rs should be removed");
    assert!(!map_file.exists(), "stale .rs.map should be removed");

    let _ = std::fs::remove_dir_all(&root);
}

/// A live `.rs` keeps its `.rs.map` during pruning.
#[test]
fn a_live_rs_keeps_its_rs_map() {
    let root = std::env::temp_dir().join(format!("rsx_prune_live_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let rs_file = root.join("component.rs");
    let map_file = root.join("component.rs.map");

    std::fs::write(&rs_file, "// generated").unwrap();
    std::fs::write(&map_file, "{}").unwrap();

    let mut written = std::collections::HashSet::new();
    written.insert(rs_file.clone());

    prune_stale_generated(&root, &written);

    assert!(rs_file.exists(), "live .rs should be kept");
    assert!(map_file.exists(), "live .rs.map should be kept");

    let _ = std::fs::remove_dir_all(&root);
}

/// A stray `.rs.map` with no `.rs` is removed during pruning.
#[test]
fn a_stray_rs_map_without_rs_is_removed() {
    let root = std::env::temp_dir().join(format!("rsx_prune_stray_map_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let rs_file = root.join("component.rs");
    let map_file = root.join("component.rs.map");

    std::fs::write(&map_file, "{}").unwrap();
    assert!(!rs_file.exists(), "no .rs file");
    assert!(map_file.exists(), "only .rs.map exists");

    let written = std::collections::HashSet::new();
    prune_stale_generated(&root, &written);

    assert!(
        !map_file.exists(),
        "stray .rs.map with no .rs should be removed"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// Pruning works across nested directories with multiple flavours.
#[test]
fn pruning_removes_orphans_across_flavours() {
    let root = std::env::temp_dir().join(format!("rsx_prune_flavours_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in [
        root.join("build/core"),
        root.join("build-hot/core"),
        root.join("build-preview/core"),
    ] {
        std::fs::create_dir_all(&dir).unwrap();
    }

    let files = vec![
        ("build/core/button.rs", "build/core/button.rs.map", true),
        ("build/core/card.rs", "build/core/card.rs.map", false),
        (
            "build-hot/core/button.rs",
            "build-hot/core/button.rs.map",
            true,
        ),
        (
            "build-hot/core/panel.rs",
            "build-hot/core/panel.rs.map",
            false,
        ),
        (
            "build-preview/core/icon.rs",
            "build-preview/core/icon.rs.map",
            false,
        ),
    ];

    let mut written = std::collections::HashSet::new();
    for (rs, map, is_live) in &files {
        let rs_path = root.join(rs);
        let map_path = root.join(map);
        std::fs::write(&rs_path, "// generated").unwrap();
        std::fs::write(&map_path, "{}").unwrap();
        if *is_live {
            written.insert(rs_path);
        }
    }

    prune_stale_generated(&root, &written);

    for (rs, map, is_live) in &files {
        let rs_path = root.join(rs);
        let map_path = root.join(map);
        if *is_live {
            assert!(rs_path.exists(), "{} should be kept", rs);
            assert!(map_path.exists(), "{} should be kept", map);
        } else {
            assert!(!rs_path.exists(), "{} should be removed", rs);
            assert!(!map_path.exists(), "{} should be removed", map);
        }
    }

    let _ = std::fs::remove_dir_all(&root);
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn set_read_only(dir: &Path, read_only: bool) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        if entry.path().is_dir() {
            set_read_only(&entry.path(), read_only);
        }
        set_permissions(&entry.path(), read_only);
    }
    set_permissions(dir, read_only);
}

fn set_permissions(path: &Path, read_only: bool) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(read_only);
    std::fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn a_previews_file_is_declared_as_a_module_beside_the_component_it_previews() {
    assert!(is_previews_file(Path::new(
        "src/forms/checkbox.previews.rsx"
    )));
    assert!(!is_previews_file(Path::new("src/forms/checkbox.rsx")));
    assert!(!is_previews_file(Path::new("src/.previews.rsx")));
    assert!(!is_previews_file(Path::new("src/previews.rsx")));
    let previews = Path::new("src/forms/checkbox.previews.rsx");
    assert_eq!(component_name(previews), "checkbox");
    assert_eq!(module_name(previews), "checkbox_previews");
    assert_eq!(module_name(Path::new("src/forms/checkbox.rsx")), "checkbox");
    assert_eq!(component_name(Path::new("src/previews.rsx")), "previews");

    let root = std::env::temp_dir().join(format!("rsx_previews_file_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("checkbox.rsx"), "").unwrap();
    std::fs::write(root.join("checkbox.previews.rsx"), "").unwrap();
    let (out, _) = discover_rust_modules(&root, &root, &root.join("__generated"));
    let _ = std::fs::remove_dir_all(&root);

    assert!(out.contains("pub mod checkbox;"), "{out}");
    assert!(
        out.contains(
            "#[path = \"../__generated/checkbox.previews.rs\"] pub mod checkbox_previews;"
        ),
        "{out}"
    );
}
