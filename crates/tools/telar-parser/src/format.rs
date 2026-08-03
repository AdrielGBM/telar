//! Canonical formatter for `.rsx` documents.
//!
//! A `.rsx` file is reformatted section by section:
//! - `[logic]` runs through `rustfmt`, so imports get reordered and long
//!   statements wrap exactly like a `.rs` file. The zone is statement-level Rust
//!   (`let` bindings live at its top level), which is not a valid item on its
//!   own, so it is wrapped in a synthetic `fn { ... }` before formatting and
//!   unwrapped afterwards.
//! - `[style]` and `[view]` are re-emitted from the parsed AST in a
//!   canonical shape: 4-space indentation, single-space token separators, and
//!   one blank line between style classes.
//!
//! Formatting is whole-document: the parsed AST is re-serialized and the backend returns it as a single replacement edit, so it never has to map edits back through the section line offsets.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{
    Attr, Element, Preview, RsxDocument, Section, StyleClass, StyleConstant, StyleValue, ViewNode,
    header_section, parse,
};

const INDENT: &str = "    ";
/// Synthetic wrapper used to make the statement-level `[logic]` zone a valid Rust item for `rustfmt`.
const WRAPPER_FN: &str = "__rsx_fmt_logic_wrapper";

/// Formats a whole `.rsx` document. Returns `None` when the source does not parse (an invalid document is left untouched, as every formatter does).
pub fn format_document(source: &str) -> Option<String> {
    let doc = parse(source).ok()?;
    let present = present_sections(source);

    let mut sections: Vec<String> = Vec::new();

    if present.contains(&Section::Logic) || !doc.logic.source.trim().is_empty() {
        sections.push(format_logic_section(&doc.logic.source));
    }
    if present.contains(&Section::Style)
        || !doc.style.constants.is_empty()
        || !doc.style.classes.is_empty()
    {
        sections.push(format_style_section(&doc));
    }
    if present.contains(&Section::View) || !doc.view.nodes.is_empty() {
        sections.push(format_view_section(&doc.view.nodes));
    }
    for preview in &doc.previews {
        sections.push(format_preview_section(preview));
    }

    if sections.is_empty() {
        return Some(String::new());
    }

    let mut out = sections.join("\n\n");
    out.push('\n');
    Some(out)
}

/// Returns the section headers present in `source`, in order of first appearance, so empty-but-declared sections are preserved by the formatter.
fn present_sections(source: &str) -> Vec<Section> {
    let mut present = Vec::new();
    for line in source.lines() {
        if let Some(section) = header_section(line.trim())
            && !present.contains(&section)
        {
            present.push(section);
        }
    }
    present
}

// === [logic] ===============================================================

fn format_logic_section(logic: &str) -> String {
    let body = run_rustfmt_on_logic(logic).unwrap_or_else(|| logic.trim_end().to_string());
    let body = body.trim_end();
    if body.is_empty() {
        "[logic]".to_string()
    } else {
        format!("[logic]\n{body}")
    }
}

/// Reformats the logic zone with `rustfmt`. Returns `None` (so the caller keeps the source verbatim) when `rustfmt` is missing or rejects the input.
fn run_rustfmt_on_logic(logic: &str) -> Option<String> {
    let logic = logic.trim_end();
    if logic.trim().is_empty() {
        return None;
    }

    let wrapped = format!("fn {WRAPPER_FN}() {{\n{logic}\n}}\n");
    let formatted = run_rustfmt(&wrapped)?;
    unwrap_logic(&formatted)
}

/// Strips the synthetic wrapper function and one level of indentation that `rustfmt` added, and turns preview sentinel comments back into attributes.
fn unwrap_logic(formatted: &str) -> Option<String> {
    let lines: Vec<&str> = formatted.lines().collect();
    let first = lines.first()?;
    if !first.trim_start().starts_with(&format!("fn {WRAPPER_FN}")) {
        return None;
    }
    // The wrapper's closing brace is the last non-blank line.
    let close = lines.iter().rposition(|l| l.trim() == "}")?;
    if close == 0 {
        return None;
    }

    // rustfmt exits 0 having only *partly* reformatted a body it could not fully parse: the inline prop-default sugar (`field: Type = expr`) is not valid Rust, so a `Props` struct using it comes back with the wrapper's indent on the lines around it and none on its own fields. There is then no right amount to strip — taking one level walks the author's struct to the left on every `cargo telar fmt`, and taking none leaves the wrapper's indent behind. Keep the source verbatim instead.
    let body = &lines[1..close];
    if !body
        .iter()
        .all(|line| line.trim().is_empty() || line.starts_with(INDENT))
    {
        return None;
    }
    let body: Vec<String> = body
        .iter()
        .map(|line| line.strip_prefix(INDENT).unwrap_or(line).to_string())
        .collect();

    Some(body.join("\n").trim_end().to_string())
}

