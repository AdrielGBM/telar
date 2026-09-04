//! Canonical formatter for `.rsx` documents.
//!
//! A `.rsx` file is reformatted section by section:
//! - `[logic]` runs through `rustfmt`, so imports get reordered and long statements wrap exactly like a `.rs` file. The zone is statement-level Rust (`let` bindings live at its top level), which is not a valid item on its own, so it is wrapped in a synthetic `fn { ... }` before formatting and unwrapped afterwards.
//! - `[style]` and `[view]` are re-emitted from the parsed AST in a canonical shape: 4-space indentation, single-space token separators, and one blank line between style classes.
//!
//! Formatting is whole-document: the parsed AST is re-serialized and the backend returns it as a single replacement edit, so it never has to map edits back through the section line offsets.

mod logic;
mod style;
mod view;

use crate::{Section, header_section, parse};

use logic::format_logic_section;
use style::format_style_section;
use view::{format_preview_section, format_view_section};

const INDENT: &str = "    ";

/// Formats a whole `.rsx` document. Returns `None` when the source does not parse (an invalid document is left untouched, as every formatter does).
pub fn format_document(source: &str) -> Option<String> {
    let doc = parse(source).ok()?;
    let present = present_sections(source);

    let mut sections: Vec<String> = Vec::new();

    if present.contains(&Section::Logic) || !doc.logic.source.trim().is_empty() {
        sections.push(format_logic_section(&doc.logic.source));
    }
    if present.contains(&Section::Style) || !doc.style.classes.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::logic::find_rustfmt;
    use super::*;

    #[test]
    fn normalizes_view_indentation_and_token_order() {
        let src = "[view]\ncol @card\n        text \"Hi\" size:14 color:dark\n        row gap:8\n                btn \"+\" fill:primary on_press:(|| count.update(|n| *n += 1))\n";
        let out = format_document(src).unwrap();
        let expected = "[view]\ncol @card\n    text \"Hi\" size:14 color:dark\n    row gap:8\n        btn \"+\" fill:primary on_press:(|| count.update(|n| *n += 1))\n";
        assert_eq!(out, expected);
    }

    // Dropping the `key <expr>` clause turns a reactive list into a compile error on the next save. A backslash must be escaped on re-emit, and formatting must be idempotent, or every save mangles the value.
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
        // Round-trips alongside `key`.
        let src = "[view]\nrow\n    for id in $ids key *id gap:6\n        text \"{id}\"\n";
        let out = format_document(src).unwrap();
        assert!(
            out.contains("for id in $ids key *id gap:6"),
            "the gap clause must survive formatting:\n{out}"
        );
        let keyless =
            format_document("[view]\nrow\n    for id in $ids gap:4\n        text \"{id}\"\n")
                .unwrap();
        assert!(
            keyless.contains("for id in $ids gap:4"),
            "a keyless gap clause must survive:\n{keyless}"
        );
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
        let src = "[view]\nbtn \"Reset\" ghost label:\"x\" on_press:(|| reset())\n";
        let out = format_document(src).unwrap();
        let expected = "[view]\nbtn \"Reset\" ghost label:\"x\" on_press:(|| reset())\n";
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
        let src = "[style]\n@card\n    width: 240\n\n[view]\ncol @card\n    text \"Hi\" size:14 color:theme.dark\n";
        let once = format_document(src).unwrap();
        let twice = format_document(&once).unwrap();
        assert_eq!(once, twice);
    }

    /// A value is one token of the AST, so the formatter re-emits it byte for byte — it never reaches inside an expression to wrap, re-space or re-order it, which would move the span a diagnostic points at.
    #[test]
    fn an_expression_survives_formatting_byte_for_byte() {
        let src = concat!(
            "[view]\n",
            "col gap:(spacing() * 2.0) pad:$theme.gutter\n",
            "    btn \"Save\" on_press:(|| save(&draft, /* keep */ true)) fill:linear(horizontal, $theme.primary, #3d78fa)\n",
        );
        assert_eq!(format_document(src).unwrap(), src);
    }

    #[test]
    fn empty_document_stays_empty() {
        assert_eq!(format_document("").unwrap(), "");
    }

    #[test]
    fn invalid_document_is_left_untouched() {
        assert!(format_document("stray text\n[view]\ncol\n").is_none());
    }

    #[test]
    fn logic_is_reordered_when_rustfmt_available() {
        if find_rustfmt().is_none() {
            return;
        }
        let src = "[logic]\nuse telar::widgets::Button;\nuse telar::prelude::*;\nlet count = signal(0i32);\n[view]\ncol\n\n[preview \"Default\"]\ncounter\n";
        let out = format_document(src).unwrap();
        let prelude = out.find("use telar::prelude::*;").unwrap();
        let button = out.find("use telar::widgets::Button;").unwrap();
        assert!(prelude < button, "imports should be reordered:\n{out}");
        assert!(out.contains("let count = signal(0i32);"));
        assert!(
            out.contains("[preview \"Default\"]"),
            "preview section should survive:\n{out}"
        );
        assert!(out.contains("counter"));
    }

    /// The colon form rejects a closure — it runs to end of line and swallows whatever follows — so re-emitting one with a colon does not merely reformat the file, it breaks the file it just formatted.
    #[test]
    fn a_move_closure_attribute_keeps_its_parens() {
        let src = "[view]\nicon name:(move || label()) tint:(|| fg()) size:16\n";
        let out = format_document(src).unwrap();
        assert!(
            out.contains("name:(move || label())") && out.contains("tint:(|| fg())"),
            "a closure keeps the parens it needs to hold its spaces:\n{out}"
        );
        assert_eq!(format_document(&out).unwrap(), out, "and it is idempotent");
    }

    /// An attribute must come back in the form it was written in, because the form is what it means: a `t"…"` re-emitted with a colon becomes one attribute followed by a run of stray flags, and a catalogue lookup re-emitted as a plain literal turns into its own key. Both were possible while the spelling was guessed from the text rather than read off the value.
    #[test]
    fn a_directive_and_a_lookup_survive_a_round_trip() {
        let src =
            "[view]\nbox transition(fill 250ms ease-out)\n    btn label:t!(\"buttons.save\")\n";
        let out = format_document(src).unwrap();
        assert_eq!(out, src);
        assert_eq!(format_document(&out).unwrap(), out, "and it is idempotent");
    }

    /// Losing the `virtual` clause would turn a list that builds ten rows into one that builds ten thousand, silently, on the next format.
    #[test]
    fn a_virtual_for_keeps_its_clause() {
        let src = "[view]\nscroll\n    for row in $rows key row.id virtual row_height:32\n        text \"a\"\n";
        let out = format_document(src).unwrap();
        assert!(
            out.contains("for row in $rows key row.id virtual row_height:32\n"),
            "the clause comes back:\n{out}"
        );
        assert_eq!(format_document(&out).unwrap(), out, "and it is idempotent");
    }

    /// Both `for` clauses change behaviour, so neither may be lost on a round trip: `key` decides what reconciles, and `gap` is what the transparent fragment spaces its items by.
    #[test]
    fn a_for_keeps_its_key_and_gap_clauses() {
        let src = "[view]\ncol\n    for x in $items key x.id gap:8\n        text \"a\"\n";
        let out = format_document(src).unwrap();
        assert!(
            out.contains("for x in $items key x.id gap:8\n"),
            "both clauses come back:\n{out}"
        );
        assert_eq!(format_document(&out).unwrap(), out, "and it is idempotent");
    }

    /// A `match` header carries two optional clauses, and both change behaviour: dropping `key` silently downgrades reconciliation to the variant, dropping `as` breaks the key that reads the binding.
    #[test]
    fn a_match_header_survives_a_round_trip() {
        let src = "[view]\ncol\n    match $state as s key s.id()\n        Ready(svg)\n            text \"ok\"\n        _\n            text \"…\"\n";
        let out = format_document(src).unwrap();
        assert!(
            out.contains("match $state as s key s.id()\n"),
            "both clauses come back:\n{out}"
        );
        assert!(
            out.contains("        Ready(svg)\n"),
            "arms keep their indent:\n{out}"
        );
        assert_eq!(format_document(&out).unwrap(), out, "and it is idempotent");
    }

    /// An `else if` chain parses to a nested `if` inside the else-branch, and re-emitting it as that nesting would rewrite the author's spelling on every format.
    #[test]
    fn an_else_if_chain_survives_a_round_trip() {
        let src = "[view]\ncol\n    if n > 1\n        text \"many\"\n    else if n > 0\n        text \"one\"\n    else\n        text \"none\"\n";
        let out = format_document(src).unwrap();
        assert!(
            out.contains("    else if n > 0\n"),
            "the chain comes back as a chain:\n{out}"
        );
        assert_eq!(format_document(&out).unwrap(), out, "and is idempotent");
    }

    /// The inline prop-default sugar (`field: Type = expr`) is not valid Rust, so rustfmt echoes the struct untouched and exits 0. Formatting must leave it exactly as written rather than unindent it — the fault compounds, so a file formatted twice loses two levels and eventually reads as a flat block.
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
