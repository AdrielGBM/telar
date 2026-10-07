use std::path::PathBuf;

use telar_project::BuildFlavour;

use super::transpile_member;

fn package(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cargo_telar_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    root
}

/// The reason the macro cannot produce on its own: left behind with the source's location, for every flavour, so whichever one a build reads can say it.
#[test]
fn a_refused_source_is_recorded_for_the_macro_to_report() {
    let root = package("refused");
    let broken = root.join("src/broken.rsx");
    std::fs::write(&broken, "[view]\ncol\n    text \"oops\n").unwrap();

    assert!(!transpile_member(&root, "test", "0.0.0"));

    for flavour in BuildFlavour::ALL {
        let failure = telar_project::read_build_failure(&root, flavour)
            .unwrap_or_else(|| panic!("{flavour:?} left no failure"));
        assert_eq!(failure.source, "broken.rsx");
        assert!(
            failure
                .message
                .starts_with(&format!("{}:3:", broken.display())),
            "{}",
            failure.message
        );
        assert!(
            failure.message.contains("unterminated"),
            "{}",
            failure.message
        );
        assert!(telar_project::read_build_index(&root, flavour).is_none());
    }

    let _ = std::fs::remove_dir_all(&root);
}

/// A fixed source must stop being blamed: the record is cleared by the run that succeeds.
#[test]
fn a_run_that_succeeds_clears_the_record() {
    let root = package("cleared");
    let source = root.join("src/home.rsx");
    std::fs::write(&source, "[view]\ncol\n    text \"oops\n").unwrap();
    assert!(!transpile_member(&root, "test", "0.0.0"));

    std::fs::write(&source, "[view]\ntext \"fine\"\n").unwrap();
    assert!(transpile_member(&root, "test", "0.0.0"));

    for flavour in BuildFlavour::ALL {
        assert!(telar_project::read_build_failure(&root, flavour).is_none());
        assert!(telar_project::read_build_index(&root, flavour).is_some());
    }

    let _ = std::fs::remove_dir_all(&root);
}

/// What makes an edited prelude reach the build: every run re-reads `telar.toml`, rewrites the output with the new globs, and records the prelude the macro will compare against the key.
#[test]
fn a_changed_prelude_is_transpiled_into_every_flavour() {
    let root = package("prelude");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"a\"\n").unwrap();
    let manifest = root.join(telar_project::MANIFEST_FILENAME);

    assert!(transpile_member(&root, "test", "0.0.0"));
    for flavour in BuildFlavour::ALL {
        assert!(
            telar_project::read_build_index(&root, flavour)
                .unwrap()
                .prelude
                .is_empty()
        );
    }

    std::fs::write(&manifest, "[telar]\nprelude = [\"telar-components\"]\n").unwrap();
    assert!(transpile_member(&root, "test", "0.0.0"));
    let declared = telar_project::resolve_prelude(&root).unwrap();
    for flavour in BuildFlavour::ALL {
        let index = telar_project::read_build_index(&root, flavour).unwrap();
        assert_eq!(index.prelude, ["telar_components"], "{flavour:?}");
        assert!(index.was_transpiled_with(&declared));
        let output =
            std::fs::read_to_string(telar_project::generated_dir(&root, flavour).join("home.rs"))
                .unwrap();
        assert!(
            output.contains("#[allow(unused_imports)] use telar_components::*;\n"),
            "{output}"
        );
        assert!(index.answers_for(
            &root.join("src"),
            &telar_project::generated_dir(&root, flavour),
            None,
            &declared,
            "0.0.0"
        ));
    }

    let _ = std::fs::remove_dir_all(&root);
}

/// A prelude that cannot be read is the run's failure, said once, rather than a transpile against no prelude that fails on every tag the missing crates provide.
#[test]
fn an_unreadable_prelude_fails_the_run() {
    let root = package("bad_prelude");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"a\"\n").unwrap();
    std::fs::write(
        root.join(telar_project::MANIFEST_FILENAME),
        "[telar]\nprelude = [\"not a path\"]\n",
    )
    .unwrap();

    assert!(!transpile_member(&root, "test", "0.0.0"));
    for flavour in BuildFlavour::ALL {
        assert!(telar_project::read_build_index(&root, flavour).is_none());
    }

    let _ = std::fs::remove_dir_all(&root);
}

