//! `cargo telar new` and `cargo telar init`: writing a project that already names one target.
//!
//! Both write the same files (`scaffold_files`); they differ only in what they consider a conflict.
//! `new` wants an empty destination and refuses the whole directory otherwise, matching the workspace
//! convention that it creates a place rather than moving into one. `init` is for a place that already
//! exists for another reason (a fresh `git init`, a Nix flake already checked in) — it writes into
//! whatever is there and refuses only the specific names it would itself have to overwrite, listing
//! every one so nothing is ever silently replaced. Neither ever touches a file this scaffold does not
//! itself own: an existing `Cargo.toml` is exactly such a conflict, refused for the same reason as any
//! other file in the list, since merging one honestly is a job for `cargo add`, not for this command.

use std::fs;
use std::path::{Path, PathBuf};

use crate::runner::cli::{InitArgs, NewArgs, Target, WebRenderer};
use crate::runner::config::WEB_PROFILE_TOML;

const TELAR_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) fn run_new_cmd(args: NewArgs) {
    let NewArgs {
        path,
        name,
        target,
        renderer,
    } = args;
    let crate_name = derive_crate_name(&path, name);
    if let Err(msg) = validate_name(&crate_name) {
        fail(&msg);
    }
    if path.exists()
        && fs::read_dir(&path)
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        fail(&format!(
            "`{}` already exists and is not empty",
            path.display()
        ));
    }

    let files = scaffold_files(&crate_name, target, renderer);
    write_all(&path, &files);
    finish(&path, &crate_name, target, renderer);
}

pub(crate) fn run_init_cmd(args: InitArgs) {
    let InitArgs {
        path,
        name,
        target,
        renderer,
    } = args;
    let path = path.unwrap_or_else(|| PathBuf::from("."));
    let crate_name = derive_crate_name(&path, name);
    if let Err(msg) = validate_name(&crate_name) {
        fail(&msg);
    }

    let files = scaffold_files(&crate_name, target, renderer);
    let conflicts = conflicts_in(&path, &files);
    if !conflicts.is_empty() {
        fail(&format!(
            "`{}` already has files this scaffold would write, so nothing was written:\n{}\n\nRemove or rename them first — `init` never overwrites an existing file.",
            path.display(),
            conflicts
                .iter()
                .map(|rel| format!("  {rel}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    write_all(&path, &files);
    finish(&path, &crate_name, target, renderer);
}

/// The files `new` and `init` both write, differing only in the target (and, for the web, the renderer)
/// baked into the manifest.
fn scaffold_files(
    crate_name: &str,
    target: Target,
    renderer: Option<WebRenderer>,
) -> Vec<(&'static str, String)> {
    let module = crate_name.replace('-', "_");
    vec![
        ("Cargo.toml", manifest(crate_name, target, renderer)),
        ("telar.toml", config(crate_name, target)),
        (".gitignore", "/target\n/.telar\n".to_string()),
        ("src/main.rs", main_rs(&module)),
        ("src/lib.rs", LIB_RS.to_string()),
        ("src/app.rs", APP_RS.to_string()),
        ("src/theme.rs", THEME_RS.to_string()),
        ("src/home.rsx", HOME_RSX.to_string()),
    ]
}

/// Which of `files` already exist under `root` — what `init` refuses to overwrite. A pure query so the
/// conflict list can be tested without going through the process-exiting command itself.
fn conflicts_in<'a>(root: &Path, files: &[(&'a str, String)]) -> Vec<&'a str> {
    files
        .iter()
        .map(|(rel, _)| *rel)
        .filter(|rel| root.join(rel).exists())
        .collect()
}

fn write_all(root: &Path, files: &[(&str, String)]) {
    for (rel, contents) in files {
        write(root, rel, contents);
    }
}

/// Transpiles the freshly written project and prints the closing instructions. Shared by `new` and `init`
/// since both leave a project in the same state once their files are on disk.
fn finish(path: &Path, crate_name: &str, target: Target, renderer: Option<WebRenderer>) {
    // Left transpiled so the project checks out of the box: opened in an editor before its first `cargo telar` command it would otherwise greet its author with the error naming one. No `cargo metadata` to resolve a version against — nothing is fetched yet, and the manifest just written pins this binary's own.
    let producer = format!("cargo-telar {TELAR_VERSION}");
    super::transpile::transpile_member(path, &producer, TELAR_VERSION);

    let dev = match (target, renderer) {
        (Target::Desktop, _) => "cargo telar dev".to_string(),
        (Target::Web, Some(renderer)) => {
            format!(
                "cargo telar dev --target web --renderer {}",
                renderer.as_str()
            )
        }
        (other, _) => format!("cargo telar dev --target {}", target_name(other)),
    };
    println!("Created `{crate_name}` at {}", path.display());
    println!();
    println!("    cd {}", path.display());
    println!("    {dev}");
    println!();
    println!(
        "Other targets are one word each: `default = [\"…\"]` in Cargo.toml, or `--target` on the command line."
    );
}

/// The package name a project starts with: the flag when one was passed, otherwise the destination
/// directory's own name — resolved against the current directory first when the destination is `.`,
/// since a bare dot has no name of its own to read.
fn derive_crate_name(path: &Path, name: Option<String>) -> String {
    name.or_else(|| name_from_path(path, || std::env::current_dir().ok()))
        .unwrap_or_else(|| {
            fail(&format!(
                "cannot derive a package name from `{}` — pass --name",
                path.display()
            ))
        })
}

/// `derive_crate_name`'s pure half: `cwd` is a lookup rather than a direct call so tests can hand it a
/// fixed directory instead of touching the process's real one.
fn name_from_path(path: &Path, cwd: impl FnOnce() -> Option<PathBuf>) -> Option<String> {
    let effective = if path.as_os_str() == std::ffi::OsStr::new(".") {
        cwd().unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    effective
        .file_name()
        .and_then(|n| n.to_str())
        .map(|segment| segment.to_string())
}

fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        fail(&format!("cannot create {}: {e}", parent.display()));
    }
    if let Err(e) = fs::write(&path, contents) {
        fail(&format!("cannot write {}: {e}", path.display()));
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("package name is empty".to_string());
    }
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(format!("`{name}` cannot start with a digit"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "`{name}` may only contain letters, digits, `-` and `_`"
        ));
    }
    Ok(())
}

