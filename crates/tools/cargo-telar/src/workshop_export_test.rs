//! The end-to-end proof of the static export: `apps/catalogue` exported with `cargo telar preview --build <dir> --target web` has a page a static host serves at the address of every preview it lists, and of its matrix, each loading the module the build wrote.
//!
//! It compiles the catalogue for this machine and for the browser into its own target directory under `target/tmp`, so it is ignored by default: run it with `cargo test -p cargo-telar --test workshop_export -- --ignored`.

use std::path::Path;
use std::process::{Command, Output};

use telar_project::protocol::preview_location;

#[path = "served_site_test.rs"]
mod served_site;

use served_site::{repo_root, served_at};

#[test]
#[ignore = "builds the catalogue twice: run it with `cargo test -p cargo-telar --test workshop_export -- --ignored`"]
fn every_preview_of_the_catalogue_has_a_page_in_its_export() {
    let repo = repo_root();
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("workshop-export");
    let target_dir = work.join("target");
    let site = work.join("site");

    let listing = run_cli(
        &repo,
        &target_dir,
        &["preview", "-p", "catalogue", "--list"],
    );
    let ids: Vec<String> = String::from_utf8_lossy(&listing.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    assert!(!ids.is_empty(), "the catalogue lists no previews");

    let site_arg = site.display().to_string();
    run_cli(
        &repo,
        &target_dir,
        &[
            "preview",
            "-p",
            "catalogue",
            "--build",
            &site_arg,
            "--target",
            "web",
        ],
    );

    for page in ["index.html", "404.html"] {
        assert_loads_the_module(&site, &site.join(page));
    }
    for id in &ids {
        let preview = preview_location(id);
        let matrix = format!("/matrix/{}", preview.trim_start_matches("/preview/"));
        for address in [preview, matrix] {
            let page = served_at(&site, &address)
                .unwrap_or_else(|| panic!("{address} names no file of the site"));
            assert!(
                page.is_file(),
                "{address} has no page in the export ({} is missing)",
                page.display()
            );
            assert_loads_the_module(&site, &page);
        }
    }
}

fn assert_loads_the_module(site: &Path, page: &Path) {
    let html = std::fs::read_to_string(page)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", page.display()));
    let glue = html
        .split("import init from \"/")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_else(|| panic!("{} imports no glue: {html}", page.display()));
    assert!(
        site.join(glue).is_file(),
        "{} imports /{glue}, which the export does not hold",
        page.display()
    );
}

/// Runs the CLI in `dir` against `target_dir`, which is never the workspace's own: the `cargo test` running this holds that one's lock until it returns.
fn run_cli(dir: &Path, target_dir: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-telar"));
    command.args(args);
    let shown = format!("{command:?} in {}", dir.display());
    let output = command
        .current_dir(dir)
        .env("CARGO_TARGET_DIR", target_dir)
        .output()
        .unwrap_or_else(|e| panic!("could not start {shown}: {e}"));
    assert!(
        output.status.success(),
        "{shown} failed with {}\n--- stdout\n{}\n--- stderr\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
