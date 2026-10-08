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
use std::path::{Path, PathBuf};

use telar_project::{
    EntryProblem, PreludeDeclaration, PreludeEntry, PreludeProblem, PreviewsIncludeProblem,
};
use telar_transpiler::{GeneratedSite, RsxSpan, SourceMap};

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
    /// Adds the `.rsx` semantic checks for one file — a style class nobody declared, an i18n key the catalogue does not hold.
    ///
    /// They arrive here rather than in a report of their own so the terminal draws one frame per diagnostic whatever found it, and so `has_errors` counts them: an error the editor shows and the build ignores is the disagreement this exists to end. No underline, because the check spans the whole line.
    pub(crate) fn add_semantic(
        &mut self,
        source: &std::path::Path,
        diagnostics: Vec<telar_diagnostics::Diagnostic>,
    ) {
        self.projected
            .extend(diagnostics.into_iter().map(|d| Projected {
                source: source.to_path_buf(),
                line: d.span.line,
                underline: None,
                level: match d.severity {
                    telar_diagnostics::Severity::Error => "error".to_string(),
                    telar_diagnostics::Severity::Warning => "warning".to_string(),
                },
                message: d.message,
                notes: Vec::new(),
            }));
    }

    /// Adds `item`, or folds its notes into an identical frame already here.
    ///
    /// One mistake in the markup is often several in the Rust it became — an unknown tag is an unresolved function and an unresolved `Props`, a missing prelude crate an unresolved import in every generated file — and once each is said about the `.rsx` they are the same sentence on the same line.
    fn push(&mut self, item: Projected) {
        let existing = self.projected.iter_mut().find(|p| {
            p.source == item.source
                && p.line == item.line
                && p.underline == item.underline
                && p.level == item.level
                && p.message == item.message
        });
        match existing {
            Some(existing) => {
                for note in item.notes {
                    if !existing
                        .notes
                        .iter()
                        .any(|n| n.level == note.level && n.message == note.message)
                    {
                        existing.notes.push(note);
                    }
                }
            }
            None => self.projected.push(item),
        }
    }

    /// Adds the `[telar] prelude` entries a package cannot reach, on the `telar.toml` lines that declare them.
    pub(crate) fn add_prelude_problems(&mut self, problems: &[PreludeProblem]) {
        for problem in problems {
            self.push(problem_frame(problem));
        }
    }

    /// Adds the `[telar.previews] include` entries a package cannot reach, on the `telar.toml` lines that declare them.
    pub(crate) fn add_previews_include_problems(&mut self, problems: &[PreviewsIncludeProblem]) {
        for problem in problems {
            self.push(problem_frame(problem));
        }
    }

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
    let mut preludes: HashMap<PathBuf, PackagePrelude> = HashMap::new();
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
            if !telar_project::is_generated_output(&generated) {
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
            let prelude = preludes
                .entry(origin.package_root.clone())
                .or_insert_with(|| PackagePrelude::read(&origin.package_root));
            let site = origin.site(span, line_start as u32);
            if let Some(GeneratedSite::Glob(path)) = &site {
                if let Some(projected) = prelude.unresolved(path, level, &text) {
                    report.push(projected);
                    mapped_any = true;
                }
                continue;
            }
            let Some((line, underline)) = origin.locate(span, line_start as u32) else {
                continue;
            };
            let code = error_code(message);
            let rewritten = site.as_ref().and_then(|site| {
                let candidates = glob_candidates(message);
                let rewritten =
                    telar_transpiler::tag_error_message(code, site, &prelude.entries, &candidates)?;
                Some((site, rewritten))
            });
            let projected = match rewritten {
                Some((GeneratedSite::Tag { tag, name }, rewritten)) => Projected {
                    source: origin.rsx_path.clone(),
                    line,
                    underline: origin.tag_underline(line, tag).or(underline),
                    level: level.to_string(),
                    message: rewritten,
                    notes: match code != "E0659" && tag_names(tag, name) {
                        true => notes_of(message),
                        false => Vec::new(),
                    },
                },
                _ => Projected {
                    source: origin.rsx_path.clone(),
                    line,
                    underline,
                    level: level.to_string(),
                    message: text.clone(),
                    notes: sigil_advice(message, notes_of(message)),
                },
            };
            report.push(projected);
            mapped_any = true;
        }
        if !mapped_any && !rendered.is_empty() {
            report.passthrough.push(rendered);
        }
    }
    report
}