/// A library is compiled as a dependency from its Plain output whatever the application's build asked for, and selected from the flavour it asked for, so every flavour is written for it — each with the module tree that wires it.
#[test]
fn a_library_member_gets_every_flavour_and_its_module_tree() {
    let root = package("library");
    std::fs::write(
        root.join(telar_project::MANIFEST_FILENAME),
        "[telar]\nlibrary = true\n",
    )
    .unwrap();
    std::fs::write(root.join("src/lib.rs"), "telar::rsx_modules!();\n").unwrap();
    std::fs::write(root.join("src/card.rsx"), "[view]\ntext \"a\"\n").unwrap();

    assert!(transpile_member(&root, "test", "0.0.0"));
    for flavour in BuildFlavour::ALL {
        assert!(
            telar_project::read_build_index(&root, flavour).is_some(),
            "{flavour:?}"
        );
        let tree = telar_project::ModuleTree::discover(
            &root.join("src"),
            &telar_project::generated_dir(&root, flavour),
            flavour.site_file_name(),
        );
        assert_eq!(tree.first_difference(), None, "{flavour:?}");
    }
    let site = std::fs::read_to_string(root.join("src/.telar/modules.rs")).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(
        site,
        "#[path = \"../../.telar/build/card.rs\"] pub mod card;\n"
    );
}

/// The macro used to sweep and now writes nothing, so a renamed `.rsx` leaves its old output for this to remove — without touching the module tree written beside it.
#[test]
fn output_of_a_removed_source_is_swept_and_the_tree_kept() {
    let root = package("sweep");
    std::fs::write(root.join("src/lib.rs"), "telar::rsx_modules!();\n").unwrap();
    std::fs::create_dir_all(root.join("src/core")).unwrap();
    std::fs::write(root.join("src/core/old.rsx"), "[view]\ntext \"a\"\n").unwrap();
    assert!(transpile_member(&root, "test", "0.0.0"));
    let generated = telar_project::generated_dir(&root, BuildFlavour::Plain);
    assert!(generated.join("core/old.rs").is_file());

    std::fs::rename(root.join("src/core/old.rsx"), root.join("src/core/new.rsx")).unwrap();
    assert!(transpile_member(&root, "test", "0.0.0"));
    let old = generated.join("core/old.rs").exists();
    let new = generated.join("core/new.rs").is_file();
    let tree = generated
        .join(telar_project::MODULE_TREE_DIR)
        .join("core.rs")
        .is_file();
    let _ = std::fs::remove_dir_all(&root);
    assert!(!old && new && tree, "old {old}, new {new}, tree {tree}");
}

/// A crate that wires only hand-written modules and a catalog still invokes the macro, so it still needs a tree; with no markup it gets no build index.
#[test]
fn a_member_without_markup_that_invokes_the_macro_gets_a_tree() {
    let root = package("no_markup");
    std::fs::write(root.join("src/lib.rs"), "telar::rsx_modules!();\n").unwrap();
    std::fs::write(root.join("src/util.rs"), "").unwrap();
    assert!(transpile_member(&root, "test", "0.0.0"));
    let site = std::fs::read_to_string(root.join("src/.telar/modules.rs")).ok();
    let index = telar_project::read_build_index(&root, BuildFlavour::Plain);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(
        site.as_deref(),
        Some("#[path = \"../util.rs\"] pub mod util;\n")
    );
    assert!(index.is_none());
}

#[test]
fn a_member_that_never_invokes_the_macro_is_left_alone() {
    let root = package("untouched");
    std::fs::write(root.join("src/lib.rs"), "pub mod util;\n").unwrap();
    std::fs::write(root.join("src/util.rs"), "").unwrap();
    assert!(transpile_member(&root, "test", "0.0.0"));
    let site = root.join("src/.telar").exists();
    let telar = root.join(".telar").exists();
    let _ = std::fs::remove_dir_all(&root);
    assert!(!site && !telar);
}
