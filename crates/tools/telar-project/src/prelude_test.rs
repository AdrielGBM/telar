use std::path::PathBuf;

use super::*;

/// A crate is named in `telar.toml` the way `Cargo.toml` names its package, and in generated code the way Rust names the crate.
#[test]
fn a_package_name_is_normalised_to_its_crate_name() {
    let entry = PreludeEntry::parse("telar-components").unwrap();
    assert_eq!(entry.path(), "telar_components");
    assert_eq!(entry.to_string(), "telar_components");
}

#[test]
fn a_path_inside_a_crate_is_kept_as_written() {
    assert_eq!(
        PreludeEntry::parse("telar_components::prelude")
            .unwrap()
            .path(),
        "telar_components::prelude"
    );
    assert_eq!(
        PreludeEntry::parse("  my-plugin::widgets::prelude ")
            .unwrap()
            .path(),
        "my_plugin::widgets::prelude"
    );
}

/// Only the crate segment is a package name; a hyphen anywhere after it is a path that does not parse.
#[test]
fn a_hyphen_past_the_crate_segment_is_refused() {
    let error = PreludeEntry::parse("telar-components::pre-lude").unwrap_err();
    assert!(error.contains("pre-lude"), "{error}");
}

/// Each of these would be emitted as a `use` that does not parse, so the error belongs on the key rather than on every generated file.
#[test]
fn an_entry_that_is_not_a_rust_path_is_refused_naming_the_entry() {
    for declared in [
        "",
        "  ",
        "::telar_components",
        "telar_components::",
        "telar_components::*",
        "telar components",
        "1st_party",
        "_",
        "telar_components::{a, b}",
        "crate::ui",
        "self",
        "super::widgets",
        "plugin::type",
    ] {
        let error = PreludeEntry::parse(declared).expect_err(&format!(
            "{declared:?} is not a path generated code can glob"
        ));
        assert!(error.contains("[telar] prelude"), "{error}");
        if !declared.trim().is_empty() {
            assert!(error.contains(declared), "{error}");
        }
    }
}

#[test]
fn an_entry_listed_twice_is_a_problem_however_it_is_spelled() {
    let entries = [
        PreludeEntry::parse("telar-components").unwrap(),
        PreludeEntry::parse("other").unwrap(),
        PreludeEntry::parse("telar_components").unwrap(),
    ];
    let problems = problems(&entries);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("telar_components"), "{problems:?}");
    assert!(super::problems(&entries[..2]).is_empty());
}

fn package_with(name: &str, cargo: &str, telar: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_prelude_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Cargo.toml"), cargo).unwrap();
    std::fs::write(root.join(crate::MANIFEST_FILENAME), telar).unwrap();
    root
}

/// The error belongs on the line the author wrote, with the entry underlined, not on the generated `use` every `.rsx` would otherwise fail on.
#[test]
fn each_entry_knows_the_line_and_columns_that_declare_it() {
    let root = package_with(
        "spans",
        "[package]\nname = \"app\"\n",
        "[telar]\ntheme = \"crate::Theme\"\nprelude = [\n    \"telar-components\",\n    \"my_plugin::prelude\",\n]\n",
    );
    let declarations = prelude_declarations(&root).unwrap();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(declarations.len(), 2);
    assert_eq!(declarations[0].entry.path(), "telar_components");
    assert_eq!(declarations[0].line, 4);
    assert_eq!(declarations[0].columns, (4, 22));
    assert_eq!(declarations[1].line, 5);
    assert_eq!(declarations[1].root_crate(), "my_plugin");
}

#[test]
fn an_entry_whose_crate_is_not_a_dependency_is_a_problem_on_its_line() {
    let root = package_with(
        "undeclared",
        "[package]\nname = \"app\"\n\n[dependencies]\ntelar = \"1\"\nmy-plugin = { path = \"../p\" }\n",
        "[telar]\nprelude = [\"telar-components\", \"my_plugin::prelude\", \"telar::extra\"]\n",
    );
    let problems = prelude_problems(&root).unwrap();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(problems.len(), 1, "{problems:?}");
    let problem = &problems[0];
    assert_eq!(problem.declaration.entry.path(), "telar_components");
    assert_eq!(problem.declaration.line, 2);
    assert!(
        problem
            .message
            .contains("`telar_components`, which is not a dependency of `app`"),
        "{}",
        problem.message
    );
    assert!(
        problem.help.contains("cargo add -p app telar-components"),
        "{}",
        problem.help
    );
}

/// A renamed dependency is named by its key; a target-specific one is still a normal dependency; `std` needs no declaring at all.
#[test]
fn a_renamed_or_target_specific_dependency_counts() {
    let root = package_with(
        "renamed",
        "[package]\nname = \"app\"\n\n[dependencies]\nwidgets = { package = \"telar-components\", version = \"1\" }\n\n[target.'cfg(unix)'.dependencies]\nunix-bits = \"1\"\n",
        "[telar]\nprelude = [\"widgets\", \"unix_bits::prelude\", \"std::collections\"]\n",
    );
    let problems = prelude_problems(&root).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert!(problems.is_empty(), "{problems:?}");
}

/// A crate under `[dev-dependencies]` is in the manifest and still out of reach of the library the `.rsx` compiles into, so the advice is to move it rather than to add it.
#[test]
fn a_dev_dependency_is_named_as_the_wrong_table() {
    let root = package_with(
        "dev_only",
        "[package]\nname = \"app\"\n\n[dev-dependencies]\ntelar-components = \"1\"\n",
        "[telar]\nprelude = [\"telar_components\"]\n",
    );
    let problems = prelude_problems(&root).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(problems.len(), 1);
    assert!(
        problems[0].help.contains("[dev-dependencies]"),
        "{}",
        problems[0].help
    );
}

/// An inherited prelude is reported against the workspace's `telar.toml`, the only file that names it.
#[test]
fn an_inherited_entry_points_at_the_workspace_manifest() {
    let root = std::env::temp_dir().join(format!("telar_prelude_inherited_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let member = root.join("apps/site");
    std::fs::create_dir_all(&member).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"apps/site\"]\n",
    )
    .unwrap();
    std::fs::write(
        root.join(crate::MANIFEST_FILENAME),
        "[telar]\nprelude = [\"telar-components\"]\n",
    )
    .unwrap();
    std::fs::write(member.join("Cargo.toml"), "[package]\nname = \"site\"\n").unwrap();
    std::fs::write(
        member.join(crate::MANIFEST_FILENAME),
        "[telar]\nbackend = \"auto\"\n",
    )
    .unwrap();

    let problems = prelude_problems(&member).unwrap();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(problems.len(), 1);
    assert_eq!(
        problems[0].declaration.file,
        root.join(crate::MANIFEST_FILENAME)
    );
    assert!(
        problems[0].message.contains("`site`"),
        "{}",
        problems[0].message
    );
}
