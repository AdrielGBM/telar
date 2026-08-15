//! rustc's diagnostics, re-pointed at the `.rsx` that produced them.
//!
//! A `.rsx` compiles to `<crate>/.telar/build/<rel>.rs` (or `build-hot` under `cargo telar dev`), so every
//! rustc error past the parse stage names a file the author never wrote and a line they never typed. The
//! transpiler writes a per-line source map beside each generated file; this reads it back.
//!
//! Parse errors do not need any of this: the macro reports them as `compile_error!("<file>:<line>: …")`, so
//! the text already names the `.rsx`. It is the errors *after* transpiling — a wrong type, an unresolved
//! name, a component called with the wrong arity — that lose their origin, and those are exactly the ones a
//! person writing `.rsx` hits most.
//!
//! Shared by `cargo telar check` and the `cargo telar dev` rebuild loop, which is the point: the mapping was
//! built, tested and then wired only into the command almost nobody runs, while the loop everybody lives in
//! printed raw paths into `.telar/`.
//!
//! The columns come from the same [`SourceMap::locate`] the editor uses, so the terminal and the editor
//! cannot reach two conclusions about one error. Where it says a column cannot be trusted — a `[view]` line
//! the transpiler rewrote into something else — the frame underlines nothing rather than the wrong thing.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Component, Path, PathBuf};

use telar_transpiler::{RsxSpan, SourceMap};

/// A `help:`/`note:` rustc hung off a diagnostic. Dropping these used to cost the half of a type error that
/// says what to do about it.
pub(crate) struct Note {
    level: String,
    message: String,
}

/// A rustc diagnostic that started life in a `.rsx`.
pub(crate) struct Projected {
    source: PathBuf,
    /// 1-based, for display.
    line: usize,
    /// Character offsets within [`Self::line`] to underline, or `None` when the transpiler rewrote this line
    /// and its columns mean nothing. Characters and not bytes because this is for drawing.
    underline: Option<(usize, usize)>,
    level: String,
    message: String,
    notes: Vec<Note>,
}

/// Everything one cargo invocation had to say.
#[derive(Default)]
pub(crate) struct Report {
    projected: Vec<Projected>,
    /// Diagnostics about hand-written Rust, kept in rustc's own rendering — it is already pointing at a file
    /// the author can open, and re-drawing it would only make it look less like the compiler they know.
    passthrough: Vec<String>,
}

impl Report {
    pub(crate) fn is_empty(&self) -> bool {
        self.projected.is_empty() && self.passthrough.is_empty()
    }

    pub(crate) fn has_errors(&self) -> bool {
        self.projected.iter().any(|p| p.level == "error")
    }

    /// The whole report as text. `color` off strips the ANSI rustc baked into its own renderings, for the
    /// in-window banner, which draws glyphs and would otherwise print the escape sequences.
    pub(crate) fn render(&self, color: bool) -> String {
        let mut out = String::new();
        let mut sources: HashMap<PathBuf, Option<Vec<String>>> = HashMap::new();
        for item in &self.projected {
            let lines = sources
                .entry(item.source.clone())
                .or_insert_with(|| read_lines(&item.source));
            item.render_into(&mut out, lines.as_deref(), color);
        }
        for rendered in &self.passthrough {
            match color {
                true => out.push_str(rendered),
                false => out.push_str(&strip_ansi(rendered)),
            }
        }
        out
    }
}

impl Projected {
    fn render_into(&self, out: &mut String, source_lines: Option<&[String]>, color: bool) {
        let paint = |code: &str, text: &str| match color {
            true => format!("\x1b[{code}m{text}\x1b[0m"),
            false => text.to_string(),
        };
        let level_color = if self.level == "error" {
            "1;31"
        } else {
            "1;33"
        };
        let number = self.line.to_string();
        let gutter = " ".repeat(number.len());

        out.push_str(&paint(level_color, &self.level));
        out.push_str(&paint("1", ": "));
        out.push_str(&paint("1", &self.message));
        out.push('\n');
        out.push_str(&format!(
            "{gutter}{} {}:{}\n",
            paint("1;34", "-->"),
            display(&self.source),
            self.line
        ));
        // Why rustc's own `rendered` is not reused: its `-->` header and its quoted snippet both name the generated file, because that is the only file rustc saw.
        if let Some(text) = source_lines.and_then(|lines| lines.get(self.line - 1)) {
            let bar = paint("1;34", "|");
            out.push_str(&format!("{gutter} {bar}\n"));
            out.push_str(&format!(
                "{} {bar} {}\n",
                paint("1;34", &number),
                text.trim_end()
            ));
            match self.underline {
                Some((start, end)) => out.push_str(&format!(
                    "{gutter} {bar} {}{}\n",
                    " ".repeat(start),
                    paint(level_color, &"^".repeat(end.saturating_sub(start).max(1)))
                )),
                None => out.push_str(&format!("{gutter} {bar}\n")),
            }
        }
        for note in &self.notes {
            // rustc packs whole tables into one `help` (every impl of a trait, say), and without the indent the continuation reads as further diagnostics.
            let mut lines = note.message.lines();
            out.push_str(&format!(
                "{gutter} {} {}: {}\n",
                paint("1;34", "="),
                paint("1", &note.level),
                lines.next().unwrap_or_default()
            ));
            for line in lines {
                out.push_str(&format!("{gutter}     {line}\n"));
            }
        }
        out.push('\n');
    }
}

