use super::*;

#[test]
fn a_generated_file_maps_back_to_its_rsx() {
    let generated = PathBuf::from("/w/crates/modules/.telar/build/clock/clock.rs");
    assert!(is_generated(&generated), "{}", generated.display());
    assert_eq!(
        generated_to_source(&generated),
        Some(PathBuf::from("/w/crates/modules/src/clock/clock.rsx"))
    );
}

/// A hot-reload build writes to `build-hot`, and its diagnostics need the same treatment.
#[test]
fn a_hot_reload_build_dir_maps_too() {
    let generated = PathBuf::from("/w/apps/a/.telar/build-hot/home.rs");
    assert!(is_generated(&generated), "{}", generated.display());
    assert_eq!(
        generated_to_source(&generated),
        Some(PathBuf::from("/w/apps/a/src/home.rsx"))
    );
}

#[test]
fn a_hand_written_rust_file_is_left_alone() {
    assert!(
        !is_generated(Path::new("/w/crates/ui/src/icon/mod.rs")),
        "a hand-written module is not generated"
    );
    assert!(
        !is_generated(Path::new("/w/crates/ui/.telar/other/x.rs")),
        "and neither is an unrelated file under .telar"
    );
}

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
