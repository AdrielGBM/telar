//! The safety net: every `.rsx` in the workspace transpiled into a committed snapshot of the generated Rust *and* of its source map, so a refactor of the transpiler has to declare what it changed.
//!
//! The map is snapshotted beside the code because a refactor can leave the output byte-identical and still ruin the column mapping — and a wrong column is a diagnostic pointing at the wrong text, which is worse than none. Snapshotting only the Rust would let that through silently.
//!
//! Run with `UPDATE_GOLDEN=1` to rewrite the snapshots after a deliberate change, then read the diff.

// The corpus is transpiled, not merely placed, so there is nothing here to check in a build that carries no transpiler.
#![cfg(feature = "transpile")]

use std::path::{Path, PathBuf};

use telar_transpiler::{GeneratedFile, RsxSpan, SourceMap, TranspiledSource};

/// One package whose `src/` tree the harness transpiles. The theme is not listed: it used to be, copied out of each `app!` invocation by hand, and a copy of the one thing that decides whether `use_theme::<T>()` is typed at all is a copy that can be wrong while every snapshot passes. It is resolved the same way the editor resolves it now.
struct Project {
    name: &'static str,
    manifest: &'static str,
}

const PROJECTS: &[Project] = &[
    Project {
        name: "sandbox",
        manifest: "apps/sandbox",
    },
    Project {
        name: "landing",
        manifest: "apps/landing",
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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("golden")
}

/// Transpiles one package exactly the way `app!` does — the same walk, the same component names, the same baked artifact, the same theme — because it calls the same function the macro calls. Nothing is written: the snapshots are compared against what a build *would* produce, not against what one left behind.
fn transpile_project(project: &Project) -> Vec<GeneratedFile> {
    let manifest = workspace_root().join(project.manifest);
    let src_dir = manifest.join("src");
    let assets = telar_project::AssetContext::load(&manifest, env!("CARGO_PKG_VERSION"));
    // Without it every `src:"…"` snapshots as a `compile_error!` telling you to bake — a diff that says nothing about the transpiler, over a snapshot that would then be wrong for everyone who did bake.
    assert!(
        assets.index().is_some(),
        "{} has no usable baked asset artifact — run `cargo run -p cargo-telar -- bake` first",
        project.name
    );
    let theme_type = telar_transpiler::resolve_theme_type(&manifest);
    assert!(
        theme_type.is_some(),
        "{} names no theme — `use_theme::<T>()` would snapshot untyped, which is not what its build compiles",
        project.name
    );

    let files = telar_transpiler::transpile_package(&telar_transpiler::PackageOptions {
        src_dir: &src_dir,
        theme_type: theme_type.as_deref(),
        assets: Some(&assets),
        // `Preview`, not `Plain`: it is the richer of the two shapes — the same Rust plus a build fn per `[preview]` — so snapshotting it keeps preview codegen covered. Pinning `Plain` would drop every preview from the corpus and stop asserting anything about how one is generated.
        flavour: telar_project::BuildFlavour::Preview,
    })
    .unwrap_or_else(|e| panic!("{} failed to transpile: {e}", project.name));
    assert!(
        !files.is_empty(),
        "{} has no .rsx files — the harness would pass by covering nothing",
        project.name
    );
    files
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

/// The shape of the generated code, paired with the number that declares which `telar` can compile it.
///
/// [`telar_project::BUILD_ARTIFACT_FORMAT`] is what lets one installed CLI serve projects pinning different `telar` versions: same format means compatible, whatever the versions say. It is *declared*, not derived — a digest cannot tell a cosmetic rewrite from one that reaches for an API an older `telar` never had, and deriving it from the output would refuse both alike, which is the one failure a matching CLI has to be installed to clear.
///
/// So the digest does not decide; it makes the decision unskippable. Any change to the corpus lands here beside the current format, and updating this file is where someone has to answer whether the format moves with it.
fn render_shape(corpus: &str) -> String {
    format!(
        "# The generated code changed if this digest did. When it does, decide whether an older `telar` can still compile the new shape: if it cannot, bump BUILD_ARTIFACT_FORMAT before updating this.\nBUILD_ARTIFACT_FORMAT = {}\ncorpus digest = {}\n",
        telar_project::BUILD_ARTIFACT_FORMAT,
        telar_project::content_hash(corpus.as_bytes())
    )
}

#[test]
fn generated_rust_and_source_maps_match_the_snapshots() {
    let mut failures = Vec::new();
    let mut covered = 0usize;
    let mut corpus = String::new();

    for project in PROJECTS {
        for file in transpile_project(project) {
            let source = std::fs::read_to_string(&file.rsx_path).unwrap_or_default();
            let base = golden_dir().join(project.name).join(&file.rel_out);
            check(&base, &file.source.rust_code, &mut failures);
            check(
                &base.with_extension("rs.map"),
                &render_map(&file.source, &source),
                &mut failures,
            );
            corpus.push_str(&file.source.rust_code);
            covered += 1;
        }
    }
    check(
        &golden_dir().join("shape.txt"),
        &render_shape(&corpus),
        &mut failures,
    );

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
        for file in transpile_project(project) {
            let rsx_path = file.rsx_path;
            let out = file.source;
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