/// Reads cargo's `--message-format=json` stream, mapping every diagnostic it can onto its `.rsx`.
pub(crate) fn collect(reader: impl BufRead) -> Report {
    let mut origins: HashMap<PathBuf, Option<Origin>> = HashMap::new();
    let mut report = Report::default();

    for line in reader.lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if value.get("reason").and_then(serde_json::Value::as_str) != Some("compiler-message") {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        let level = message
            .get("level")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("error");
        if level != "error" && level != "warning" {
            continue;
        }
        let text = str_field(message, "message");
        let rendered = str_field(message, "rendered");

        let mut mapped_any = false;
        for span in primary_spans(message) {
            let Some(file) = span.get("file_name").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let generated = PathBuf::from(file);
            if !is_generated(&generated) {
                continue;
            }
            let Some(line_start) = span.get("line_start").and_then(serde_json::Value::as_u64)
            else {
                continue;
            };
            let origin = origins
                .entry(generated.clone())
                .or_insert_with(|| Origin::read(&generated));
            let Some(origin) = origin.as_ref() else {
                continue;
            };
            let Some((line, underline)) = origin.locate(span, line_start as u32) else {
                continue;
            };
            report.projected.push(Projected {
                source: origin.rsx_path.clone(),
                line,
                underline,
                level: level.to_string(),
                message: text.clone(),
                notes: notes_of(message),
            });
            mapped_any = true;
        }
        if !mapped_any && !rendered.is_empty() {
            report.passthrough.push(rendered);
        }
    }
    report
}