fn run_rustfmt(input: &str) -> Option<String> {
    let rustfmt = find_rustfmt()?;
    let mut child = Command::new(rustfmt)
        .arg("--edition")
        .arg("2024")
        .arg("--emit")
        .arg("stdout")
        .arg("--quiet")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    {
        let mut stdin = child.stdin.take()?;
        stdin.write_all(input.as_bytes()).ok()?;
    }

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn find_rustfmt() -> Option<PathBuf> {
    let exe = format!("rustfmt{}", std::env::consts::EXE_SUFFIX);

    let path_env = std::env::var("PATH").unwrap_or_default();
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in path_env.split(sep) {
        let candidate = Path::new(dir).join(&exe);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    let cargo_bin = Path::new(&home).join(".cargo").join("bin").join(&exe);
    cargo_bin.exists().then_some(cargo_bin)
}

// === [style] ===============================================================

enum StyleEntry<'a> {
    Const(&'a StyleConstant),
    Class(&'a StyleClass),
}

fn format_style_section(doc: &RsxDocument) -> String {
    // Constants and classes live in two AST vecs but interleave in the source; their `line` fields let us restore the original ordering.
    let mut entries: Vec<(usize, StyleEntry)> = Vec::new();
    for constant in &doc.style.constants {
        entries.push((constant.line, StyleEntry::Const(constant)));
    }
    for class in &doc.style.classes {
        entries.push((class.line, StyleEntry::Class(class)));
    }
    entries.sort_by_key(|(line, _)| *line);

    let mut out = String::from("[style]");
    let mut prev_was_class = false;
    for (index, (_, entry)) in entries.iter().enumerate() {
        let is_class = matches!(entry, StyleEntry::Class(_));
        // A blank line sets classes apart from each other and from the constants block.
        if index > 0 && (is_class || prev_was_class) {
            out.push('\n');
        }
        out.push('\n');
        match entry {
            StyleEntry::Const(constant) => {
                out.push_str(&format!(
                    "{}: {}",
                    constant.name,
                    format_style_value(&constant.value)
                ));
            }
            StyleEntry::Class(class) => {
                out.push_str(&format!("@{}", class.name));
                for prop in &class.props {
                    out.push('\n');
                    out.push_str(INDENT);
                    out.push_str(&format!("{}: {}", prop.key, prop.value));
                }
            }
        }
        prev_was_class = is_class;
    }
    out
}

fn format_style_value(value: &StyleValue) -> String {
    match value {
        StyleValue::Hex(hex) => hex.clone(),
        // Display gives the shortest round-trip form and drops a trailing `.0`.
        StyleValue::Number(number) => format!("{number}"),
        StyleValue::Raw(raw) => raw.clone(),
    }
}

// === [view] ================================================================

fn format_view_section(nodes: &[ViewNode]) -> String {
    let mut body = String::new();
    for node in nodes {
        emit_node(node, 0, &mut body);
    }
    let body = body.trim_end();
    if body.is_empty() {
        "[view]".to_string()
    } else {
        format!("[view]\n{body}")
    }
}

fn emit_node(node: &ViewNode, depth: usize, out: &mut String) {
    let pad = INDENT.repeat(depth);
    match node {
        ViewNode::Element(element) => {
            out.push_str(&pad);
            out.push_str(&format_element_header(element));
            out.push('\n');
            // Leading `|params|` line (e.g. `canvas` drawing-area dimensions) re-emitted before children.
            if let Some(params) = &element.leading_params {
                out.push_str(&INDENT.repeat(depth + 1));
                out.push_str(&format!("|{params}|"));
                out.push('\n');
            }
            for child in &element.children {
                emit_node(child, depth + 1, out);
            }
        }
        ViewNode::IfBlock(block) => {
            out.push_str(&pad);
            out.push_str(&format!("if {}\n", block.condition));
            for child in &block.then_branch {
                emit_node(child, depth + 1, out);
            }
            if let Some(else_branch) = &block.else_branch {
                out.push_str(&pad);
                out.push_str("else\n");
                for child in else_branch {
                    emit_node(child, depth + 1, out);
                }
            }
        }
        ViewNode::ForBlock(block) => {
            out.push_str(&pad);
            out.push_str(&format!("for {} in {}", block.pattern, block.iterable));
            // Both clauses are optional and must survive a round-trip: dropping `gap:N` silently changes a
            // reactive row's spacing (and, since the transpiler keys the transparent gap fragment off it, its
            // whole layout path) on the next save.
            if let Some(key) = block
                .key_expr
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                out.push_str(&format!(" key {key}"));
            }
            if let Some(gap) = block
                .gap_expr
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                out.push_str(&format!(" gap:{gap}"));
            }
            out.push('\n');
            for child in &block.body {
                emit_node(child, depth + 1, out);
            }
        }
        ViewNode::LetStmt(stmt) => {
            out.push_str(&pad);
            out.push_str(&stmt.source);
            out.push('\n');
        }
    }
}

/// Re-emits an element header as `tag .class "content" key:value`, the canonical token order (the parser accepts these in any order but splits them apart).
fn format_element_header(element: &Element) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(1 + element.classes.len());
    parts.push(element.tag.clone());
    for class in &element.classes {
        parts.push(format!("@{class}"));
    }
    if let Some(content) = &element.content {
        parts.push(format!("\"{}\"", escape_rsx_string(content)));
    }
    for attr in &element.attributes {
        parts.push(format_attr(attr));
    }
    parts.join(" ")
}

