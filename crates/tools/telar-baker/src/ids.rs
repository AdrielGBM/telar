//! The ids a package's `.rsx` gives a component-named asset kind, such as `icon name:"mdi:home"`, collected for the bake.

use std::path::{Path, PathBuf};

use telar_parser::{RsxDocument, Value, ViewNode};
use telar_project::AssetKind;

use crate::macro_ids::collect_macro_ids;

/// One prop, or one macro call, that names an asset by id.
#[derive(Clone)]
pub struct IdRef {
    pub kind: &'static AssetKind,
    /// The id, when the prop is written as a literal. `None` for a signal or an expression, which cannot be baked.
    pub literal: Option<String>,
    /// The value as written, for a message about it.
    pub written: String,
    pub file: PathBuf,
    /// 1-based.
    pub line: usize,
}

impl IdRef {
    /// `file:line`, relative to `package_dir` where it can be.
    pub fn location(&self, package_dir: &Path) -> String {
        let file = self.file.strip_prefix(package_dir).unwrap_or(&self.file);
        format!("{}:{}", file.display(), self.line)
    }
}

/// Every prop of `kinds` that `doc`'s view and previews write, and every `tag!("id")` its Rust names one by — in its logic zone, its `[play]` zones and the expressions its view writes — where `tag` is the kind's component. `file` is where `doc` was read from, and `source` its text.
pub fn collect_id_refs(
    doc: &RsxDocument,
    source: &str,
    file: &Path,
    kinds: &[&'static AssetKind],
    out: &mut Vec<IdRef>,
) {
    collect_macro_ids(&doc.logic.source, file, doc.logic.start_line, kinds, out);
    let mut walk = Walk {
        source,
        file,
        kinds,
        out,
    };
    walk.nodes(&doc.view.nodes);
    for preview in &doc.previews {
        for arg in &preview.args {
            if let Value::Expr(text) = &arg.default {
                walk.rust(text, preview.line);
            }
        }
        walk.nodes(&preview.body);
        if let Some(play) = &preview.play {
            walk.rust(&play.source, play.start_line);
        }
    }
}

struct Walk<'a> {
    source: &'a str,
    file: &'a Path,
    kinds: &'a [&'static AssetKind],
    out: &'a mut Vec<IdRef>,
}

impl Walk<'_> {
    fn rust(&mut self, text: &str, line: usize) {
        collect_macro_ids(text, self.file, line, self.kinds, self.out);
    }

    fn line_at(&self, offset: usize) -> usize {
        self.source
            .get(..offset)
            .map_or(1, |before| before.matches('\n').count() + 1)
    }

    fn nodes(&mut self, nodes: &[ViewNode]) {
        for node in nodes {
            match node {
                ViewNode::Element(el) => {
                    let tag = el.tag.rsplit("::").next().unwrap_or(&el.tag);
                    for kind in self.kinds {
                        if !kind.component.is_some_and(|component| component.tag == tag) {
                            continue;
                        }
                        for attr in el.attributes.iter().filter(|attr| attr.key == kind.attr) {
                            let literal = match &attr.value {
                                Value::Quoted(id) => Some(id.trim().to_string()),
                                _ => None,
                            };
                            self.out.push(IdRef {
                                kind,
                                literal,
                                written: attr.value.text().trim().to_string(),
                                file: self.file.to_path_buf(),
                                line: el.line,
                            });
                        }
                    }
                    for attr in &el.attributes {
                        if let Value::Expr(text) = &attr.value {
                            self.rust(text, self.line_at(attr.value_start));
                        }
                    }
                    self.nodes(&el.children);
                }
                ViewNode::IfBlock(block) => {
                    self.rust(&block.condition, block.line);
                    self.nodes(&block.then_branch);
                    if let Some(else_branch) = &block.else_branch {
                        self.nodes(else_branch);
                    }
                }
                ViewNode::ForBlock(block) => {
                    self.rust(&block.iterable, block.line);
                    self.nodes(&block.body);
                }
                ViewNode::MatchBlock(block) => {
                    self.rust(&block.scrutinee, block.line);
                    for arm in &block.arms {
                        self.nodes(&arm.body);
                    }
                }
                ViewNode::LetStmt(statement) => {
                    self.rust(&statement.source, self.line_at(statement.source_start));
                }
                ViewNode::Comment(_) => {}
            }
        }
    }
}