fn target_name(target: Target) -> &'static str {
    match target {
        Target::Desktop => "desktop",
        Target::Tui => "tui",
        Target::Web => "web",
        Target::Android => "android",
    }
}

fn manifest(name: &str, target: Target, renderer: Option<WebRenderer>) -> String {
    // `Target::feature` is what tells `web-dom` and `web` apart; every other target names the same word either way.
    let default_feature = target.feature(renderer);
    let tooling = super::config::tooling_feature_entry();
    // A module crosses a network before it runs an instruction, which `[profile.release]` was never tuned for — `cargo telar build --target web` passes `--profile web` and needs the entry to exist (see docs/build-tuning.md).
    let web_profile = match target {
        Target::Web => format!("\n{WEB_PROFILE_TOML}"),
        _ => String::new(),
    };
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

# `cdylib` is the library `cargo telar dev` swaps on a hot reload; `lib` is what the binary and the tests link.
[lib]
crate-type = ["cdylib", "lib"]

[[bin]]
name = "{name}"
path = "src/main.rs"

[dependencies]
# `default-features = false` so this file is the only place a target is named: telar's own default is a
# desktop window, and it would otherwise ride along into a build that asked for the terminal or a browser.
telar = {{ version = "{TELAR_VERSION}", default-features = false, features = [
    "runtime",
    "components",
] }}

[features]
# The target this project builds for. Each one is complete on its own — swapping one word here, or
# passing `--target` (and, for the browser, `--renderer`) on the command line, is the whole of the change.
default = ["{default_feature}"]
desktop = ["telar/desktop"]
tui = ["telar/tui"]
web = ["telar/web"]
web-dom = ["telar/web-dom"]
android = ["telar/android"]
# Never turned on by a build: `cargo telar` names these on the command line for its own commands, and this entry is what lets Cargo.lock pin what they bring in.
{tooling}

[profile.dev]
opt-level = 1
debug = "line-tables-only"

# Dependencies compile once and are not what you are stepping through: optimising them buys a renderer and
# a layout engine that run at a usable speed in dev, and dropping their debug info takes most of the weight
# out of the cdylib a hot reload rewrites. Your own crates keep theirs, so panics still name a line.
[profile.dev.package."*"]
opt-level = 3
debug = false

# Do not add `panic = "abort"`: Telar unwinds to keep one panicking surface from taking the process with it.
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = "symbols"
{web_profile}"#
    )
}

fn config(name: &str, target: Target) -> String {
    let window = match target {
        Target::Desktop | Target::Android => format!(
            r#"
[telar.dev.window]
title = "{name}"
width = 1000
height = 700
"#
        ),
        Target::Tui | Target::Web => String::new(),
    };
    format!(
        r#"[telar]
# "auto" draws with the GPU where there is one and the CPU where there is not.
backend = "auto"
{window}"#
    )
}

