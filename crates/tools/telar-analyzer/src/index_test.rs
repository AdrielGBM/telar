use super::*;

// url::Url::from_file_path rejects unix-style absolute paths on Windows, so tests build platform-valid ones.
fn abs(unix: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!("C:{}", unix.replace('/', "\\")))
    } else {
        PathBuf::from(unix)
    }
}

#[test]
fn indexes_stem_classes_and_tags() {
    let src = "[style]\n@card\n    width: 240\n[view]\ncol @card\n    feature_card icon:\"x\"\n";
    let entry = index_source(&abs("/x/src/home.rsx"), src).unwrap();
    assert_eq!(entry.stem, "home");
    assert_eq!(entry.classes, vec![("card".to_string(), 1)]);
    assert_eq!(entry.tags.len(), 1);
    assert_eq!(entry.tags[0].name, "feature_card");
    assert_eq!(entry.tags[0].range.start.line, 5);
}

#[test]
fn symbols_and_references_query_the_cache() {
    let mut idx = WorkspaceIndex {
        root: abs("/x"),
        files: HashMap::new(),
    };
    let src = "[view]\ncol\n    feature_card\n";
    idx.update(&abs("/x/src/home.rsx"), src);
    idx.update(
        &abs("/x/src/feature_card.rsx"),
        "[view]\ncol\n    text \"hi\"\n",
    );

    let refs = idx.component_references("feature_card");
    assert_eq!(refs.len(), 2);

    let syms = idx.symbols("feature");
    assert!(
        syms.iter().any(|s| s.name == "feature_card"),
        "the component is missing from the index: {syms:?}"
    );
}

/// A `.rsx` is a module named after its file, so renaming the file moves the segment every importer spells — the half of a component rename that rust-analyzer cannot see, because the name it changes lives on disk rather than in the generated Rust.
#[test]
fn a_use_line_records_the_module_a_component_is_imported_from() {
    let source =
        "[logic]\nuse crate::ui::card::{card, CardProps};\nuse std::rc::Rc;\n\n[view]\ncard\n";
    let imports = scan_component_imports(source);
    assert_eq!(imports.len(), 1, "only the component import is one");
    assert_eq!(imports[0].name, "card");
    assert_eq!(imports[0].range.start.line, 1);

    let line = "use crate::ui::card::{card, CardProps};";
    let (segment, at) = component_module_segment(line).unwrap();
    assert_eq!(&line[at..at + segment.len()], "card");
}

#[test]
fn a_use_line_that_imports_nothing_of_its_own_name_is_not_one() {
    for line in [
        "use std::rc::Rc;",
        "use crate::theme::palette::{Palette};",
        "use crate::ui::card::CardProps;",
    ] {
        assert!(
            component_module_segment(line).is_none(),
            "`{line}` is not a component import"
        );
    }
}
