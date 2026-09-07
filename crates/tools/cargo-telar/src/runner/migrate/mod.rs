//! `cargo telar migrate` — rewrites a project's `.rsx` files into the one value grammar.
//!
//! Every rewrite here is mechanical: a spelling that used to mean something the language no longer has a second way to say. What is *not* mechanical is reported instead of guessed — a `build "…"` or `widget "…"` needs names for positional arguments, and only a person knows them.
//!
//! Run it once per project, then `cargo telar fmt` and the usual build. It is idempotent: a file already in the new grammar comes out byte-identical, which is what makes `--check` a CI answer.
//!
//! Quoted text is left alone throughout. A `"…"` is the author's data, and a documentation string showing the old spelling is prose about the language rather than a use of it — rewriting one would change what a sentence says.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::cli::MigrateArgs;
mod imports;
mod reactive;
mod text;
mod theme;
mod view;
mod zones;

use imports::{component_modules, imports_for_tags};
use reactive::{reactive_closures, reactive_props, shared_handlers};
use theme::{style_constants_to_logic, theme_calls, theme_reads};
use view::{clip_shapes, colonise, i18n_macro};
use zones::{Section, zones};

pub(crate) fn run_migrate_cmd(args: MigrateArgs) {
    let roots = match args.paths.is_empty() {
        true => vec![std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))],
        false => args.paths.clone(),
    };

    let mut sources = Vec::new();
    for root in &roots {
        collect_rsx(root, &mut sources);
    }
    sources.sort();
    sources.dedup();

    let modules = component_modules(&sources);
    let reactive = reactive_props(&sources);
    let (mut changed, mut failed, mut manual) = (Vec::new(), Vec::new(), Vec::new());
    for path in &sources {
        let Ok(source) = std::fs::read_to_string(path) else {
            failed.push(path.clone());
            continue;
        };
        manual.extend(escapes_needing_a_person(path, &source));
        let migrated = migrate(&source, &modules, own_stem(path), &reactive);
        if migrated == source {
            continue;
        }
        changed.push(path.clone());
        if !args.check && std::fs::write(path, &migrated).is_err() {
            failed.push(path.clone());
        }
    }

    for (path, line, text) in &manual {
        println!("{}:{line}: {text}", display(path));
    }
    if !manual.is_empty() {
        println!(
            "[cargo-telar] {} escape(s) need a component with named props — converting them is hand work",
            manual.len()
        );
    }
    for path in &changed {
        println!("{}", display(path));
    }
    for path in &failed {
        eprintln!(
            "[cargo-telar] {}: could not be read or written",
            display(path)
        );
    }
    let verb = match args.check {
        true => "would be rewritten",
        false => "rewritten",
    };
    println!(
        "[cargo-telar] {} of {} file(s) {verb}",
        changed.len(),
        sources.len()
    );
    if !failed.is_empty() || (args.check && !changed.is_empty()) {
        std::process::exit(1);
    }
}

/// Every rewrite, in the order the later ones depend on: the colon form first, so what follows reads one grammar rather than two.
fn migrate(
    source: &str,
    modules: &BTreeMap<String, String>,
    own: &str,
    reactive: &BTreeMap<String, Vec<String>>,
) -> String {
    // A file that binds `theme` itself is not talking about the view's handle: its own binding shadows it, so leaving it alone keeps exactly the behaviour the file had.
    let binds_own_theme = zones(source).iter().any(|z| {
        z.section == Section::Logic
            && z.body
                .lines()
                .any(|line| line.starts_with("let theme =") || line.starts_with("let theme:"))
    });
    let read_theme = |body: &str| match binds_own_theme {
        true => body.to_string(),
        false => theme_reads(body),
    };

    let mut out = String::with_capacity(source.len());
    for zone in zones(source) {
        let body = match zone.section {
            Section::View | Section::Preview => {
                let body = colonise(zone.body);
                let body = i18n_macro(&body);
                let body = read_theme(&body);
                reactive_closures(&clip_shapes(&body), reactive)
            }
            Section::Style => read_theme(zone.body),
            Section::Logic => match binds_own_theme {
                true => shared_handlers(zone.body),
                false => shared_handlers(&theme_calls(zone.body)),
            },
            Section::None => zone.body.to_string(),
        };
        out.push_str(zone.header);
        out.push_str(&body);
    }
    style_constants_to_logic(&imports_for_tags(&out, modules, own))
}

/// The `build "…"` and `widget "…"` sites, reported rather than guessed: turning `build "tray_icon(item, config, fg, size)?"` into a tag needs *names* for four positional arguments, and only a person knows them.
fn escapes_needing_a_person(path: &Path, source: &str) -> Vec<(PathBuf, usize, String)> {
    let mut out = Vec::new();
    for zone in zones(source) {
        if !matches!(zone.section, Section::View | Section::Preview) {
            continue;
        }
        let offset = source.len() - zone.body.len();
        let first_line = source[..offset].lines().count();
        for (i, line) in zone.body.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("build \"") || trimmed.starts_with("widget \"") {
                out.push((path.to_path_buf(), first_line + i + 1, trimmed.to_string()));
            }
        }
    }
    out
}

fn collect_rsx(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        if root.extension().and_then(|e| e.to_str()) == Some("rsx") {
            out.push(root.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "target" {
            continue;
        }
        collect_rsx(&path, out);
    }
}

/// The component this file *is*, which it never imports: a `[preview]` calling it is a sibling function in the same generated module.
fn own_stem(path: &Path) -> &str {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("")
}

fn display(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
#[path = "migrate_test.rs"]
mod tests;
