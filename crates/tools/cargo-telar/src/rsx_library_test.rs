//! The end-to-end proof that a `[telar] library` builds from the package it publishes: `plugins/telar-rsx-fixture` passes `cargo telar package --check`, is packaged by cargo, unpacked read-only, and compiled as a dependency of a generated application that the CLI transpiles while never running on the library.
//!
//! The application overrides one of the library's strings, names a theme of its own and draws the library's component headlessly, once with the Plain flavour and once with the features `cargo telar dev` and `cargo telar preview` turn on. Afterwards the unpacked library has to be byte-for-byte what cargo packaged.
//!
//! It compiles a few hundred crates into its own target directory under `target/tmp`, so it is ignored by default: run it with `cargo test -p cargo-telar --test rsx_library -- --ignored`. Every cargo it starts runs offline, so the registry has to hold what `Cargo.lock` names first (`cargo fetch`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const FIXTURE: &str = "telar-rsx-fixture";
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[test]
#[ignore = "builds a generated application twice: run it with `cargo test -p cargo-telar --test rsx_library -- --ignored`"]
fn a_packaged_rsx_library_builds_as_a_read_only_dependency() {
    let repo = repo_root();
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rsx-library");
    let target_dir = work.join("target");
    let run = work.join("run");
    remove_tree(&run);
    std::fs::create_dir_all(&run).expect("create the run directory");

    run_cli(&repo, &target_dir, &["package", "-p", FIXTURE, "--check"]);
    let library = unpack(&package_fixture(&repo, &target_dir), &run);
    let packaged = snapshot(&library);
    set_tree_writable(&library, false);

    let consumer = run.join("consumer");
    write_consumer(&consumer, &repo, &library);
    run_cli(&consumer, &target_dir, &["transpile"]);
    run_cargo(&consumer, &target_dir, &["test"]);
    run_cargo(
        &consumer,
        &target_dir,
        &[
            "test",
            "--features",
            "telar/hot-reload,telar/preview,telar/dev",
        ],
    );

    assert!(
        snapshot(&library) == packaged,
        "building the application changed the unpacked library at {}",
        library.display()
    );
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("cargo-telar sits three levels below the repository root")
        .to_path_buf()
}

fn run_cli(dir: &Path, target_dir: &Path, args: &[&str]) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-telar"));
    command.args(args);
    run(command, dir, target_dir);
}

fn run_cargo(dir: &Path, target_dir: &Path, args: &[&str]) {
    let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command.args(args);
    run(command, dir, target_dir);
}

/// Runs `command` in `dir` offline against `target_dir`, which is never the workspace's own: the `cargo test` running this holds that one's lock until it returns.
fn run(mut command: Command, dir: &Path, target_dir: &Path) {
    let shown = format!("{command:?} in {}", dir.display());
    let output = command
        .current_dir(dir)
        .env("CARGO_TARGET_DIR", target_dir)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .unwrap_or_else(|e| panic!("could not start {shown}: {e}"));
    assert!(
        output.status.success(),
        "{shown} failed with {}\n--- stdout\n{}\n--- stderr\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn package_fixture(repo: &Path, target_dir: &Path) -> PathBuf {
    // A lockfile is read by no build that depends on the package, and writing one resolves its dependencies against the registry, which offline, or before this telar is released, has no answer.
    run_cargo(
        repo,
        target_dir,
        &[
            "package",
            "--no-verify",
            "--allow-dirty",
            "--exclude-lockfile",
            "-p",
            FIXTURE,
        ],
    );
    target_dir
        .join("package")
        .join(format!("{FIXTURE}-{VERSION}.crate"))
}

fn unpack(crate_file: &Path, into: &Path) -> PathBuf {
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(crate_file)
        .arg("-C")
        .arg(into)
        .status()
        .expect("could not start tar");
    assert!(
        status.success(),
        "tar could not unpack {}",
        crate_file.display()
    );
    into.join(format!("{FIXTURE}-{VERSION}"))
}

/// Every file and directory under `root`, each file with its bytes.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("read the unpacked library") {
            let path = entry.expect("read a directory entry").path();
            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            if path.is_dir() {
                entries.insert(relative, None);
                pending.push(path);
            } else {
                let bytes = std::fs::read(&path).expect("read an unpacked file");
                entries.insert(relative, Some(bytes));
            }
        }
    }
    entries
}

/// Directories are made writable before what they hold and read-only after it, so a walk never meets a directory it cannot enter or change.
fn set_tree_writable(path: &Path, writable: bool) {
    let is_dir = path.is_dir();
    if writable {
        set_writable(path, true);
    }
    if is_dir {
        for entry in std::fs::read_dir(path).expect("read a directory to change its permissions") {
            set_tree_writable(&entry.expect("read a directory entry").path(), writable);
        }
    }
    if !writable {
        set_writable(path, false);
    }
}

fn set_writable(path: &Path, writable: bool) {
    let mut permissions = std::fs::metadata(path)
        .expect("read permissions")
        .permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = permissions.mode();
        permissions.set_mode(match writable {
            true => mode | 0o200,
            false => mode & !0o222,
        });
    }
    #[cfg(not(unix))]
    permissions.set_readonly(!writable);
    std::fs::set_permissions(path, permissions).expect("change permissions");
}

