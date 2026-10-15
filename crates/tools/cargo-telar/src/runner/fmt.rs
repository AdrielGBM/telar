//! `cargo telar fmt` — formats a project's `.rsx` and `.rs` files.
//!
//! Two formatters behind one command, because a Telar project is two languages. `.rsx` goes through [`telar_parser::format`], the same function the language server serves `textDocument/formatting` from, so a file formatted from a terminal and one formatted on save come out identical. `.rs` goes to `rustfmt`, one file at a time — which is the whole reason this command exists, since `cargo fmt` walks the module tree from the crate root and a telar crate declares that tree from a macro `cargo fmt` cannot expand.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::cli::FmtArgs;

pub(crate) fn run_fmt_cmd(args: FmtArgs) {
    let roots = if args.paths.is_empty() {
        vec![std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))]
    } else {
        args.paths.clone()
    };

    let mut sources = Vec::new();
    for root in &roots {
        collect(root, &mut sources);
    }
    sources.sort();
    sources.dedup();

    let (mut changed, mut failed) = (Vec::new(), Vec::new());
    for path in &sources {
        match format_file(path, args.check) {
            Ok(true) => changed.push(path.clone()),
            Ok(false) => {}
            Err(e) => failed.push((path.clone(), e)),
        }
    }

    for (path, error) in &failed {
        eprintln!("[cargo-telar] {}: {error}", display(path));
    }
    if args.check {
        for path in &changed {
            println!("{}", display(path));
        }
        let verdict = match (changed.len(), failed.len()) {
            (0, 0) => return println!("[cargo-telar] {} files already formatted", sources.len()),
            (n, 0) => format!("{n} file(s) need formatting"),
            (0, f) => format!("{f} file(s) could not be read"),
            (n, f) => format!("{n} file(s) need formatting, {f} could not be read"),
        };
        eprintln!("[cargo-telar] {verdict}");
        std::process::exit(1);
    }
    println!(
        "[cargo-telar] formatted {} of {} files",
        changed.len(),
        sources.len()
    );
    if !failed.is_empty() {
        std::process::exit(1);
    }
}

/// Every `.rsx` and `.rs` file Cargo compiles under `root`: the sources of each package found there or enclosing it, not every file the tree holds, so fixtures, generated corpora and scratch directories are never touched. `target/`, the generated `.telar/` tree (formatting it would be undone by the next build) and any dot-directory are skipped.
fn collect(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        out.extend(telar_project::collect_files_by_ext(
            root,
            &["rsx", "rs"],
            &|_| true,
        ));
        return;
    }
    let mut packages = packages_under(root);
    if let Some(enclosing) = root
        .ancestors()
        .find(|dir| dir.join("Cargo.toml").is_file())
    {
        packages.push(enclosing.to_path_buf());
    }
    packages.sort();
    packages.dedup();
    for package in &packages {
        for source_root in source_roots(package) {
            let found =
                telar_project::collect_files_by_ext(&source_root, &["rsx", "rs"], &|name| {
                    name != "target" && !name.starts_with('.')
                });
            out.extend(found.into_iter().filter(|path| path.starts_with(root)));
        }
    }
}

fn packages_under(root: &Path) -> Vec<PathBuf> {
    let mut packages = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        if dir.join("Cargo.toml").is_file() {
            packages.push(dir.clone());
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if entry.path().is_dir() && name != "target" && !name.starts_with('.') {
                pending.push(entry.path());
            }
        }
    }
    packages
}

/// The places Cargo looks for a package's sources, plus the directory of every target whose `path` the manifest declares outside them.
fn source_roots(package: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = ["src", "tests", "examples", "benches"]
        .iter()
        .map(|name| package.join(name))
        .filter(|dir| dir.is_dir())
        .collect();
    let build_script = package.join("build.rs");
    if build_script.is_file() {
        roots.push(build_script);
    }
    let manifest = std::fs::read_to_string(package.join("Cargo.toml"))
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok());
    for declared in manifest.iter().flat_map(declared_paths) {
        let path = package.join(declared);
        if roots.iter().any(|root| path.starts_with(root)) {
            continue;
        }
        if let Some(dir) = path.parent().filter(|dir| dir.is_dir()) {
            roots.push(dir.to_path_buf());
        }
    }
    roots
}

fn declared_paths(manifest: &toml::Table) -> Vec<String> {
    let path_of = |table: &toml::Value| {
        table
            .get("path")
            .and_then(toml::Value::as_str)
            .map(str::to_string)
    };
    let mut paths: Vec<String> = manifest.get("lib").and_then(path_of).into_iter().collect();
    for kind in ["bin", "test", "example", "bench"] {
        if let Some(targets) = manifest.get(kind).and_then(toml::Value::as_array) {
            paths.extend(targets.iter().filter_map(path_of));
        }
    }
    paths
}

/// Formats one file, reporting whether it changed. `check` reads and compares without writing.
fn format_file(path: &Path, check: bool) -> Result<bool, String> {
    let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let formatted = match path.extension().and_then(|e| e.to_str()) {
        // A document that does not parse is left exactly as it is, which is what every formatter does with a file it cannot read — the error belongs to the compiler, not to the formatter.
        Some("rsx") => match telar_parser::format::format_document(&source) {
            Some(formatted) => formatted,
            None => return Ok(false),
        },
        _ => rustfmt(&source)?,
    };
    if formatted == source {
        return Ok(false);
    }
    if !check {
        std::fs::write(path, &formatted).map_err(|e| e.to_string())?;
    }
    Ok(true)
}

/// Runs `rustfmt` over one file's text. Through stdin rather than by path so `--check` never writes, and so a file rustfmt rejects leaves the original untouched instead of half-formatted.
fn rustfmt(source: &str) -> Result<String, String> {
    use std::io::Write;

    let mut child = Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout", "--quiet"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run rustfmt: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("rustfmt took no stdin")?
        .write_all(source.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

fn display(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

#[cfg(test)]
#[path = "fmt_test.rs"]
mod tests;