/// Escapes a string value for re-emission inside `"…"`. Parsed content is stored unescaped (the parser
/// interprets `\n`/`\"`/… and raw `r"…"` keeps backslashes literal), so re-emitting verbatim would corrupt
/// any value containing a quote, backslash, or control char on the next save. Always emits the escaped
/// form; raw literals normalize to it (same content).
fn escape_rsx_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            _ => out.push(c),
        }
    }
    out
}

fn format_attr(attr: &Attr) -> String {
    if attr.is_quoted {
        format!("{}:\"{}\"", attr.key, escape_rsx_string(&attr.value))
    } else if attr.value.is_empty() {
        // Bare flag attribute, e.g. `ghost`.
        attr.key.clone()
    } else if attr.value.starts_with('|') {
        // A closure value uses the parenthesized form `key(|…| …)` — the colon form is gone, since it ran to end of line and swallowed any following attributes.
        format!("{}({})", attr.key, attr.value)
    } else {
        format!("{}:{}", attr.key, attr.value)
    }
}

// === [preview] =============================================================

/// Re-emits a `[preview "Name" key:value flag …]` section: the header (name plus options) followed by its body, formatted like a `[view]` tree.
fn format_preview_section(preview: &Preview) -> String {
    let mut header = format!("[preview \"{}\"", preview.name);
    for opt in &preview.options {
        if opt.value.is_empty() {
            header.push_str(&format!(" {}", opt.key));
        } else {
            header.push_str(&format!(" {}:{}", opt.key, opt.value));
        }
    }
    header.push(']');

    let mut body = String::new();
    for node in &preview.body {
        emit_node(node, 0, &mut body);
    }
    let body = body.trim_end();
    if body.is_empty() {
        header
    } else {
        format!("{header}\n{body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_view_indentation_and_token_order() {
        let src = "[view]\ncol @card\n        text \"Hi\" size:14 color:dark\n        row gap:8\n                btn \"+\" fill:primary on_press(|| count.update(|n| *n += 1))\n";
        let out = format_document(src).unwrap();
        let expected = "[view]\ncol @card\n    text \"Hi\" size:14 color:dark\n    row gap:8\n        btn \"+\" fill:primary on_press(|| count.update(|n| *n += 1))\n";
        assert_eq!(out, expected);
    }

    // Formatting a reactive `for` must preserve its `key <expr>` clause — dropping it turns a reactive
    // list into a compile error on the next save.
    // A backslash (e.g. from a raw string) must be escaped on re-emit, and formatting must be idempotent —
    // otherwise every save mangles the value.
    #[test]
    fn escapes_string_values_and_is_idempotent() {
        let out = format_document("[view]\ntext r\"a\\b\"\n").unwrap();
        assert!(
            out.contains("\"a\\\\b\""),
            "backslash escaped on re-emit:\n{out}"
        );
        assert_eq!(
            out,
            format_document(&out).unwrap(),
            "re-formatting is a fixed point"
        );
    }

    #[test]
    fn preserves_reactive_for_key_clause() {
        let src =
            "[view]\ncol\n    for todo in $todos key todo.id\n        text \"{todo.label}\"\n";
        let out = format_document(src).unwrap();
        assert!(
            out.contains("for todo in $todos key todo.id"),
            "the key clause must survive formatting:\n{out}"
        );
    }

    #[test]
    fn preserves_reactive_for_gap_clause() {
        // Dropping `gap:N` on format silently changes a reactive row's spacing and layout path — it must
        // round-trip alongside `key`.
        let src = "[view]\nrow\n    for id in $ids key *id gap:6\n        text \"{id}\"\n";
        let out = format_document(src).unwrap();
        assert!(
            out.contains("for id in $ids key *id gap:6"),
            "the gap clause must survive formatting:\n{out}"
        );
        // A gap without a key clause also round-trips.
        let keyless =
            format_document("[view]\nrow\n    for id in $ids gap:4\n        text \"{id}\"\n")
                .unwrap();
        assert!(
            keyless.contains("for id in $ids gap:4"),
            "a keyless gap clause must survive:\n{keyless}"
        );
    }

    #[test]
    fn restores_interleaved_constants_and_classes_in_source_order() {
        let src =
            "[style]\nprimary: #3d78fa\nradius: 6\n@card\n    width: 240\n    direction: col\n";
        let out = format_document(src).unwrap();
        let expected =
            "[style]\nprimary: #3d78fa\nradius: 6\n\n@card\n    width: 240\n    direction: col\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn inline_style_class_becomes_multiline() {
        let src = "[style]\n@badge: padding_x:6  padding_y:2  radius:6\n";
        let out = format_document(src).unwrap();
        let expected = "[style]\n@badge\n    padding_x: 6\n    padding_y: 2\n    radius: 6\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn preserves_quoted_and_flag_attrs() {
        let src = "[view]\nbtn \"Reset\" ghost label:\"x\" on_press(|| reset())\n";
        let out = format_document(src).unwrap();
        let expected = "[view]\nbtn \"Reset\" ghost label:\"x\" on_press(|| reset())\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn if_else_and_for_blocks_reindent() {
        let src = "[view]\ncol\n  if count > 0\n      text \"positive\"\n  else\n      text \"zero\"\n  for item in items\n      text \"{item}\"\n";
        let out = format_document(src).unwrap();
        let expected = "[view]\ncol\n    if count > 0\n        text \"positive\"\n    else\n        text \"zero\"\n    for item in items\n        text \"{item}\"\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn is_idempotent_for_ast_sections() {
        let src = "[style]\nprimary: #3d78fa\n\n@card\n    width: 240\n\n[view]\ncol @card\n    text \"Hi\" size:14 color:dark\n";
        let once = format_document(src).unwrap();
        let twice = format_document(&once).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn empty_document_stays_empty() {
        assert_eq!(format_document("").unwrap(), "");
    }

    #[test]
    fn invalid_document_is_left_untouched() {
        // Content before [logic] is a parse error, so no formatting is offered.
        assert!(format_document("stray text\n[view]\ncol\n").is_none());
    }

    #[test]
    fn logic_is_reordered_when_rustfmt_available() {
        if find_rustfmt().is_none() {
            return;
        }
        let src = "[logic]\nuse telar::widgets::Button;\nuse telar::prelude::*;\nlet count = signal(0i32);\n[view]\ncol\n\n[preview \"Default\"]\ncounter\n";
        let out = format_document(src).unwrap();
        // Imports are sorted...
        let prelude = out.find("use telar::prelude::*;").unwrap();
        let button = out.find("use telar::widgets::Button;").unwrap();
        assert!(prelude < button, "imports should be reordered:\n{out}");
        // ...the let binding survives...
        assert!(out.contains("let count = signal(0i32);"));
        // ...and the trailing `[preview …]` section is re-emitted with its body.
        assert!(
            out.contains("[preview \"Default\"]"),
            "preview section should survive:\n{out}"
        );
        assert!(out.contains("counter"));
    }

    /// The inline prop-default sugar (`field: Type = expr`) is not valid Rust, so rustfmt echoes the struct
    /// untouched and exits 0. Formatting must leave it exactly as written rather than unindent it — the fault
    /// compounds, so a file formatted twice loses two levels and eventually reads as a flat block.
    #[test]
    fn a_props_struct_with_inline_defaults_keeps_its_indentation() {
        if find_rustfmt().is_none() {
            return;
        }
        let src = "[logic]\npub struct Props {\n    pub text: Box<dyn Fn() -> String> = Box::new(String::new),\n    pub muted: bool = false,\n}\n\n[view]\ntext \"x\"\n";
        let once = format_document(src).unwrap();
        assert!(
            once.contains("    pub text: Box<dyn Fn() -> String> = Box::new(String::new),"),
            "the field keeps its indent:\n{once}"
        );
        assert_eq!(
            format_document(&once).unwrap(),
            once,
            "and formatting is idempotent"
        );
    }
}