fn remove_tree(path: &Path) {
    if !path.exists() {
        return;
    }
    set_tree_writable(path, true);
    std::fs::remove_dir_all(path).expect("remove the previous run");
}

fn toml_string(path: &Path) -> String {
    toml::Value::String(path.display().to_string()).to_string()
}

fn write_consumer(dir: &Path, repo: &Path, library: &Path) {
    let manifest = format!(
        r#"[package]
name = "rsx-library-consumer"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
telar = {{ version = "={VERSION}", default-features = false, features = ["runtime", "shaper", "testing"] }}
telar-rsx-fixture = {{ path = {library} }}

[patch.crates-io]
telar = {{ path = {telar} }}

[workspace]
"#,
        library = toml_string(library),
        telar = toml_string(&repo.join("crates/telar")),
    );
    let files = [
        ("Cargo.toml", manifest.as_str()),
        ("telar.toml", CONSUMER_TELAR_TOML),
        ("locales/en.toml", CONSUMER_LOCALE),
        ("src/lib.rs", CONSUMER_LIB_RS),
        ("src/theme.rs", CONSUMER_THEME_RS),
        ("src/welcome.rsx", CONSUMER_WELCOME_RSX),
        ("src/lib_test.rs", CONSUMER_LIB_TEST_RS),
    ];
    for (relative, contents) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a file has a parent"))
            .expect("create a consumer directory");
        std::fs::write(&path, contents).expect("write a consumer file");
    }
    // Offline, the versions the workspace already fetched are the only ones the application can resolve to, and the same ones let it reuse what earlier runs compiled.
    std::fs::copy(repo.join("Cargo.lock"), dir.join("Cargo.lock")).expect("copy Cargo.lock");
}

const CONSUMER_TELAR_TOML: &str = r#"[telar]
theme = "crate::theme::ConsumerTheme"
prelude = ["telar_rsx_fixture"]
"#;

const CONSUMER_LOCALE: &str = r#"[telar_rsx_fixture]
greeting = "Welcome aboard, {name}!"
"#;

const CONSUMER_LIB_RS: &str = r#"telar::rsx_modules!();

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
"#;

const CONSUMER_THEME_RS: &str = r#"use telar::{Color, ThemeTokens};

#[derive(Clone, ThemeTokens)]
pub struct ConsumerTheme {
    pub primary: Color,
    pub on_primary: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub border: Color,
    pub ink: Color,
    pub muted: Color,
    pub scrollbar: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    pub highlight_low: Color,
    pub highlight_med: Color,
    pub highlight_high: Color,
    pub radius: f32,
    pub spacing: f32,
    pub icon_size: f32,
}

impl ConsumerTheme {
    pub const PRIMARY: Color = Color::rgba(0.8, 0.1, 0.5, 1.0);

    pub fn new() -> Self {
        let grey = Color::rgba(0.5, 0.5, 0.5, 1.0);
        Self {
            primary: Self::PRIMARY,
            on_primary: Color::WHITE,
            surface: Color::WHITE,
            surface_alt: grey,
            border: grey,
            ink: Color::BLACK,
            muted: grey,
            scrollbar: grey,
            success: grey,
            warning: grey,
            error: grey,
            info: grey,
            highlight_low: grey,
            highlight_med: grey,
            highlight_high: grey,
            radius: 6.0,
            spacing: 8.0,
            icon_size: 16.0,
        }
    }
}
"#;

const CONSUMER_WELCOME_RSX: &str = r#"[view]
col pad:16 fill:$theme.surface
    greeting_card name:"Ada"
"#;

const CONSUMER_LIB_TEST_RS: &str = r#"use telar::testing::{mount, texts};
use telar::{Children, ComponentList, DrawCommand, WindowRoot, reset_layout_runtime};

use crate::theme::ConsumerTheme;
use crate::welcome::{WelcomeProps, welcome};

fn render() -> ComponentList {
    telar::install_default_text_metrics();
    reset_layout_runtime();
    let page = welcome(WelcomeProps::props().build(), Children::default()).expect("the page lays out");
    mount(WindowRoot::new(page), 480, 160)
}

#[test]
fn the_library_component_draws_with_the_application_catalog_and_theme() {
    telar::set_theme(ConsumerTheme::new());
    telar::set_catalog(&crate::__rsx_i18n::CATALOG);

    telar::set_locale("en");
    let tree = render();
    let drawn = texts(&tree);
    assert!(drawn.iter().any(|text| text == "Welcome aboard, Ada!"), "the application's override is drawn: {drawn:?}");
    let commands = tree.commands();
    assert!(
        commands.iter().any(|command| matches!(command, DrawCommand::Rect { style, .. } if style.fill.as_ref().is_some_and(|paint| paint.solid_color() == ConsumerTheme::PRIMARY))),
        "the library's `$theme.primary` reads the application's theme: {commands:?}"
    );
    assert!(
        commands.iter().any(|command| matches!(command, DrawCommand::Path { .. })),
        "the library's baked SVG is drawn: {commands:?}"
    );

    telar::set_locale("es");
    let drawn = texts(&render());
    assert!(drawn.iter().any(|text| text == "¡Hola, Ada!"), "a locale the application does not override falls back to the library's catalog: {drawn:?}");
}
"#;