fn str_field(message: &serde_json::Value, key: &str) -> String {
    message
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The `help`/`note` children, flattened to their text. Nested children are not followed: rustc uses those
/// for suggestion machinery whose value is in the span rendering, which is exactly what does not survive the
/// hop to another file.
fn notes_of(message: &serde_json::Value) -> Vec<Note> {
    message
        .get("children")
        .and_then(serde_json::Value::as_array)
        .map(|children| {
            children
                .iter()
                .filter_map(|child| {
                    let level = child.get("level").and_then(serde_json::Value::as_str)?;
                    if level != "help" && level != "note" {
                        return None;
                    }
                    let message = str_field(child, "message");
                    (!message.is_empty()).then(|| Note {
                        level: level.to_string(),
                        message,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn primary_spans(message: &serde_json::Value) -> Vec<&serde_json::Value> {
    message
        .get("spans")
        .and_then(serde_json::Value::as_array)
        .map(|spans| {
            spans
                .iter()
                .filter(|span| {
                    span.get("is_primary")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `<crate>/.telar/build/<rel>.rs` — or `build-hot`, which a hot-reload build writes instead.
fn is_generated(path: &Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) != Some("rs") {
        return false;
    }
    build_root(path).is_some()
}

/// The index of the `.telar` component and the build-dir name that follows it.
fn build_root(path: &Path) -> Option<usize> {
    let parts: Vec<Component> = path.components().collect();
    parts.iter().enumerate().position(|(i, part)| {
        part.as_os_str() == ".telar"
            && matches!(
                parts.get(i + 1).map(|part| part.as_os_str()),
                Some(next) if next == "build" || next == "build-hot"
            )
    })
}

/// Maps a generated `<crate>/.telar/build/<rel>.rs` back to `<crate>/src/<rel>.rsx`.
fn generated_to_source(generated: &Path) -> Option<PathBuf> {
    let at = build_root(generated)?;
    let parts: Vec<Component> = generated.components().collect();
    let mut source: PathBuf = parts[..at].iter().collect();
    source.push("src");
    for part in &parts[at + 2..] {
        source.push(part.as_os_str());
    }
    Some(source.with_extension("rsx"))
}

/// One generated file, everything needed to place a diagnostic in it, read once. A single broken component
/// usually produces a run of diagnostics, so this is cached for the length of the stream.
struct Origin {
    rsx_path: PathBuf,
    rsx_source: String,
    generated: String,
    map: SourceMap,
}

impl Origin {
    fn read(generated: &Path) -> Option<Self> {
        let rsx_path = generated_to_source(generated)?;
        let mut map_path = generated.as_os_str().to_os_string();
        map_path.push(".map");
        Some(Self {
            rsx_source: std::fs::read_to_string(&rsx_path).ok()?,
            generated: std::fs::read_to_string(generated).ok()?,
            map: SourceMap::from_json(&std::fs::read_to_string(PathBuf::from(map_path)).ok()?)?,
            rsx_path,
        })
    }

    /// Places one rustc span in the `.rsx`: its 1-based line, and the characters to underline when the
    /// columns can be trusted.
    fn locate(
        &self,
        span: &serde_json::Value,
        line_start: u32,
    ) -> Option<(usize, Option<(usize, usize)>)> {
        // rustc counts lines from 1 and the map from 0.
        let by_line = |line: u32| Some((line as usize + 1, None));
        let Some((byte_start, byte_end)) = span_bytes(span) else {
            return by_line((*self.map.lines.get(line_start as usize - 1)?)?);
        };
        // The generated file is read back from disk after rustc compiled it, so an edit in between would
        // leave these offsets pointing at other text entirely. rustc's own line number is the check: if it
        // disagrees with where the offset lands, the columns are not about this file any more.
        if line_of(&self.generated, byte_start) != Some(line_start as usize - 1) {
            return by_line((*self.map.lines.get(line_start as usize - 1)?)?);
        }
        match self
            .map
            .locate(&self.generated, byte_start, byte_end, &self.rsx_source)?
        {
            RsxSpan::Line(line) => by_line(line),
            RsxSpan::Exact { start, end } => {
                let line = line_of(&self.rsx_source, start)?;
                let text = telar_transpiler::nth_line(&self.rsx_source, line)?;
                let from = column_of(&self.rsx_source, start);
                // A span running past this line — a multi-line `if` expression — underlines to its end.
                let to = match line_of(&self.rsx_source, end) == Some(line) {
                    true => column_of(&self.rsx_source, end),
                    false => text.len(),
                };
                // Characters, not bytes: this is measured out in spaces under the quoted line.
                let chars_to = |at: usize| text[..at.min(text.len())].chars().count();
                Some((line + 1, Some((chars_to(from), chars_to(to)))))
            }
        }
    }
}

/// rustc's byte offsets for a span, which are what the source map is written in.
fn span_bytes(span: &serde_json::Value) -> Option<(u32, u32)> {
    let at = |key| span.get(key).and_then(serde_json::Value::as_u64);
    Some((at("byte_start")? as u32, at("byte_end")? as u32))
}

/// The 0-based line containing `offset`, or `None` past the end of `text`.
fn line_of(text: &str, offset: u32) -> Option<usize> {
    let offset = offset as usize;
    (offset <= text.len()).then(|| text[..offset].matches('\n').count())
}

/// The byte offset of `offset` within its own line.
fn column_of(text: &str, offset: u32) -> usize {
    let offset = (offset as usize).min(text.len());
    offset - text[..offset].rfind('\n').map_or(0, |at| at + 1)
}

fn read_lines(source: &Path) -> Option<Vec<String>> {
    std::fs::read_to_string(source)
        .ok()
        .map(|text| text.lines().map(str::to_string).collect())
}

fn display(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

/// Removes SGR escape sequences. rustc's `rendered` carries them because the build asks for `--color=always`
/// — which is what keeps a terminal's diagnostics looking like cargo's own, and what the in-window banner
/// must not be handed, since it draws glyphs rather than interpreting escapes.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        if chars.next() != Some('[') {
            continue;
        }
        for c in chars.by_ref() {
            if c.is_ascii_alphabetic() {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_file_maps_back_to_its_rsx() {
        let generated = PathBuf::from("/w/crates/modules/.telar/build/clock/clock.rs");
        assert!(is_generated(&generated));
        assert_eq!(
            generated_to_source(&generated),
            Some(PathBuf::from("/w/crates/modules/src/clock/clock.rsx"))
        );
    }

    /// A hot-reload build writes to `build-hot`, and its diagnostics need the same treatment.
    #[test]
    fn a_hot_reload_build_dir_maps_too() {
        let generated = PathBuf::from("/w/apps/a/.telar/build-hot/home.rs");
        assert!(is_generated(&generated));
        assert_eq!(
            generated_to_source(&generated),
            Some(PathBuf::from("/w/apps/a/src/home.rsx"))
        );
    }

    #[test]
    fn a_hand_written_rust_file_is_left_alone() {
        assert!(!is_generated(Path::new("/w/crates/ui/src/icon/mod.rs")));
        assert!(!is_generated(Path::new("/w/crates/ui/.telar/other/x.rs")));
    }

    /// The `help:` line is where rustc puts what to actually do, and it used to be read off the wire and
    /// dropped — leaving the half of a type error that says there is a problem without the half that says
    /// what the fix is.
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

    /// Lays out a package the way a hot-reload build leaves one: the `.rsx` the author wrote, the generated
    /// `.rs` nobody wrote, and the map between them. Returns the package root.
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

    /// `bytes` is what rustc puts in every real span; `None` stands for the spans that arrive without one,
    /// which fall back to the line.
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

    /// The whole point. A type error in a `.rsx` used to be reported against `.telar/build-hot/….rs:LINE` —
    /// a file the author never opened, at a line they never typed — for the entire length of a
    /// `cargo telar dev` session, while the mapping that fixes it sat in a command almost nobody runs.
    #[test]
    fn a_type_error_is_reported_against_the_rsx_line_not_the_generated_one() {
        let root = fake_package(
            "err",
            "[logic]\nlet n = signal(0);\n\n[view]\ntext \"{n}\"\n",
            "1\n2\n3\n4\n5\n    let n = signal(0);\n",
            // Generated line 6 came from `.rsx` line 2 (both 0-based in the map).
            SourceMap::new(vec![None, None, None, None, None, Some(1)], vec![]),
        );
        let report = collect(message_line(&root, "error", "mismatched types", 6).as_bytes());
        let text = report.render(false);

        assert!(text.contains("error: mismatched types"), "{text}");
        assert!(text.contains("src/home.rsx:2"), "{text}");
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

    /// Warnings used to be captured into a `String` that only the failure path ever read, so a whole
    /// development session could pass without one ever reaching the terminal. They travel the same route as
    /// errors now, which is what makes them visible on the builds that succeed.
    #[test]
    fn a_warning_is_not_silently_swallowed() {
        let root = fake_package(
            "warn",
            "[logic]\nlet unused = 1;\n\n[view]\ncolumn\n",
            "boilerplate\n    let unused = 1;\n",
            SourceMap::new(vec![None, Some(1)], vec![]),
        );
        let report =
            collect(message_line(&root, "warning", "unused variable: `unused`", 2).as_bytes());

        assert!(!report.is_empty());
        assert!(!report.has_errors(), "a warning does not fail the build");
        let text = report.render(false);
        assert!(text.contains("warning: unused variable"), "{text}");
        assert!(text.contains("src/home.rsx:2"), "{text}");
        std::fs::remove_dir_all(&root).ok();
    }

    /// A diagnostic about hand-written Rust keeps rustc's own rendering, which is already pointing at a file
    /// the author can open.
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

    /// A `.rsx` that cannot be read still reports the diagnostic — losing the quoted line is a worse frame,
    /// not a lost error.
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

    /// A `[logic]` line is transpiled 1:1 under a fixed indent, so rustc's columns come back by subtracting
    /// it — and the frame can point at the identifier rather than at the line it happens to sit on.
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

        let text = collect(
            message_span(&root, "error", "mismatched types", 2, Some((at, at + 5))).as_bytes(),
        )
        .render(false);

        assert!(text.contains("src/home.rsx:2"), "{text}");
        assert!(
            text.contains("    ^^^^^"),
            "the carets sit under `count`, four columns in:\n{text}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// The case that was unreachable until the expression spans reached the sidecar: a verbatim `[view]`
    /// fragment is the same bytes in both files, so an error inside it underlines exactly it. With only the
    /// line map on disk, the CLI could say nothing narrower than the whole `text "{…}"` line.
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

        assert!(text.contains("src/home.rsx:3"), "{text}");
        assert!(
            text.contains(&format!("{}^^^^^^^", " ".repeat(11))),
            "the carets sit under `missing`, past `    text \"{{`:\n{text}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// And the case that has to stay wide. A `[view]` line the transpiler rewrote into something else has no
    /// column correspondence, so underlining *something* would point at text unrelated to the error.
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

        let text = collect(
            message_span(&root, "error", "no method `new`", 2, Some((at, at + 4))).as_bytes(),
        )
        .render(false);

        assert!(text.contains("src/home.rsx:5"), "{text}");
        assert!(
            !text.contains('^'),
            "nothing is underlined when the columns mean nothing:\n{text}"
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
