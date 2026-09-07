//! rustc's diagnostics, re-pointed at the `.rsx` that produced them.
//!
//! A `.rsx` compiles to `<crate>/.telar/build/<rel>.rs` (or `build-hot` under `cargo telar dev`), so every rustc error past the parse stage names a file the author never wrote and a line they never typed. The transpiler writes a per-line source map beside each generated file; this reads it back.
//!
//! Parse errors do not need any of this: the macro reports them as `compile_error!("<file>:<line>: …")`, so the text already names the `.rsx`. It is the errors *after* transpiling — a wrong type, an unresolved name, a component called with the wrong arity — that lose their origin, and those are exactly the ones a person writing `.rsx` hits most.
//!
//! Shared by `cargo telar check` and the `cargo telar dev` rebuild loop, which is the point: the mapping was built, tested and then wired only into the command almost nobody runs, while the loop everybody lives in printed raw paths into `.telar/`.
//!
//! The columns come from the same [`SourceMap::locate`] the editor uses, so the terminal and the editor cannot reach two conclusions about one error. Where it says a column cannot be trusted — a `[view]` line the transpiler rewrote into something else — the frame underlines nothing rather than the wrong thing.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Component, Path, PathBuf};

use telar_transpiler::{RsxSpan, SourceMap};

/// A `help:`/`note:` rustc hung off a diagnostic. Dropping these used to cost the half of a type error that says what to do about it.
pub(crate) struct Note {
    level: String,
    message: String,
}

/// A rustc diagnostic that started life in a `.rsx`.
pub(crate) struct Projected {
    source: PathBuf,
    /// 1-based, for display.
    line: usize,
    /// Character offsets within [`Self::line`] to underline, or `None` when the transpiler rewrote this line and its columns mean nothing. Characters and not bytes because this is for drawing.
    underline: Option<(usize, usize)>,
    level: String,
    message: String,
    notes: Vec<Note>,
}

/// Everything one cargo invocation had to say.
#[derive(Default)]
pub(crate) struct Report {
    projected: Vec<Projected>,
    /// Diagnostics about hand-written Rust, kept in rustc's own rendering — it is already pointing at a file the author can open, and re-drawing it would only make it look less like the compiler they know.
    passthrough: Vec<String>,
}

impl Report {
    pub(crate) fn is_empty(&self) -> bool {
        self.projected.is_empty() && self.passthrough.is_empty()
    }

    pub(crate) fn has_errors(&self) -> bool {
        self.projected.iter().any(|p| p.level == "error")
    }

    /// The whole report as text. `color` off strips the ANSI rustc baked into its own renderings, for the in-window banner, which draws glyphs and would otherwise print the escape sequences.
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
        // rustc's own `rendered` is not reused: its `-->` header and quoted snippet both name the generated file, the only file rustc saw.
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
            // rustc packs whole tables into one `help`, and without the indent the continuation reads as further diagnostics.
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
                notes: sigil_advice(message, notes_of(message)),
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

/// Replaces rustc's advice on a move-out-of-closure error with the one that applies in `.rsx`.
///
/// rustc says "consider cloning the value", which is right for Rust and wrong here: writing `held.clone()` in markup is the bookkeeping the sigil exists to remove, and it clones on every call rather than once per closure. `$held` is the answer — the transpiler emits one clone per capturing closure and leaves the binding usable. The diagnostic already knew *where*; this is what to write.
fn sigil_advice(message: &serde_json::Value, notes: Vec<Note>) -> Vec<Note> {
    let code = message
        .get("code")
        .and_then(|c| c.get("code"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if code != "E0382" {
        return notes;
    }
    let mut notes: Vec<Note> = notes
        .into_iter()
        .filter(|n| !n.message.contains("cloning the value"))
        .collect();
    notes.push(Note {
        level: "help".to_string(),
        message:
            "mark it with the sigil — `$name` — so each closure that captures it gets its own copy"
                .to_string(),
    });
    notes
}

/// The `help`/`note` children, flattened to their text. Nested children are not followed: rustc uses those for suggestion machinery whose value is in the span rendering, which is exactly what does not survive the hop to another file.
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

/// One generated file, everything needed to place a diagnostic in it, read once. A single broken component usually produces a run of diagnostics, so this is cached for the length of the stream.
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

    /// Places one rustc span in the `.rsx`: its 1-based line, and the characters to underline when the columns can be trusted.
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
        // The generated file is read back from disk after rustc compiled it, so an edit in between would leave these offsets pointing at other text. rustc's own line number is the check.
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

/// Removes SGR escape sequences. rustc's `rendered` carries them because the build asks for `--color=always` — which is what keeps a terminal's diagnostics looking like cargo's own, and what the in-window banner must not be handed, since it draws glyphs rather than interpreting escapes.
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
#[path = "diagnostics_test.rs"]
mod tests;
