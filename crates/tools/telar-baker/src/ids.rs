//! The ids a package's `.rsx` gives a component-named asset kind, such as `icon name:"mdi:home"`, collected for the bake.

use std::path::{Path, PathBuf};

use telar_parser::{RsxDocument, Value, ViewNode};
use telar_project::AssetKind;

/// One prop that names an asset by id.
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

/// Every prop of `kinds` that `doc`'s view and previews write, in document order. `file` is where `doc` was read from.
pub fn collect_id_refs(
    doc: &RsxDocument,
    file: &Path,
    kinds: &[&'static AssetKind],
    out: &mut Vec<IdRef>,
) {
    let mut walk = Walk { file, kinds, out };
    walk.nodes(&doc.view.nodes);
    for preview in &doc.previews {
        walk.nodes(&preview.body);
    }
}

struct Walk<'a> {
    file: &'a Path,
    kinds: &'a [&'static AssetKind],
    out: &'a mut Vec<IdRef>,
}

impl Walk<'_> {
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
                    self.nodes(&el.children);
                }
                ViewNode::IfBlock(block) => {
                    self.nodes(&block.then_branch);
                    if let Some(else_branch) = &block.else_branch {
                        self.nodes(else_branch);
                    }
                }
                ViewNode::ForBlock(block) => self.nodes(&block.body),
                ViewNode::MatchBlock(block) => {
                    for arm in &block.arms {
                        self.nodes(&arm.body);
                    }
                }
                ViewNode::LetStmt(_) | ViewNode::Comment(_) => {}
            }
        }
    }
}
