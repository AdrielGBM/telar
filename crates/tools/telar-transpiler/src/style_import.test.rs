use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::*;

fn package(files: &[(&str, &str)]) -> HashMap<PathBuf, String> {
    files
        .iter()
        .map(|(path, source)| (Path::new("/pkg/src").join(path), source.to_string()))
        .collect()
}

fn resolve(files: &HashMap<PathBuf, String>, importer: &str) -> ResolvedImports {
    let path = Path::new("/pkg/src").join(importer);
    let document = telar_parser::parse(&files[&path]).expect("the importer parses");
    resolve_style_imports(&path, Path::new("/pkg/src"), &document.style, &|path| {
        files.get(path).cloned()
    })
}

const STYLES: &str = "[style]\npub @mono\n    font_family: monospace\n\npub @line\n    gap: 8\n\n@private\n    gap: 2\n";

#[test]
fn a_class_is_found_by_its_modules_path_from_the_crate_root() {
    let files = package(&[
        ("styles.rsx", STYLES),
        (
            "docs/table.rsx",
            "[style]\nuse crate::styles::{mono, line as rule};\n[view]\ntext @mono \"x\"\n",
        ),
    ]);
    let resolved = resolve(&files, "docs/table.rsx");
    assert_eq!(resolved.errors, Vec::new());
    let names: Vec<&str> = resolved
        .classes
        .iter()
        .map(|c| c.class.name.as_str())
        .collect();
    assert_eq!(names, ["mono", "rule"]);
    assert_eq!(resolved.classes[1].class.props[0].key, "gap");
    assert_eq!(resolved.classes[0].source, Path::new("/pkg/src/styles.rsx"));
    assert_eq!(resolved.classes[0].line, 2);
}

#[test]
fn super_reaches_a_sibling_and_a_directory_module_is_its_mod_rsx() {
    let files = package(&[
        ("docs/mod.rsx", "[style]\npub @card\n    gap: 4\n"),
        ("docs/shared.rsx", STYLES),
        (
            "docs/table.rsx",
            "[style]\nuse super::shared::mono;\nuse crate::docs::card;\n[view]\ncol\n",
        ),
    ]);
    let resolved = resolve(&files, "docs/table.rsx");
    assert_eq!(resolved.errors, Vec::new());
    assert_eq!(resolved.classes.len(), 2);
}

#[test]
fn a_glob_brings_in_only_what_is_pub() {
    let files = package(&[
        ("styles.rsx", STYLES),
        ("page.rsx", "[style]\nuse crate::styles::*;\n[view]\ncol\n"),
    ]);
    let names: Vec<String> = resolve(&files, "page.rsx")
        .classes
        .into_iter()
        .map(|c| c.class.name)
        .collect();
    assert_eq!(names, ["mono", "line"]);
}

/// Each refusal names what is wrong and where to fix it, on the line of the `use` that asked.
#[test]
fn an_import_that_cannot_resolve_says_why() {
    let files = package(&[
        ("styles.rsx", STYLES),
        (
            "page.rsx",
            "[style]\nuse crate::styles::private;\nuse crate::styles::missing;\nuse crate::nowhere::mono;\nuse super::super::mono;\n\n@mono\n    gap: 1\nuse crate::styles::mono;\n[view]\ncol\n",
        ),
    ]);
    let errors = resolve(&files, "page.rsx").errors;
    let messages: Vec<(usize, &str)> = errors
        .iter()
        .map(|e| (e.line, e.message.as_str()))
        .collect();
    assert_eq!(messages.len(), 5, "{messages:#?}");
    assert!(messages[0].1.contains("private to `crate::styles`") && messages[0].0 == 2);
    assert!(messages[1].1.contains("declares no class `@missing`"));
    assert!(messages[2].1.contains("no `.rsx` module"));
    assert!(messages[3].1.contains("above the crate root"));
    assert!(
        messages[4]
            .1
            .contains("declared in this `[style]` and imported too")
    );
}

#[test]
fn a_file_outside_a_package_refuses_every_import() {
    let document = telar_parser::parse("[style]\nuse crate::styles::mono;\n[view]\ncol\n").unwrap();
    let errors = ResolvedImports::unresolvable(&document.style).errors;
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 2);
}
