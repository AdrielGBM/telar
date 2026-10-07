use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use clap::Parser;

use super::super::cli::{Cli, TelarCommand};
use super::super::transpile::transpile_member;
use super::*;

const VERSION: &str = "1.0.0";

fn library(name: &str) -> Library {
    let dir =
        std::env::temp_dir().join(format!("cargo_telar_library_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    write(&dir, "telar.toml", "[telar]\nlibrary = true\n");
    write(&dir, "src/lib.rs", "telar::rsx_modules!();\n");
    write(&dir, "src/badge.rsx", "[view]\ntext \"badge\"\n");
    Library {
        name: name.to_string(),
        dir,
    }
}

fn write(dir: &Path, file: &str, content: &str) {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn transpiled(name: &str) -> Library {
    let library = library(name);
    assert!(transpile_member(&library.dir, "test", VERSION));
    library
}

/// A crate of its own, depending on nothing, so `cargo package --list` answers without a registry. `include` is the manifest's, or none at all.
fn packageable(name: &str, include: Option<&[String]>) -> Library {
    let library = transpiled(name);
    let include = include
        .map(|entries| {
            let quoted: Vec<String> = entries.iter().map(|entry| format!("{entry:?}")).collect();
            format!("include = [{}]\n", quoted.join(", "))
        })
        .unwrap_or_default();
    write(
        &library.dir,
        "Cargo.toml",
        &format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n{include}\n[workspace]\n",
            library.name.replace('_', "-")
        ),
    );
    library
}

fn joined(problems: &[String]) -> String {
    problems.join("\n")
}

#[test]
fn a_freshly_transpiled_library_answers_for_its_sources() {
    let library = transpiled("fresh");
    let problems = artifact_problems(&library.dir, VERSION);
    let _ = std::fs::remove_dir_all(&library.dir);
    assert!(problems.is_empty(), "{}", joined(&problems));
}

#[test]
fn a_source_edited_after_the_transpile_is_reported() {
    let library = transpiled("stale_source");
    write(&library.dir, "src/badge.rsx", "[view]\ntext \"edited\"\n");
    let problems = artifact_problems(&library.dir, VERSION);
    let _ = std::fs::remove_dir_all(&library.dir);
    assert!(
        joined(&problems).contains("no longer answers for `src/`"),
        "{}",
        joined(&problems)
    );
}

#[test]
fn a_module_added_after_the_transpile_is_reported_on_the_module_tree() {
    let library = transpiled("stale_tree");
    write(&library.dir, "src/util.rs", "pub fn helper() {}\n");
    let problems = artifact_problems(&library.dir, VERSION);
    let _ = std::fs::remove_dir_all(&library.dir);
    assert_eq!(problems.len(), 1, "{}", joined(&problems));
    assert!(
        problems[0].contains("src/.telar/modules.rs") && problems[0].contains("module tree"),
        "{}",
        problems[0]
    );
}

#[test]
fn an_artifact_written_for_another_telar_is_reported_with_both_versions() {
    let library = transpiled("version");
    let problems = artifact_problems(&library.dir, "2.0.0");
    let _ = std::fs::remove_dir_all(&library.dir);
    let text = joined(&problems);
    assert!(
        text.contains("transpiled for telar 1.0.0") && text.contains("builds telar 2.0.0"),
        "{text}"
    );
}

#[test]
fn a_library_that_was_never_transpiled_has_no_artifact() {
    let library = library("untranspiled");
    let problems = artifact_problems(&library.dir, VERSION);
    let _ = std::fs::remove_dir_all(&library.dir);
    let text = joined(&problems);
    assert!(text.contains("no transpiled artifact"), "{text}");
    assert!(text.contains("src/.telar/modules.rs"), "{text}");
}

/// What `cargo package` does with no `include`: every dot-directory is left out, so the whole artifact is, and the message gives the block to paste.
#[test]
fn a_manifest_without_include_is_given_the_whole_list() {
    let library = packageable("no_include", None);
    let listed = packaged_files(&library.dir).unwrap();
    let problem = missing_from_package(&library, &listed);
    let _ = std::fs::remove_dir_all(&library.dir);

    assert!(
        !listed.iter().any(|file| file.starts_with(".telar/")),
        "{listed:?}"
    );
    assert!(
        !listed.contains("src/.telar/modules.rs"),
        "a dot-directory inside src/ is left out too: {listed:?}"
    );
    let problem = problem.expect("the artifact is missing from the package");
    assert!(problem.contains(".telar/build.json"), "{problem}");
    assert!(problem.contains("src/.telar/modules.rs"), "{problem}");
    assert!(problem.contains("declares no `include`"), "{problem}");
    assert!(problem.contains(&include_block()), "{problem}");
}

#[test]
fn an_include_missing_one_entry_is_told_that_entry() {
    let partial: Vec<String> = telar_project::library_include()
        .into_iter()
        .filter(|entry| entry != "/.telar/build/**")
        .collect();
    let library = packageable("partial_include", Some(&partial));
    let listed = packaged_files(&library.dir).unwrap();
    let problem = missing_from_package(&library, &listed);
    let _ = std::fs::remove_dir_all(&library.dir);

    let problem = problem.expect("the generated Rust is missing from the package");
    assert!(problem.contains(".telar/build/badge.rs"), "{problem}");
    assert!(problem.contains("Add to `include`"), "{problem}");
    assert!(problem.contains("    \"/.telar/build/**\","), "{problem}");
    assert!(!problem.contains("\"/src/**\""), "{problem}");
}

/// The scaffold's list, end to end: cargo lists every file, and the copy holding only those files still answers.
#[test]
fn the_library_include_ships_an_artifact_that_answers_in_a_copy() {
    let library = packageable("full_include", Some(&telar_project::library_include()));
    let listed = packaged_files(&library.dir).unwrap();
    let problems = readiness(&library, VERSION);
    let _ = std::fs::remove_dir_all(&library.dir);

    for file in [
        "telar.toml",
        "src/lib.rs",
        "src/badge.rsx",
        "src/.telar/modules.rs",
        ".telar/build.json",
        ".telar/build/badge.rs",
        ".telar/build/badge.rs.map",
    ] {
        assert!(listed.contains(file), "{file} is not packaged: {listed:?}");
    }
    assert!(
        !listed
            .iter()
            .any(|file| file.starts_with(".telar/build-hot")
                || file.starts_with("src/.telar/modules-")),
        "only the Plain flavour ships: {listed:?}"
    );
    assert!(problems.is_empty(), "{}", joined(&problems));
}

#[test]
fn a_rename_names_its_destination_once() {
    let status = "R  src/new.rs\0src/old.rs\0 M src/lib.rs\0?? src/extra.rs\0";
    assert_eq!(
        changed_paths(status),
        ["src/new.rs", "src/lib.rs", "src/extra.rs"]
    );
}

fn workspace(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "cargo_telar_libraries_{name}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    write(
        &root,
        "Cargo.toml",
        "[workspace]\nmembers = [\"app\", \"kit\"]\n",
    );
    for member in ["app", "kit"] {
        write(
            &root,
            &format!("{member}/Cargo.toml"),
            &format!("[package]\nname = \"{member}\"\nversion = \"0.1.0\"\n"),
        );
    }
    write(&root, "kit/telar.toml", "[telar]\nlibrary = true\n");
    root
}

#[test]
fn the_workspace_flag_selects_every_library_member_and_nothing_else() {
    let root = workspace("all");
    let selected = select_libraries(&root, &root, None, true);
    let _ = std::fs::remove_dir_all(&root);
    let names: Vec<String> = selected.unwrap().into_iter().map(|l| l.name).collect();
    assert_eq!(names, ["kit"]);
}

#[test]
fn a_named_package_has_to_be_a_library() {
    let root = workspace("named");
    let kit = select_libraries(&root, &root, Some("kit"), false);
    let app = select_libraries(&root, &root, Some("app"), false);
    let unknown = select_libraries(&root, &root, Some("nope"), false);
    let here = select_libraries(&root.join("app"), &root, None, false);
    let virtual_root = select_libraries(&root, &root, None, false);
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(kit.unwrap()[0].name, "kit");
    let app = app.unwrap_err();
    assert!(app.contains("library = true"), "{app}");
    assert!(unknown.unwrap_err().contains("no package named `nope`"));
    assert!(here.unwrap_err().contains("library = true"));
    assert!(virtual_root.unwrap_err().contains("--workspace"));
}

#[test]
fn the_package_command_takes_a_library_a_workspace_or_neither() {
    let parse = |args: &[&str]| {
        Cli::try_parse_from(std::iter::once("cargo-telar").chain(args.iter().copied()))
    };
    let Ok(Cli {
        command: Some(TelarCommand::Package(args)),
    }) = parse(&["package", "--workspace", "--check"])
    else {
        panic!("`package --workspace --check` does not parse");
    };
    assert!(args.workspace && args.check && args.package.is_none());

    let Ok(Cli {
        command: Some(TelarCommand::Package(args)),
    }) = parse(&["package", "-p", "kit", "--", "--no-verify"])
    else {
        panic!("`package -p kit -- --no-verify` does not parse");
    };
    assert_eq!(args.package.as_deref(), Some("kit"));
    assert_eq!(args.cargo_args, ["--no-verify"]);

    assert!(parse(&["package", "-p", "kit", "--workspace"]).is_err());
}

#[test]
fn an_empty_listing_holds_nothing() {
    let listed: BTreeSet<String> = BTreeSet::new();
    let library = library("empty_listing");
    let problem = missing_from_package(&library, &listed);
    let _ = std::fs::remove_dir_all(&library.dir);
    let problem = problem.expect("nothing is packaged");
    assert!(problem.contains("telar.toml"), "{problem}");
}

#[test]
fn the_publish_command_takes_the_package_selection_and_a_dry_run() {
    let parse = |args: &[&str]| {
        Cli::try_parse_from(std::iter::once("cargo-telar").chain(args.iter().copied()))
    };
    let Ok(Cli {
        command: Some(TelarCommand::Publish(args)),
    }) = parse(&["publish", "--workspace", "--dry-run", "--", "--no-verify"])
    else {
        panic!("`publish --workspace --dry-run -- --no-verify` does not parse");
    };
    assert!(args.workspace && args.dry_run && args.package.is_none());
    assert_eq!(args.cargo_args, ["--no-verify"]);

    assert!(parse(&["publish", "-p", "kit", "--workspace"]).is_err());
}

#[test]
fn publish_forwards_its_arguments_and_adds_allow_dirty() {
    let root = Path::new("/ws");
    let libraries = [library("kit_a"), library("kit_b")];
    let forwarded = publish_args(true, vec!["--registry".into(), "mine".into()]);
    let args = cargo_invocation(Shipment::Publish, root, &libraries, &forwarded);
    assert_eq!(args[..2], ["publish", "--allow-dirty"]);
    assert!(args.windows(2).any(|w| w == ["-p", "kit_a"]));
    assert!(args.windows(2).any(|w| w == ["-p", "kit_b"]));
    assert_eq!(args[args.len() - 3..], ["--dry-run", "--registry", "mine"]);
    let _ = std::fs::remove_dir_all(&libraries[0].dir);
    let _ = std::fs::remove_dir_all(&libraries[1].dir);

    let once = publish_args(true, vec!["--dry-run".into()]);
    assert_eq!(once, ["--dry-run"]);
    let package = cargo_invocation(Shipment::Package, root, &[], &["--allow-dirty".into()]);
    assert_eq!(package[0], "package");
    assert_eq!(package.iter().filter(|a| *a == "--allow-dirty").count(), 1);
}

#[test]
fn a_committed_artifact_is_accepted_and_uncommitted_sources_are_not() {
    let listed: BTreeSet<String> = [
        "src/lib.rs",
        "src/view.rsx",
        ".telar/build/index.json",
        "Cargo.toml",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let regenerated_artifact = " M kit/.telar/build/index.json\0";
    assert!(uncommitted_sources(regenerated_artifact, "kit/", &listed).is_empty());

    let status = " M kit/.telar/build/index.json\0 M kit/src/view.rsx\0?? kit/src/lib.rs\0 M other/src/lib.rs\0?? kit/notes.txt\0";
    assert_eq!(
        uncommitted_sources(status, "kit/", &listed),
        ["src/view.rsx", "src/lib.rs"]
    );
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[test]
fn git_sees_a_committed_artifact_as_clean_and_an_edited_source_as_dirty() {
    let root = std::env::temp_dir().join(format!("cargo_telar_commit_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(&root, "src/lib.rs", "pub fn a() {}\n");
    write(&root, ".telar/build/index.json", "{}\n");
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "init"]);
    let listed: BTreeSet<String> = ["src/lib.rs", ".telar/build/index.json"]
        .into_iter()
        .map(str::to_string)
        .collect();

    let clean = uncommitted(&root, &listed);
    write(&root, ".telar/build/index.json", "{\"new\":1}\n");
    let artifact_only = uncommitted(&root, &listed);
    write(&root, "src/lib.rs", "pub fn b() {}\n");
    let edited = uncommitted(&root, &listed);
    let _ = std::fs::remove_dir_all(&root);

    assert!(clean.is_empty() && artifact_only.is_empty());
    assert_eq!(edited, ["src/lib.rs"]);
}

#[test]
fn metadata_names_the_packages_whose_publish_is_false_or_empty() {
    let metadata = r#"{"packages":[
        {"name":"open","publish":null},
        {"name":"closed","publish":[]},
        {"name":"private","publish":["my-registry"]}
    ]}"#;
    let names: Vec<String> = unpublishable_names(metadata).into_iter().collect();
    assert_eq!(names, ["closed"]);
    assert!(unpublishable_names("not json").is_empty());
}

#[test]
fn publishing_the_workspace_skips_what_forbids_it_and_a_named_one_is_refused() {
    let named = |name: &str| Library {
        name: name.to_string(),
        dir: PathBuf::from(name),
    };
    let closed: BTreeSet<String> = ["fixture".to_string()].into();

    let (kept, skipped) =
        publishable(vec![named("kit"), named("fixture")], &closed, false).unwrap();
    assert_eq!(kept, [named("kit")]);
    assert_eq!(skipped, ["fixture"]);

    let error = publishable(vec![named("fixture")], &closed, true).unwrap_err();
    assert!(error.contains("`fixture` cannot be published"), "{error}");
    assert!(publishable(vec![named("kit")], &closed, true).is_ok());
}
