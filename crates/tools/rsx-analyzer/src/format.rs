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
//! Formatting is whole-document: the parsed AST is re-serialized and the backend
//! returns it as a single replacement edit, so it never has to map edits back
//! through the section line offsets.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rsx_parser::{
    Attr, Element, Preview, RsxDocument, Section, StyleClass, StyleConstant, StyleValue, ViewNode,
    header_section, parse,
};

const INDENT: &str = "    ";
/// Synthetic wrapper used to make the statement-level `[logic]` zone a valid Rust item for `rustfmt`.
const WRAPPER_FN: &str = "__rsx_fmt_logic_wrapper";

/// Formats a whole `.rsx` document. Returns `None` when the source does not parse
/// (an invalid document is left untouched, as every formatter does).
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

/// Returns the section headers present in `source`, in order of first appearance,
/// so empty-but-declared sections are preserved by the formatter.
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

/// Reformats the logic zone with `rustfmt`. Returns `None` (so the caller keeps
/// the source verbatim) when `rustfmt` is missing or rejects the input.
fn run_rustfmt_on_logic(logic: &str) -> Option<String> {
    let logic = logic.trim_end();
    if logic.trim().is_empty() {
        return None;
    }

    let wrapped = format!("fn {WRAPPER_FN}() {{\n{logic}\n}}\n");
    let formatted = run_rustfmt(&wrapped)?;
    unwrap_logic(&formatted)
}

/// Strips the synthetic wrapper function and one level of indentation that
/// `rustfmt` added, and turns preview sentinel comments back into attributes.
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

    let body: Vec<String> = lines[1..close]
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
    // Constants and classes live in two AST vecs but interleave in the source;
    // their `line` fields let us restore the original ordering.
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
            // `canvas` declares its drawing-area params on the first child line.
            if let Some(params) = &element.canvas_parameters {
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
            out.push_str(&format!("for {} in {}\n", block.pattern, block.iterable));
            for child in &block.body {
                emit_node(child, depth + 1, out);
            }
        }
        ViewNode::LetStmt { source, .. } => {
            out.push_str(&pad);
            out.push_str(source);
            out.push('\n');
        }
    }
}

/// Re-emits an element header as `tag .class "content" key:value`, the canonical
/// token order (the parser accepts these in any order but splits them apart).
fn format_element_header(element: &Element) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(1 + element.classes.len());
    parts.push(element.tag.clone());
    for class in &element.classes {
        parts.push(format!("@{class}"));
    }
    if let Some(content) = &element.content {
        parts.push(format!("\"{content}\""));
    }
    for attr in &element.attributes {
        parts.push(format_attr(attr));
    }
    parts.join(" ")
}

fn format_attr(attr: &Attr) -> String {
    if attr.is_quoted {
        format!("{}:\"{}\"", attr.key, attr.value)
    } else if attr.value.is_empty() {
        // Bare flag attribute, e.g. `ghost`.
        attr.key.clone()
    } else {
        format!("{}:{}", attr.key, attr.value)
    }
}

// === [preview] =============================================================

/// Re-emits a `[preview "Name" key:value flag …]` section: the header (name plus options) followed
/// by its body, formatted like a `[view]` tree.
fn format_preview_section(preview: &Preview) -> String {
    let mut header = format!("[preview \"{}\"", preview.name);
    for (key, value) in &preview.options {
        if value.is_empty() {
            header.push_str(&format!(" {key}"));
        } else {
            header.push_str(&format!(" {key}:{value}"));
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
        let src = "[view]\ncol @card\n        text \"Hi\" size:14 color:dark\n        row gap:8\n                btn \"+\" fill:primary on_press:|| count.update(|n| *n += 1)\n";
        let out = format_document(src).unwrap();
        let expected = "[view]\ncol @card\n    text \"Hi\" size:14 color:dark\n    row gap:8\n        btn \"+\" fill:primary on_press:|| count.update(|n| *n += 1)\n";
        assert_eq!(out, expected);
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
        let src = "[view]\nbtn \"Reset\" ghost label:\"x\" on_press:|| reset()\n";
        let out = format_document(src).unwrap();
        let expected = "[view]\nbtn \"Reset\" ghost label:\"x\" on_press:|| reset()\n";
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
        let src = "[logic]\nuse rsx::widgets::Button;\nuse rsx::prelude::*;\nlet count = signal(0i32);\n[view]\ncol\n\n[preview \"Default\"]\ncounter\n";
        let out = format_document(src).unwrap();
        // Imports are sorted...
        let prelude = out.find("use rsx::prelude::*;").unwrap();
        let button = out.find("use rsx::widgets::Button;").unwrap();
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
}
