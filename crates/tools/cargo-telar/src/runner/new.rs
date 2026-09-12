//! `cargo telar new`: writing a project that already names one target.

use std::fs;
use std::path::Path;

use crate::runner::cli::{NewArgs, Target};

const TELAR_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) fn run_new_cmd(args: NewArgs) {
    let NewArgs { path, name, target } = args;
    let crate_name = match name.or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|segment| segment.to_string())
    }) {
        Some(name) => name,
        None => fail(&format!(
            "cannot derive a package name from `{}` — pass --name",
            path.display()
        )),
    };
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

    let module = crate_name.replace('-', "_");
    for (rel, contents) in [
        ("Cargo.toml", manifest(&crate_name, target)),
        ("telar.toml", config(&crate_name, target)),
        (".gitignore", "/target\n/.telar\n".to_string()),
        ("src/main.rs", main_rs(&module)),
        ("src/lib.rs", LIB_RS.to_string()),
        ("src/app.rs", APP_RS.to_string()),
        ("src/theme.rs", THEME_RS.to_string()),
        ("src/home.rsx", HOME_RSX.to_string()),
    ] {
        write(&path, rel, &contents);
    }

    // Left transpiled so the project checks out of the box: opened in an editor before its first `cargo telar` command it would otherwise greet its author with the error naming one. No `cargo metadata` to resolve a version against — nothing is fetched yet, and the manifest just written pins this binary's own.
    let producer = format!("cargo-telar {TELAR_VERSION}");
    super::transpile::transpile_member(&path, &producer, TELAR_VERSION);

    let dev = match target {
        Target::Desktop => "cargo telar dev".to_string(),
        other => format!("cargo telar dev --target {}", target_name(other)),
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

fn manifest(name: &str, target: Target) -> String {
    let target = target_name(target);
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
# The target this project builds for. Each of the four below is complete on its own — swapping one word
# here, or passing `--target` on the command line, is the whole of the change.
default = ["{target}"]
desktop = ["telar/desktop"]
tui = ["telar/tui"]
web = ["telar/web"]
android = ["telar/android"]

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
"#
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
