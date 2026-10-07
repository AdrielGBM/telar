use super::*;

/// The `help:` line is where rustc puts what to actually do, and it used to be read off the wire and dropped — leaving the half of a type error that says there is a problem without the half that says what the fix is.
#[test]
fn help_and_note_children_survive_the_remap() {
    let message = serde_json::json!({
        "level": "error",
        "message": "mismatched types",
        "children": [
            { "level": "help", "message": "consider borrowing here" },
            { "level": "note", "message": "expected `&str`, found `String`" },
            { "level": "error", "message": "not a child worth repeating" },
        ],
    });
    let notes = notes_of(&message);
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].level, "help");
    assert_eq!(notes[1].message, "expected `&str`, found `String`");
}

#[test]
fn ansi_is_stripped_for_the_in_window_banner() {
    assert_eq!(
        strip_ansi("\x1b[1;31merror\x1b[0m: mismatched types"),
        "error: mismatched types"
    );
}

/// Whether the report points at `suffix`, written with `/` whatever the platform spells it with. A frame carries a real mapped path, so on Windows it arrives with backslashes and a literal `src/home.rsx` never matches — which is a fact about the separator and not about the mapping under test.
fn points_at(text: &str, suffix: &str) -> bool {
    text.replace('\\', "/").contains(suffix)
}

/// Lays out a package the way a hot-reload build leaves one: the `.rsx` the author wrote, the generated `.rs` nobody wrote, and the map between them. Returns the package root.
fn fake_package(tag: &str, rsx: &str, generated: &str, map: SourceMap) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_diag_{tag}_{}", std::process::id()));
    let build = root.join(".telar/build-hot");
    std::fs::create_dir_all(&build).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/home.rsx"), rsx).unwrap();
    std::fs::write(build.join("home.rs"), generated).unwrap();
    std::fs::write(build.join("home.rs.map"), map.to_json()).unwrap();
    root
}

fn message_line(root: &Path, level: &str, message: &str, line: u64) -> String {
    message_span(root, level, message, line, None)
}

/// `bytes` is what rustc puts in every real span; `None` stands for the spans that arrive without one, which fall back to the line.
fn message_span(
    root: &Path,
    level: &str,
    message: &str,
    line: u64,
    bytes: Option<(u32, u32)>,
) -> String {
    let mut span = serde_json::json!({
        "file_name": root.join(".telar/build-hot/home.rs").to_str().unwrap(),
        "line_start": line,
        "is_primary": true,
    });
    if let Some((start, end)) = bytes {
        span["byte_start"] = start.into();
        span["byte_end"] = end.into();
    }
    serde_json::json!({
        "reason": "compiler-message",
        "message": {
            "level": level,
            "message": message,
            "rendered": format!("{level}: {message}\n --> {}/.telar/build-hot/home.rs:{line}\n", root.display()),
            "spans": [span],
        },
    })
    .to_string()
}

