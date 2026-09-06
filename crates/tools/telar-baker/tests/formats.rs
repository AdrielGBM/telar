//! The format ceiling that disappeared when baking left the library.
//!
//! `renderer-assets` builds `image` with `png` + `jpeg` for an app; this crate builds it with `default-formats`, because it compiles once per machine into a CLI rather than once per project. So an `.rsx` can point `src:` at any format `image` reads without the project naming a feature for it — the choice the old in-macro baker could not afford to make.

const DOT_WEBP: &[u8] = include_bytes!("fixtures/dot.webp");

#[test]
fn a_format_no_app_build_decodes_still_bakes() {
    let baked = telar_baker::baker_for_id("image")
        .expect("image is a registered asset kind")
        .bake(DOT_WEBP)
        .expect("the CLI decodes every format `image` ships");
    assert!(baked.starts_with("ImageData::"), "{baked}");
}
