//! The safety net: every `.rsx` in the workspace transpiled into a committed snapshot of the generated Rust *and* of its source map, so a refactor of the transpiler has to declare what it changed.
//!
//! The map is snapshotted beside the code because a refactor can leave the output byte-identical and still ruin the column mapping — and a wrong column is a diagnostic pointing at the wrong text, which is worse than none. Snapshotting only the Rust would let that through silently.
//!
//! Run with `UPDATE_GOLDEN=1` to rewrite the snapshots after a deliberate change, then read the diff.

use std::path::{Path, PathBuf};

use telar_transpiler::{RsxSpan, SourceMap, TranspiledSource};

/// One package whose `src/` tree the harness transpiles, with the theme type its `app!` invocation names — `use_theme::<T>()` is typed by it, so the wrong one here would snapshot code the build never emits.
struct Project {
    name: &'static str,
    manifest: &'static str,
    theme: &'static str,
}

const PROJECTS: &[Project] = &[
    Project {
        name: "sandbox",
        manifest: "apps/sandbox",
        theme: "core::theme::SandboxTheme",
    },
    Project {
        name: "landing",
        manifest: "apps/landing",
        theme: "theme::LandingTheme",
    },
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("crates/tools/telar-transpiler is three levels below the workspace root")
        .to_path_buf()
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Transpiles one package exactly the way `app!` does: the same component name (the file stem), the same asset root. There is no pre-pass to match any more — a file transpiles knowing only itself.
fn transpile_project(project: &Project) -> Vec<(PathBuf, TranspiledSource)> {
    let manifest = workspace_root().join(project.manifest);
    let src_dir = manifest.join("src");
    let assets_root = telar_transpiler::assets_root(&manifest);
    let mut files = telar_transpiler::find_rsx_files(&src_dir);
    files.sort();
    assert!(
        !files.is_empty(),
        "{} has no .rsx files — the harness would pass by covering nothing",
        project.name
    );

    files
        .into_iter()
        .map(|rsx| {
            let source = std::fs::read_to_string(&rsx)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", rsx.display()));
            let stem = telar_transpiler::component_name(&rsx);
            let out = telar_transpiler::transpile_source(
                &source,
                &stem,
                Some(project.theme),
                Some(assets_root.as_path()),
            )
            .unwrap_or_else(|e| panic!("{} failed to transpile: {e}", rsx.display()));
            let rel = telar_transpiler::relative_output_path(&rsx, &src_dir)
                .unwrap_or_else(|| panic!("{} is not under src/", rsx.display()));
            (rel, out)
        })
        .collect()
}

/// The source map as a reviewable snapshot: one line per generated line, and the verbatim expression spans with the `.rsx` text they claim to cover.
///
/// Deliberately not the JSON the `.rs.map` sidecar carries. That form is one line, so a diff over it says only "the map changed" — and the whole reason for snapshotting the map is to be told *which* line stopped pointing where it did. Quoting the covered source text is what makes a shifted span self-evident.
fn render_map(out: &TranspiledSource, rsx_source: &str) -> String {
    let mut text = String::new();
    text.push_str("# generated line -> rsx line (1-based; '.' means transpiler-injected)\n");
    for (gen_line, origin) in out.source_map.iter().enumerate() {
        match origin {
            Some(rsx_line) => text.push_str(&format!("{}\t{}\n", gen_line + 1, rsx_line + 1)),
            None => text.push_str(&format!("{}\t.\n", gen_line + 1)),
        }
    }
    text.push_str("\n# verbatim expression spans: rsx[start..end] -> gen[start] `source text`\n");
    for span in &out.expr_spans {
        let start = span.rsx_start as usize;
        let end = start + span.len as usize;
        let covered = rsx_source.get(start..end).unwrap_or("<out of bounds>");
        text.push_str(&format!(
            "{start}..{end}\t{}\t`{covered}`\n",
            span.gen_start
        ));
    }
    text
}

/// Compares `actual` against the snapshot at `path`, or rewrites it under `UPDATE_GOLDEN=1`. Collects the failure instead of panicking so one run reports every drifted file rather than the first.
fn check(path: &Path, actual: &str, failures: &mut Vec<String>) {
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("cannot create the golden directory");
        }
        std::fs::write(path, actual)
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
        return;
    }
    let Ok(expected) = std::fs::read_to_string(path) else {
        failures.push(format!(
            "{}: no snapshot (run with UPDATE_GOLDEN=1)",
            path.display()
        ));
        return;
    };
    if expected != actual {
        failures.push(format!(
            "{}\n{}",
            path.display(),
            first_diff(&expected, actual)
        ));
    }
}

