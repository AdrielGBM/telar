//! Line-oriented lexer for `.rsx` source.
//!
//! `.rsx` is whitespace-sensitive, so the lexer keeps working at the line level instead of producing a flat token stream. It classifies each line by the active section:
//!
//! - Logic, `[play]` and `[previews]` prose lines are captured verbatim.
//! - Style and View lines carry their original text plus the leading indentation width, which the parser uses to reconstruct the view hierarchy.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Which zone of a `.rsx` a line belongs to.
pub enum Section {
    Unknown,
    Logic,
    Style,
    View,
    /// A `[preview "Name" …]` header line. Its body is lexed as [`Section::View`] (the preview body is view markup), so the parser reuses the whole view machinery for it.
    Preview,
    /// A `[previews "Group/Title" …]` header line, the meta section every preview in the file shares. Its body is lexed as [`Section::Prose`].
    Previews,
    /// The prose under a `[previews …]` header, kept verbatim for the docs page.
    Prose,
    /// A `[play]` zone: verbatim Rust run against the preview it follows. Unlike the other fixed headers, its header line is kept, since `[play]` repeats once per preview.
    Play,
}

impl Section {
    /// Whether the zone holds Rust or prose kept as written rather than `.rsx` markup, so markup scans (`@class`, tags, signals) skip it.
    pub fn is_verbatim(self) -> bool {
        matches!(self, Self::Logic | Self::Unknown | Self::Play | Self::Prose)
    }
}

#[derive(Debug, Clone)]
/// One lexed line: its section, its indentation, its content and the bytes it came from.
pub struct Line {
    pub section: Section,
    /// 1-based line number in the original source.
    pub number: usize,
    /// Number of leading spaces (tabs count as one space each).
    pub indent: usize,
    /// The line content with leading indentation stripped (trailing whitespace trimmed too).
    pub content: String,
    /// Absolute byte offset in the original source where `content` begins (line byte start + leading-whitespace bytes). Lets the parser turn intra-line char positions into source byte offsets so the transpiler can map `[view]` Rust expressions back to the `.rsx` precisely.
    pub content_start: usize,
    /// The raw, untouched line (used to preserve the verbatim zones exactly).
    pub raw: String,
}

impl Line {
    pub fn is_blank(&self) -> bool {
        self.content.is_empty()
    }

    /// Whether this is a `[play]` header rather than a line of a play zone's Rust.
    pub fn is_play_header(&self) -> bool {
        self.section == Section::Play && header_section(&self.content) == Some(Section::Play)
    }
}

/// Returns the [`Section`] a fixed `[...]` header line switches into, or `None` for any other line. `trimmed` must already have surrounding whitespace removed.
pub fn header_section(trimmed: &str) -> Option<Section> {
    match trimmed {
        "[logic]" => Some(Section::Logic),
        "[style]" => Some(Section::Style),
        "[view]" => Some(Section::View),
        "[play]" => Some(Section::Play),
        _ => None,
    }
}

/// A header line's own section and the section of the lines after it, or `None` for a non-header line. A fixed header is both; a parameterized one is a line of its own kind that opens the zone its body is written in.
fn classify_header(trimmed: &str) -> Option<(Section, Section)> {
    if let Some(section) = header_section(trimmed) {
        Some((section, section))
    } else if is_preview_header(trimmed) {
        Some((Section::Preview, Section::View))
    } else if is_previews_header(trimmed) {
        Some((Section::Previews, Section::Prose))
    } else {
        None
    }
}

/// The section the lines after `trimmed` belong to when it is a header of any kind, `[preview …]` and `[previews …]` included. For a scan that tracks the current section line by line: [`header_section`] alone misses the parameterized headers, so the zone of a `[play]` would never end.
pub fn section_opened_by(trimmed: &str) -> Option<Section> {
    classify_header(trimmed).map(|(_, opened)| opened)
}

