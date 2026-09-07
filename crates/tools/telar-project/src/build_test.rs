use super::*;

/// `DIR_NAMES` is read by `cargo-telar` to recognise a generated path, and `dir_name` is what writes one. Two lists of the same thing, so a fifth flavour added to one and not the other makes every diagnostic in it point at generated Rust instead of at the `.rsx`.
#[test]
fn every_flavour_writes_a_directory_the_list_names() {
    for flavour in BuildFlavour::ALL {
        assert!(
            BuildFlavour::DIR_NAMES.contains(&flavour.dir_name()),
            "{flavour:?} writes `{}`, which nothing downstream recognises",
            flavour.dir_name()
        );
    }
    let mut names: Vec<&str> = BuildFlavour::ALL.iter().map(|f| f.dir_name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        BuildFlavour::DIR_NAMES.len(),
        "one directory per flavour, and no two sharing one"
    );
}

/// The pair a flavour stands for, round-tripped: a caller builds one from two booleans and reads them back off it.
#[test]
fn a_flavour_reports_the_pair_it_was_built_from() {
    for hot in [false, true] {
        for previews in [false, true] {
            let flavour = BuildFlavour::new(hot, previews);
            assert_eq!(flavour.is_hot(), hot, "{flavour:?}");
            assert_eq!(flavour.has_previews(), previews, "{flavour:?}");
        }
    }
}