/// The first differing line with a little context, which is what a reader needs to judge a drift. The full diff is one `UPDATE_GOLDEN=1` and a `git diff` away, and reproducing a diff engine here would be a second implementation of something the reader already has.
fn first_diff(expected: &str, actual: &str) -> String {
    let (want, got): (Vec<_>, Vec<_>) = (expected.lines().collect(), actual.lines().collect());
    for (i, (w, g)) in want.iter().zip(got.iter()).enumerate() {
        if w != g {
            return format!("  line {}:\n  - {w}\n  + {g}", i + 1);
        }
    }
    format!(
        "  same prefix, different length: {} snapshot lines vs {} generated",
        want.len(),
        got.len()
    )
}

#[test]
fn generated_rust_and_source_maps_match_the_snapshots() {
    let mut failures = Vec::new();
    let mut covered = 0usize;

    for project in PROJECTS {
        let src_dir = workspace_root().join(project.manifest).join("src");
        for (rel, out) in transpile_project(project) {
            let rsx = src_dir.join(&rel).with_extension("rsx");
            let source = std::fs::read_to_string(&rsx).unwrap_or_default();
            let base = golden_dir().join(project.name).join(&rel);
            check(&base, &out.rust_code, &mut failures);
            check(
                &base.with_extension("rs.map"),
                &render_map(&out, &source),
                &mut failures,
            );
            covered += 1;
        }
    }

    assert!(
        covered >= 39,
        "the harness covered only {covered} files — the corpus lost coverage rather than the snapshots passing"
    );
    assert!(
        failures.is_empty(),
        "{} snapshot(s) drifted:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// What `cargo telar check` and the editor both rest on: a span of generated Rust, handed back through [`SourceMap::locate`], names the `.rsx` text it actually came from.
///
/// The snapshots above freeze the map's *contents*; this exercises the *lookup* over the whole corpus. The two fail apart — a refactor can leave every byte of both output and map identical and still break `locate`, and that shows up as a diagnostic underlining the wrong words rather than as a diff.
#[test]
fn every_verbatim_span_locates_back_to_the_text_it_came_from() {
    let mut checked = 0usize;

    for project in PROJECTS {
        let src_dir = workspace_root().join(project.manifest).join("src");
        for (rel, out) in transpile_project(project) {
            let rsx_path = src_dir.join(&rel).with_extension("rsx");
            let rsx = std::fs::read_to_string(&rsx_path).expect("the file just transpiled");
            let map = SourceMap::new(out.source_map.clone(), out.expr_spans.clone());

            for span in &out.expr_spans {
                let gen_end = span.gen_start + span.len;
                let generated = &out.rust_code[span.gen_start as usize..gen_end as usize];
                let located = map
                    .locate(&out.rust_code, span.gen_start, gen_end, &rsx)
                    .unwrap_or_else(|| {
                        panic!("{}: `{generated}` located nowhere", rsx_path.display())
                    });
                let RsxSpan::Exact { start, end } = located else {
                    panic!(
                        "{}: `{generated}` came back as a whole line, losing the column it had",
                        rsx_path.display()
                    );
                };
                assert_eq!(
                    rsx.get(start as usize..end as usize),
                    Some(generated),
                    "{}: generated `{generated}` mapped to rsx[{start}..{end}], which is different text",
                    rsx_path.display()
                );
                checked += 1;
            }

            let lines = rsx.lines().count() as u32;
            for (gen_line, origin) in out.source_map.iter().enumerate() {
                if let Some(rsx_line) = origin {
                    assert!(
                        *rsx_line < lines,
                        "{}: generated line {} claims rsx line {}, past the file's {lines}",
                        rsx_path.display(),
                        gen_line + 1,
                        rsx_line + 1
                    );
                }
            }
        }
    }

    // A floor, not a target: a span is only emitted for text copied through byte-for-byte, so this should climb as more of the grammar passes values through verbatim. It catches the count collapsing instead.
    assert!(
        checked >= 50,
        "only {checked} verbatim spans checked — the corpus stopped producing them, which is itself the regression"
    );
}