fn error_code(message: &serde_json::Value) -> &str {
    message
        .get("code")
        .and_then(|c| c.get("code"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

/// Whether `name` is the tag's own name rather than its `Props` type or a module on its path. rustc's help at the other two is about a struct or a module the author never wrote.
fn tag_names(tag: &str, name: &str) -> bool {
    tag.rsplit("::").next() == Some(name)
}

/// The glob imports an ambiguity (E0659) is between, in rustc's order: the path each `could refer to … imported here` note underlines, without its `::*`.
fn glob_candidates(message: &serde_json::Value) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    let children = message
        .get("children")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten();
    for span in children
        .filter_map(|child| child.get("spans").and_then(serde_json::Value::as_array))
        .flatten()
    {
        let Some(highlighted) = highlighted_text(span) else {
            continue;
        };
        if let Some(path) = highlighted.strip_suffix("::*")
            && !candidates.iter().any(|known| known == path)
        {
            candidates.push(path.to_string());
        }
    }
    candidates
}

/// The text a span underlines on its first line. rustc's highlight columns are 1-based and count characters.
fn highlighted_text(span: &serde_json::Value) -> Option<String> {
    let first = span.get("text")?.as_array()?.first()?;
    let line = first.get("text")?.as_str()?;
    let from = first.get("highlight_start")?.as_u64()? as usize;
    let to = first.get("highlight_end")?.as_u64()? as usize;
    Some(
        line.chars()
            .skip(from.checked_sub(1)?)
            .take(to.checked_sub(from)?)
            .collect(),
    )
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
    if error_code(message) != "E0382" {
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
///
/// A help whose whole point is one replacement — "a function with a similar name exists" — names it, since the span that carried the name is the part that does not survive.
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
                    let mut message = str_field(child, "message");
                    if message.is_empty() {
                        return None;
                    }
                    if let Some(replacement) = single_replacement(child) {
                        message.push_str(&format!(": `{replacement}`"));
                    }
                    Some(Note {
                        level: level.to_string(),
                        message,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The one replacement a child suggests, when it suggests exactly one and it fits on a line.
fn single_replacement(child: &serde_json::Value) -> Option<String> {
    let mut replacements = child
        .get("spans")?
        .as_array()?
        .iter()
        .filter_map(|span| span.get("suggested_replacement")?.as_str())
        .map(str::trim);
    let first = replacements.next()?;
    let single = replacements.all(|other| other == first);
    (single && !first.is_empty() && !first.contains('\n')).then(|| first.to_string())
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

/// One generated file, everything needed to place a diagnostic in it, read once. A single broken component usually produces a run of diagnostics, so this is cached for the length of the stream.
struct Origin {
    rsx_path: PathBuf,
    package_root: PathBuf,
    rsx_source: String,
    generated: String,
    map: SourceMap,
}

impl Origin {
    fn read(generated: &Path) -> Option<Self> {
        let rsx_path = telar_project::source_for_generated(generated)?;
        let mut map_path = generated.as_os_str().to_os_string();
        map_path.push(".map");
        Some(Self {
            rsx_source: std::fs::read_to_string(&rsx_path).ok()?,
            generated: std::fs::read_to_string(generated).ok()?,
            map: SourceMap::from_json(&std::fs::read_to_string(PathBuf::from(map_path)).ok()?)?,
            package_root: generated
                .ancestors()
                .find(|dir| dir.file_name() == Some(".telar".as_ref()))?
                .parent()?
                .to_path_buf(),
            rsx_path,
        })
    }

    /// What the transpiler wrote the span's generated text for, when the file on disk is still the one rustc compiled.
    fn site(&self, span: &serde_json::Value, line_start: u32) -> Option<GeneratedSite<'_>> {
        let (start, end) = span_bytes(span)?;
        if line_of(&self.generated, start) != Some(line_start as usize - 1) {
            return None;
        }
        telar_transpiler::generated_site(&self.generated, start as usize, end as usize)
    }

    /// The characters `tag` covers on 1-based `line` of the `.rsx`.
    fn tag_underline(&self, line: usize, tag: &str) -> Option<(usize, usize)> {
        let text = telar_transpiler::nth_line(&self.rsx_source, line.checked_sub(1)?)?;
        let (from, to) = telar_transpiler::tag_columns(text, tag)?;
        Some((text[..from].chars().count(), text[..to].chars().count()))
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

/// One package's `[telar] prelude` as the error mapping needs it, read once per stream.
struct PackagePrelude {
    entries: Vec<PreludeEntry>,
    declarations: Vec<PreludeDeclaration>,
    problems: Vec<PreludeProblem>,
}

impl PackagePrelude {
    /// An unreadable `telar.toml` reads as an empty prelude: the transpile that produced these files has already reported it, and the tags still deserve their own errors.
    fn read(package_root: &Path) -> Self {
        let declarations = telar_project::prelude_declarations(package_root).unwrap_or_default();
        Self {
            entries: declarations.iter().map(|d| d.entry.clone()).collect(),
            problems: telar_project::prelude_problems(package_root).unwrap_or_default(),
            declarations,
        }
    }

    /// rustc failing on the glob import of `path`, said on the `telar.toml` line that asked for it. `None` for a glob no prelude entry wrote — `telar`'s or the crate's own.
    fn unresolved(&self, path: &str, level: &str, rustc_message: &str) -> Option<Projected> {
        if let Some(problem) = self
            .problems
            .iter()
            .find(|problem| problem.declaration.entry.path() == path)
        {
            return Some(problem_frame(problem));
        }
        let declaration = self
            .declarations
            .iter()
            .find(|declaration| declaration.entry.path() == path)?;
        Some(Projected {
            source: declaration.file.clone(),
            line: declaration.line,
            underline: Some(declaration.columns),
            level: level.to_string(),
            message: format!("`[telar] prelude` entry `{path}` does not resolve: {rustc_message}"),
            notes: Vec::new(),
        })
    }
}

fn problem_frame<T>(problem: &EntryProblem<T>) -> Projected {
    Projected {
        source: problem.declaration.file.clone(),
        line: problem.declaration.line,
        underline: Some(problem.declaration.columns),
        level: "error".to_string(),
        message: problem.message.clone(),
        notes: vec![Note {
            level: "help".to_string(),
            message: problem.help.clone(),
        }],
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
