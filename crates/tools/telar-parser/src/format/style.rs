//! The `[style]` zone, re-emitted from the AST: its class imports first, then one class per block, one blank line between them.

use crate::{RsxDocument, StyleClass, StyleImport, StyleImportItems};

use super::INDENT;

pub(super) fn format_style_section(doc: &RsxDocument) -> String {
    let mut out = String::from("[style]");
    for import in &doc.style.imports {
        out.push('\n');
        out.push_str(&format_import(import));
    }
    let mut classes: Vec<&StyleClass> = doc.style.classes.iter().collect();
    classes.sort_by_key(|class| class.line);
    for (index, class) in classes.into_iter().enumerate() {
        if index > 0 || !doc.style.imports.is_empty() {
            out.push('\n');
        }
        out.push('\n');
        if class.public {
            out.push_str("pub ");
        }
        out.push_str(&format!("@{}", class.name));
        for prop in &class.props {
            out.push('\n');
            out.push_str(INDENT);
            out.push_str(&format!("{}: {}", prop.key, prop.value));
        }
    }
    out
}

fn format_import(import: &StyleImport) -> String {
    let module = import.module.join("::");
    let items = match &import.items {
        StyleImportItems::Glob => "*".to_string(),
        StyleImportItems::Names(names) => {
            let names: Vec<String> = names
                .iter()
                .map(|name| match &name.alias {
                    Some(alias) => format!("{} as {alias}", name.name),
                    None => name.name.clone(),
                })
                .collect();
            match names.as_slice() {
                [one] => one.clone(),
                _ => format!("{{{}}}", names.join(", ")),
            }
        }
    };
    format!("use {module}::{items};")
}
