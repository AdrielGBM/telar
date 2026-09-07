//! The format ceiling that disappeared when baking left the library.
//!
//! `renderer-assets` builds `image` with `png` + `jpeg` for an app; this crate builds it with `default-formats`, because it compiles once per machine into a CLI rather than once per project. So an `.rsx` can point `src:` at any format `image` reads without the project naming a feature for it — the choice the old in-macro baker could not afford to make.

const DOT_WEBP: &[u8] = include_bytes!("../fixtures/dot.webp");

#[test]
fn a_format_no_app_build_decodes_still_bakes() {
    let baked = telar_baker::baker_for_id("image")
        .expect("image is a registered asset kind")
        .bake(DOT_WEBP)
        .expect("the CLI decodes every format `image` ships");
    assert!(baked.starts_with("ImageData::"), "{baked}");
}

/// The registry's extension list is what a file watcher asks "is that file an asset", and it is written out by hand in `telar-transpiler`, which cannot link `image` to ask. So the agreement is checked here instead: a format the CLI decodes but the registry does not name is one whose edits raise no event, which is how `.webp` went unnoticed.
#[test]
fn the_registry_lists_every_readable_format() {
    let kind = telar_project::asset_kind_for_id("image").expect("the image kind is registered");
    for format in image::ImageFormat::all().filter(image::ImageFormat::reading_enabled) {
        for ext in format.extensions_str() {
            assert!(
                kind.extensions.contains(ext),
                "`{ext}` ({format:?}) bakes but is missing from ASSET_KINDS"
            );
        }
    }
}

/// And the other way: an extension nothing can decode would make the watcher rebuild for a file the baker then rejects.
#[test]
fn the_registry_lists_nothing_unreadable() {
    let kind = telar_project::asset_kind_for_id("image").expect("the image kind is registered");
    for ext in kind.extensions {
        let format = image::ImageFormat::from_extension(ext);
        assert!(
            format.is_some_and(|f| f.reading_enabled()),
            "ASSET_KINDS names `{ext}`, which the baker cannot read"
        );
    }
}
