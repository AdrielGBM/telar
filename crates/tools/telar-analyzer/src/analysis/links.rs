//! `textDocument/documentLink`: turns an `img src:"./logo.png"` (or `svg src:"./icon.svg"`) literal into a clickable link to the asset on disk. Only static, quoted, local paths that exist are linked — remote URLs (`http…`) and interpolated values (`{…}`) carry no resolvable target.

use std::path::Path;

use lsp_types::{DocumentLink, Range};
use telar_parser::{RsxDocument, ViewNode};
use telar_project::asset_kind_for_tag;

use crate::text::offset_to_position;

/// The asset paths in the document that resolve to a file, made clickable.
pub fn document_links(doc: &RsxDocument, source: &str, file_dir: &Path) -> Vec<DocumentLink> {
    let mut out = Vec::new();
    collect(&doc.view.nodes, source, file_dir, &mut out);
    for preview in &doc.previews {
        collect(&preview.body, source, file_dir, &mut out);
    }
    out
}

fn collect(nodes: &[ViewNode], source: &str, file_dir: &Path, out: &mut Vec<DocumentLink>) {
    for node in nodes {
        match node {
            ViewNode::Element(el) => {
                if let Some(kind) = asset_kind_for_tag(&el.tag) {
                    for attr in &el.attributes {
                        if attr.key == kind.attr
                            && attr.value.is_quoted()
                            && let Some(link) =
                                link_for(attr.value.text(), attr.value_start, source, file_dir)
                        {
                            out.push(link);
                        }
                    }
                }
                collect(&el.children, source, file_dir, out);
            }
            ViewNode::IfBlock(block) => {
                collect(&block.then_branch, source, file_dir, out);
                if let Some(else_branch) = &block.else_branch {
                    collect(else_branch, source, file_dir, out);
                }
            }
            ViewNode::ForBlock(block) => collect(&block.body, source, file_dir, out),
            ViewNode::MatchBlock(block) => {
                for arm in &block.arms {
                    collect(&arm.body, source, file_dir, out);
                }
            }
            ViewNode::LetStmt(_) | ViewNode::Comment(_) => {}
        }
    }
}

fn link_for(
    value: &str,
    value_start: usize,
    source: &str,
    file_dir: &Path,
) -> Option<DocumentLink> {
    if value.contains('{') || value.starts_with("http://") || value.starts_with("https://") {
        return None;
    }
    let target = file_dir.join(value);
    if !target.is_file() {
        return None;
    }
    let uri = crate::uri::from_path(&target)?;
    Some(DocumentLink {
        range: Range {
            start: offset_to_position(source, value_start),
            end: offset_to_position(source, value_start + value.len()),
        },
        target: Some(uri),
        tooltip: None,
        data: None,
    })
}

#[cfg(test)]
#[path = "links_test.rs"]
mod tests;