/// The whole point. A type error in a `.rsx` used to be reported against `.telar/build-hot/….rs:LINE` — a file the author never opened, at a line they never typed — for the entire length of a `cargo telar dev` session, while the mapping that fixes it sat in a command almost nobody runs.
#[test]
fn a_type_error_is_reported_against_the_rsx_line_not_the_generated_one() {
    let root = fake_package(
        "err",
        "[logic]\nlet n = signal(0);\n\n[view]\ntext \"{n}\"\n",
        "1\n2\n3\n4\n5\n    let n = signal(0);\n",
        SourceMap::new(vec![None, None, None, None, None, Some(1)], vec![]),
    );
    let report = collect(message_line(&root, "error", "mismatched types", 6).as_bytes());
    let text = report.render(false);

    assert!(text.contains("error: mismatched types"), "{text}");
    assert!(points_at(&text, "src/home.rsx:2"), "{text}");
    assert!(
        text.contains("let n = signal(0);"),
        "the frame quotes the line the author wrote:\n{text}"
    );
    assert!(
        !text.contains(".telar"),
        "and nothing points into the build directory:\n{text}"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// Warnings used to be captured into a `String` that only the failure path ever read, so a whole development session could pass without one ever reaching the terminal. They travel the same route as errors now, which is what makes them visible on the builds that succeed.
#[test]
fn a_warning_is_not_silently_swallowed() {
    let root = fake_package(
        "warn",
        "[logic]\nlet unused = 1;\n\n[view]\ncolumn\n",
        "boilerplate\n    let unused = 1;\n",
        SourceMap::new(vec![None, Some(1)], vec![]),
    );
    let report = collect(message_line(&root, "warning", "unused variable: `unused`", 2).as_bytes());

    assert!(
        !report.is_empty(),
        "a warning must reach the report, not be swallowed"
    );
    assert!(!report.has_errors(), "a warning does not fail the build");
    let text = report.render(false);
    assert!(text.contains("warning: unused variable"), "{text}");
    assert!(points_at(&text, "src/home.rsx:2"), "{text}");
    std::fs::remove_dir_all(&root).ok();
}

/// A diagnostic about hand-written Rust keeps rustc's own rendering, which is already pointing at a file the author can open.
#[test]
fn a_diagnostic_about_hand_written_rust_is_passed_through_untouched() {
    let message = serde_json::json!({
        "reason": "compiler-message",
        "message": {
            "level": "error",
            "message": "cannot find value `x`",
            "rendered": "error: cannot find value `x`\n --> src/state.rs:9\n",
            "spans": [{ "file_name": "src/state.rs", "line_start": 9, "is_primary": true }],
        },
    })
    .to_string();
    let text = collect(message.as_bytes()).render(false);
    assert_eq!(text, "error: cannot find value `x`\n --> src/state.rs:9\n");
}

/// A `.rsx` that cannot be read still reports the diagnostic — losing the quoted line is a worse frame, not a lost error.
#[test]
fn a_missing_source_file_still_reports_the_diagnostic() {
    let report = Report {
        projected: vec![Projected {
            source: PathBuf::from("/nowhere/home.rsx"),
            line: 4,
            underline: None,
            level: "error".to_string(),
            message: "mismatched types".to_string(),
            notes: vec![],
        }],
        passthrough: vec![],
    };
    let text = report.render(false);
    assert!(text.contains("error: mismatched types"), "{text}");
    assert!(text.contains("/nowhere/home.rsx:4"), "{text}");
}

/// A `[logic]` line is transpiled 1:1 under a fixed indent, so rustc's columns come back by subtracting it — and the frame can point at the identifier rather than at the line it happens to sit on.
#[test]
fn a_logic_error_underlines_the_columns_rustc_gave_it() {
    let generated = "fn home() {\n    let count = signal(0);\n}\n";
    let at = generated.find("count").unwrap() as u32;
    let root = fake_package(
        "cols",
        "[logic]\nlet count = signal(0);\n\n[view]\ncolumn\n",
        generated,
        SourceMap::new(vec![None, Some(1), None], vec![]),
    );

    let text =
        collect(message_span(&root, "error", "mismatched types", 2, Some((at, at + 5))).as_bytes())
            .render(false);

    assert!(points_at(&text, "src/home.rsx:2"), "{text}");
    assert!(
        text.contains("    ^^^^^"),
        "the carets sit under `count`, four columns in:\n{text}"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// The case that was unreachable until the expression spans reached the sidecar: a verbatim `[view]` fragment is the same bytes in both files, so an error inside it underlines exactly it. With only the line map on disk, the CLI could say nothing narrower than the whole `text "{…}"` line.
#[test]
fn a_verbatim_view_expression_is_underlined_exactly() {
    let rsx = "[view]\ncolumn\n    text \"{missing}\"\n";
    let generated = "fn home() {\n    text(format!(\"{}\", missing))\n}\n";
    let gen_at = generated.find("missing").unwrap() as u32;
    let root = fake_package(
        "verbatim",
        rsx,
        generated,
        SourceMap::new(
            vec![None, Some(2), None],
            vec![telar_transpiler::ExprSpan {
                rsx_start: rsx.find("missing").unwrap() as u32,
                len: 7,
                gen_start: gen_at,
            }],
        ),
    );

    let text = collect(
        message_span(
            &root,
            "error",
            "cannot find value `missing`",
            2,
            Some((gen_at, gen_at + 7)),
        )
        .as_bytes(),
    )
    .render(false);

    assert!(points_at(&text, "src/home.rsx:3"), "{text}");
    assert!(
        text.contains(&format!("{}^^^^^^^", " ".repeat(11))),
        "the carets sit under `missing`, past `    text \"{{`:\n{text}"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// And the case that has to stay wide. A `[view]` line the transpiler rewrote into something else has no column correspondence, so underlining *something* would point at text unrelated to the error.
#[test]
fn a_rewritten_view_line_is_not_underlined_at_all() {
    let generated = "fn home() {\n    Text::new(\"hi\")\n}\n";
    let at = generated.find("Text").unwrap() as u32;
    let root = fake_package(
        "wide",
        "[logic]\nlet x = 1;\n\n[view]\ntext \"hi\"\n",
        generated,
        SourceMap::new(vec![None, Some(4), None], vec![]),
    );

    let text =
        collect(message_span(&root, "error", "no method `new`", 2, Some((at, at + 4))).as_bytes())
            .render(false);

    assert!(points_at(&text, "src/home.rsx:5"), "{text}");
    assert!(
        !text.contains('^'),
        "nothing is underlined when the columns mean nothing:\n{text}"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// A package as `cargo telar` leaves one after a hot build: its manifests, the `.rsx`, and the transpiler's real output and map for it — so the fixtures below point at the Rust the emitter actually writes.
fn transpiled_package(tag: &str, cargo: &str, telar: &str, rsx: &str) -> (PathBuf, String) {
    let root = std::env::temp_dir().join(format!("telar_diag_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("Cargo.toml"), cargo).unwrap();
    std::fs::write(root.join("telar.toml"), telar).unwrap();
    let prelude = telar_project::resolve_prelude(&root).unwrap();
    let out = telar_transpiler::transpile_buffer(
        &root.join("src/home.rsx"),
        rsx,
        &telar_transpiler::PackageOptions {
            src_dir: &root.join("src"),
            theme_type: None,
            assets: None,
            prelude: &prelude,
            flavour: telar_project::BuildFlavour::Plain,
        },
    )
    .unwrap();
    let package = fake_package(
        tag,
        rsx,
        &out.rust_code,
        SourceMap::new(out.source_map, out.expr_spans),
    );
    assert_eq!(package, root);
    (root, out.rust_code)
}

/// One rustc `compiler-message`, shaped as rustc 1.99 writes it, whose primary span covers the first `needle` in the generated file — without a trailing `(`, which only picks the call out of the names around it.
fn rustc_error(
    root: &Path,
    generated: &str,
    code: &str,
    message: &str,
    needle: &str,
    children: serde_json::Value,
) -> String {
    let start = generated
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is not in:\n{generated}"));
    let line = generated[..start].matches('\n').count() + 1;
    serde_json::json!({
        "reason": "compiler-message",
        "message": {
            "level": "error",
            "message": message,
            "code": { "code": code, "explanation": null },
            "rendered": format!("error[{code}]: {message}\n --> .telar/build-hot/home.rs:{line}\n"),
            "spans": [{
                "file_name": root.join(".telar/build-hot/home.rs").to_str().unwrap(),
                "byte_start": start,
                "byte_end": start + needle.trim_end_matches('(').len(),
                "line_start": line,
                "is_primary": true,
            }],
            "children": children,
        },
    })
    .to_string()
}

/// A help child carrying rustc's one suggested replacement.
fn suggestion(message: &str, replacement: &str) -> serde_json::Value {
    serde_json::json!({
        "level": "help",
        "message": message,
        "spans": [{ "suggested_replacement": replacement, "is_primary": true }],
        "children": [],
    })
}

/// The `could refer to … imported here` note rustc hangs off an E0659, underlining the glob import `path::*` on its line.
fn glob_note(message: &str, path: &str) -> serde_json::Value {
    let line = format!("#[allow(unused_imports)] use {path}::*;");
    let at = line.find(&format!("{path}::*")).unwrap() + 1;
    serde_json::json!({
        "level": "note",
        "message": message,
        "spans": [{
            "is_primary": true,
            "text": [{
                "text": line,
                "highlight_start": at,
                "highlight_end": at + path.len() + 3,
            }],
        }],
        "children": [],
    })
}

const DEPENDS_ON_COMPONENTS: &str =
    "[package]\nname = \"app\"\n\n[dependencies]\ntelar = \"0.2\"\ntelar-components = \"0.2\"\n";

/// An unknown tag reaches rustc as two unresolved names, the function and its `Props`, in a file the author never wrote. It is one mistake in the markup, so it is one error there, said in the markup's terms and underlining the tag.
#[test]
fn an_unknown_tag_is_one_error_on_the_tag() {
    let (root, generated) = transpiled_package(
        "unknown_tag",
        DEPENDS_ON_COMPONENTS,
        "[telar]\nprelude = [\"telar-components\"]\n",
        "[view]\ncolumn\n    buton label:\"Go\"\n",
    );
    let stream = [
        rustc_error(
            &root,
            &generated,
            "E0433",
            "cannot find type `ButonProps` in this scope",
            "ButonProps",
            serde_json::json!([suggestion(
                "a struct with a similar name exists",
                "ButtonProps"
            )]),
        ),
        rustc_error(
            &root,
            &generated,
            "E0425",
            "cannot find function `buton` in this scope",
            "buton(",
            serde_json::json!([suggestion(
                "a function with a similar name exists",
                "button"
            )]),
        ),
    ]
    .join("\n");

    let text = collect(stream.as_bytes()).render(false);
    std::fs::remove_dir_all(&root).ok();

    assert_eq!(
        text.matches("unknown tag").count(),
        1,
        "two rustc errors, one mistake:\n{text}"
    );
    assert!(
        text.contains("error: unknown tag `buton`: not a built-in, not a `.rsx` in this package, and not exported by any `[telar] prelude` entry (`telar_components`)"),
        "{text}"
    );
    assert!(points_at(&text, "src/home.rsx:3"), "{text}");
    assert!(
        text.contains("\n  |     ^^^^^\n"),
        "the tag is underlined:\n{text}"
    );
    assert!(
        text.contains("help: a function with a similar name exists: `button`"),
        "{text}"
    );
    assert!(
        !text.contains("ButtonProps") && !text.contains(".telar") && !text.contains("cannot find"),
        "nothing about the Rust the tag became:\n{text}"
    );
}

/// Two prelude crates exporting one tag is E0659 twice — on the function and on its `Props` — pointing at glob imports the author never wrote.
#[test]
fn a_tag_two_crates_export_names_both_and_the_use_that_settles_it() {
    let (root, generated) = transpiled_package(
        "clash",
        DEPENDS_ON_COMPONENTS,
        "[telar]\nprelude = [\"telar-components\"]\n",
        "[view]\nbutton\n",
    );
    let ambiguous = |name: &str, kind: &str, needle: &str| {
        rustc_error(
            &root,
            &generated,
            "E0659",
            &format!("`{name}` is ambiguous"),
            needle,
            serde_json::json!([
                { "level": "note", "message": "ambiguous because of multiple glob imports of a name in the same module", "spans": [], "children": [] },
                glob_note(&format!("`{name}` could refer to the {kind} imported here"), "telar"),
                { "level": "help", "message": format!("consider adding an explicit import of `{name}` to disambiguate"), "spans": [], "children": [] },
                glob_note(&format!("`{name}` could also refer to the {kind} imported here"), "telar_components"),
                { "level": "help", "message": format!("consider adding an explicit import of `{name}` to disambiguate"), "spans": [], "children": [] },
            ]),
        )
    };
    let stream = [
        ambiguous("button", "function", "button("),
        ambiguous("ButtonProps", "struct", "ButtonProps"),
    ]
    .join("\n");

    let text = collect(stream.as_bytes()).render(false);
    std::fs::remove_dir_all(&root).ok();

    assert_eq!(text.matches("error:").count(), 1, "{text}");
    assert!(
        text.contains("error: tag `button` is exported by both `telar` and `telar_components`; pick one with `use telar::{button, ButtonProps};` in `[logic]`"),
        "{text}"
    );
    assert!(points_at(&text, "src/home.rsx:2"), "{text}");
    assert!(
        !text.contains("could refer to") && !text.contains("explicit import"),
        "rustc's notes point at generated imports:\n{text}"
    );
}

/// A prelude crate the package does not depend on fails the glob import in every generated file. The check reads `Cargo.toml` and says so on the `telar.toml` line; rustc's copies of it land on the same line in the same words, so there is one error.
#[test]
fn a_prelude_crate_that_is_not_a_dependency_is_one_error_on_its_telar_toml_line() {
    let (root, generated) = transpiled_package(
        "undeclared",
        "[package]\nname = \"app\"\n\n[dependencies]\ntelar = \"0.2\"\n",
        "[telar]\nprelude = [\"telar-components\"]\n",
        "[view]\ntext \"hi\"\n",
    );
    let stream = rustc_error(
        &root,
        &generated,
        "E0432",
        "unresolved import `telar_components`",
        "telar_components",
        serde_json::json!([{ "level": "help", "message": "you might be missing a crate named `telar_components`", "spans": [], "children": [] }]),
    );

    let mut report = collect(stream.as_bytes());
    report.add_prelude_problems(&telar_project::prelude_problems(&root).unwrap());
    let text = report.render(false);
    std::fs::remove_dir_all(&root).ok();

    assert_eq!(text.matches("error:").count(), 1, "{text}");
    assert!(
        text.contains("error: `[telar] prelude` entry `telar_components` names `telar_components`, which is not a dependency of `app`"),
        "{text}"
    );
    assert!(points_at(&text, "telar.toml:2"), "{text}");
    assert!(
        text.contains("\n  |            ^^^^^^^^^^^^^^^^^^\n"),
        "the entry is underlined:\n{text}"
    );
    assert!(text.contains("cargo add -p app telar-components"), "{text}");
    assert!(!text.contains(".telar/"), "{text}");
}

/// A crate the package does depend on, and a path inside it that does not exist: only rustc can tell, and its answer still belongs on the key.
#[test]
fn a_prelude_path_that_does_not_resolve_is_reported_on_its_telar_toml_line() {
    let (root, generated) = transpiled_package(
        "unresolved_path",
        DEPENDS_ON_COMPONENTS,
        "[telar]\nprelude = [\"telar_components::prelud\"]\n",
        "[view]\ntext \"hi\"\n",
    );
    let stream = rustc_error(
        &root,
        &generated,
        "E0432",
        "unresolved import `telar_components::prelud`",
        "prelud::*",
        serde_json::json!([]),
    );

    let text = collect(stream.as_bytes()).render(false);
    std::fs::remove_dir_all(&root).ok();

    assert!(
        text.contains("error: `[telar] prelude` entry `telar_components::prelud` does not resolve: unresolved import `telar_components::prelud`"),
        "{text}"
    );
    assert!(points_at(&text, "telar.toml:2"), "{text}");
}