/// The [`Section`] a 0-based line of `source` belongs to. For tooling that has a line number and needs to know what kind of source is on it — which is a different question from lexing, since the caller is holding a coordinate rather than the text. A parameterized header line reports its own kind ([`Section::Preview`], [`Section::Previews`]), as the lexer classifies it.
pub fn find_section_at(source: &str, line: u32) -> Section {
    let target = line as usize;
    let mut current = Section::Unknown;
    for (i, line) in source.lines().enumerate() {
        let header = classify_header(line.trim());
        if let Some((_, opened)) = header {
            current = opened;
        }
        if i == target {
            return header.map_or(current, |(own, _)| own);
        }
    }
    current
}

/// Splits `source` into classified lines, switching sections on every header.
pub fn lex(source: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut section = Section::Unknown;
    // Running byte offset of the current chunk's start within `source`.
    let mut byte_offset = 0usize;

    for (idx, chunk) in source.split_inclusive('\n').enumerate() {
        let line_byte_start = byte_offset;
        byte_offset += chunk.len();

        let number = idx + 1;
        // Mirrors `str::lines()`: drop the trailing `\n` and any `\r`, so `raw` stays unchanged.
        let raw = chunk.strip_suffix('\n').unwrap_or(chunk);
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = raw.trim();

        let (line_section, next_section) = match classify_header(trimmed) {
            Some((Section::Logic | Section::Style | Section::View, opened)) => {
                section = opened;
                continue;
            }
            Some(header) => header,
            None => (section, section),
        };

        lines.push(Line {
            section: line_section,
            number,
            indent: leading_indent(raw),
            content: trimmed.to_string(),
            content_start: line_byte_start + (raw.len() - raw.trim_start().len()),
            raw: raw.to_string(),
        });
        section = next_section;
    }

    lines
}

/// Strips a `[<keyword> …]` header down to the remainder after the keyword, or `None` when `trimmed` is not that bracketed header. The keyword must be a whole word, so `preview` matches neither `[previews …]` nor `[previewish]`. This is the single home of the bracket/keyword rule: the lexer classifies with it and the preview parser consumes the remainder, so the two cannot drift apart.
fn strip_keyword_header<'a>(trimmed: &'a str, keyword: &str) -> Option<HeaderRest<'a>> {
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))?;
    let inner_trimmed = inner.trim();
    let rest = inner_trimmed.strip_prefix(keyword)?;
    if !(rest.is_empty() || rest.starts_with(char::is_whitespace)) {
        return None;
    }
    let text = rest.trim_start();
    let offset =
        1 + (inner.len() - inner.trim_start().len()) + keyword.len() + (rest.len() - text.len());
    Some(HeaderRest { offset, text })
}

/// What follows a header keyword and the byte offset where it begins within the header line, so a parser can map positions in it back to the source.
pub(crate) struct HeaderRest<'a> {
    pub offset: usize,
    pub text: &'a str,
}

/// The name+options remainder of a `[preview …]` header.
pub(crate) fn strip_preview_header(trimmed: &str) -> Option<HeaderRest<'_>> {
    strip_keyword_header(trimmed, "preview")
}

/// The title+options remainder of a `[previews …]` header.
pub(crate) fn strip_previews_header(trimmed: &str) -> Option<HeaderRest<'_>> {
    strip_keyword_header(trimmed, "previews")
}

/// Whether `trimmed` is a `[preview …]` header. Parameterized (carries a name/options), so it is matched here rather than in [`header_section`]'s exact table. Public so tooling that walks sections (e.g. selection ranges) can treat a preview header as a section boundary like the fixed headers.
pub fn is_preview_header(trimmed: &str) -> bool {
    strip_preview_header(trimmed).is_some()
}

/// Whether `trimmed` is a `[previews …]` meta header, which [`is_preview_header`] does not match.
pub fn is_previews_header(trimmed: &str) -> bool {
    strip_previews_header(trimmed).is_some()
}

/// Counts leading whitespace columns; a tab is treated as a single column.
fn leading_indent(line: &str) -> usize {
    let mut count = 0;
    for ch in line.chars() {
        match ch {
            ' ' | '\t' => count += 1,
            _ => break,
        }
    }
    count
}
