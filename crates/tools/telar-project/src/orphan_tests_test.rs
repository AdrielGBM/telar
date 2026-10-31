use super::*;

fn package(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("telar_orphan_tests_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, content) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    root
}

#[test]
fn a_test_file_a_sibling_module_includes_through_path_is_wired() {
    let root = package(
        "path",
        &[
            ("Cargo.toml", "[package]\nname = \"p\"\n"),
            ("src/lib.rs", "mod badge;\n"),
            (
                "src/badge.rs",
                "#[cfg(test)]\n#[path = \"badge_test.rs\"]\nmod tests;\n",
            ),
            ("src/badge_test.rs", ""),
            (
                "src/nested/mod.rs",
                "#[cfg(all(test, unix))] #[path=\"../card_test.rs\"] mod card_tests;\n",
            ),
            ("src/card_test.rs", ""),
            ("src/test_support.rs", ""),
        ],
    );
    let warnings = orphan_test_warnings(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_test_file_a_cargo_target_names_is_wired() {
    let root = package(
        "target",
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"p\"\n\n[[test]]\nname = \"rsx\"\npath = \"src/rsx_test.rs\"\n",
            ),
            ("src/lib.rs", ""),
            ("src/rsx_test.rs", ""),
        ],
    );
    let warnings = orphan_test_warnings(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_test_file_nothing_includes_is_a_warning_naming_both_fixes() {
    let root = package(
        "orphan",
        &[
            ("Cargo.toml", "[package]\nname = \"p\"\n"),
            ("src/lib.rs", "mod badge;\n"),
            ("src/badge.rs", "// #[path = \"badge_test.rs\"]\n"),
            ("src/badge_test.rs", ""),
            ("src/sub/chip_test.rs", ""),
        ],
    );
    let warnings = orphan_test_warnings(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings[0].starts_with("src/badge_test.rs is compiled by nothing"),
        "{warnings:?}"
    );
    assert!(
        warnings[0].contains("`#[cfg(test)] #[path = \"badge_test.rs\"] mod tests;` to badge.rs"),
        "{warnings:?}"
    );
    assert!(
        warnings[0].contains("`[[test]]` target with `path = \"src/badge_test.rs\"`"),
        "{warnings:?}"
    );
    assert!(
        warnings[1].starts_with("src/sub/chip_test.rs "),
        "{warnings:?}"
    );
}

#[test]
fn a_test_file_a_plain_mod_or_another_test_file_declares_is_wired() {
    let root = package(
        "plain",
        &[
            ("Cargo.toml", "[package]\nname = \"p\"\n"),
            ("src/lib.rs", "#[cfg(test)]\nmod consistency_test;\n"),
            ("src/consistency_test.rs", ""),
            (
                "src/strings_test.rs",
                "#[path = \"strings_support_test.rs\"]\nmod support;\n",
            ),
            ("src/strings_support_test.rs", ""),
            ("src/lib_extra.rs", "#[path = \"strings_test.rs\"] mod t;\n"),
        ],
    );
    let warnings = orphan_test_warnings(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert!(warnings.is_empty(), "{warnings:?}");
}