fn main_rs(module: &str) -> String {
    format!(
        r#"fn main() {{
    {module}::run();
}}
"#
    )
}

const LIB_RS: &str = r#"telar::app!(
    theme::AppTheme,
    {
        telar::set_theme(theme::AppTheme::light());
    },
    telar::AppConfig::default(),
    app::Root
);
"#;

const APP_RS: &str = r#"use telar::{App, Color, ScrollPage, reset_layout_runtime};

use crate::theme::theme;

pub struct Root;

impl App for Root {
    fn root(&self) -> Box<dyn telar::Component> {
        reset_layout_runtime();
        let content = crate::home::home(
            crate::home::HomeProps::props().build(),
            telar::Children::default(),
        )
        .expect("home layout failed");
        Box::new(ScrollPage::new(content).expect("page layout failed"))
    }

    fn clear_color(&self) -> Option<Color> {
        Some(theme().surface_alt)
    }
}
"#;

const THEME_RS: &str = r#"use telar::{Color, ThemeTokens, use_theme};

/// The application's design tokens. Every component reads these, so a restyle happens here and nowhere else.
#[derive(Clone, ThemeTokens)]
pub struct AppTheme {
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

impl AppTheme {
    pub fn light() -> Self {
        Self {
            primary: Color::rgba(0.26, 0.38, 0.93, 1.0),
            on_primary: Color::WHITE,
            surface: Color::WHITE,
            surface_alt: Color::rgba(0.96, 0.97, 0.99, 1.0),
            border: Color::rgba(0.86, 0.87, 0.93, 1.0),
            ink: Color::rgba(0.09, 0.10, 0.18, 1.0),
            muted: Color::rgba(0.46, 0.48, 0.58, 1.0),
            scrollbar: Color::rgba(0.66, 0.68, 0.76, 1.0),
            success: Color::rgba(0.18, 0.69, 0.45, 1.0),
            warning: Color::rgba(0.90, 0.62, 0.16, 1.0),
            error: Color::rgba(0.86, 0.26, 0.30, 1.0),
            info: Color::rgba(0.24, 0.55, 0.90, 1.0),
            highlight_low: Color::rgba(0.0, 0.0, 0.0, 0.04),
            highlight_med: Color::rgba(0.0, 0.0, 0.0, 0.08),
            highlight_high: Color::rgba(0.0, 0.0, 0.0, 0.14),
            radius: 10.0,
            spacing: 8.0,
            icon_size: 16.0,
        }
    }

    pub fn dark() -> Self {
        Self {
            primary: Color::rgba(0.45, 0.58, 1.0, 1.0),
            on_primary: Color::rgba(0.05, 0.06, 0.12, 1.0),
            surface: Color::rgba(0.11, 0.12, 0.16, 1.0),
            surface_alt: Color::rgba(0.07, 0.08, 0.11, 1.0),
            border: Color::rgba(0.24, 0.26, 0.32, 1.0),
            ink: Color::rgba(0.92, 0.93, 0.96, 1.0),
            muted: Color::rgba(0.60, 0.63, 0.72, 1.0),
            scrollbar: Color::rgba(0.36, 0.38, 0.46, 1.0),
            success: Color::rgba(0.30, 0.78, 0.55, 1.0),
            warning: Color::rgba(0.96, 0.72, 0.28, 1.0),
            error: Color::rgba(0.94, 0.42, 0.44, 1.0),
            info: Color::rgba(0.42, 0.68, 0.98, 1.0),
            highlight_low: Color::rgba(1.0, 1.0, 1.0, 0.05),
            highlight_med: Color::rgba(1.0, 1.0, 1.0, 0.10),
            highlight_high: Color::rgba(1.0, 1.0, 1.0, 0.16),
            radius: 10.0,
            spacing: 8.0,
            icon_size: 16.0,
        }
    }
}

/// The active theme, read reactively: a component calling this re-runs when the theme changes.
pub fn theme() -> AppTheme {
    use_theme::<AppTheme>()
}
"#;

const HOME_RSX: &str = r#"[logic]
let clicks = signal(0i32);

[view]
col grow:1 gap:16 pad:32 align:center fill:$theme.surface_alt
    text "Hello from Telar" font_size:32 color:$theme.ink
    text "Clicks · {$clicks}" font_size:16 color:$theme.muted
    row gap:10
        button label:"+1" fill:$theme.primary on_press:(|| $clicks += 1)
        button label:"Reset" ghost on_press:(|| $clicks.set(0))

[preview "Home"]
home
"#;

#[cfg(test)]
#[path = "new_test.rs"]
mod tests;
