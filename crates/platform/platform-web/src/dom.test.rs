use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use super::{asset_base, document, host};

wasm_bindgen_test_configure!(run_in_browser);

const ASSETS_META: &str = "test-assets";

fn with_element(tag: &str, attributes: &[(&str, &str)], check: impl FnOnce()) {
    let element = document().create_element(tag).expect("an element");
    for (name, value) in attributes {
        element.set_attribute(name, value).expect("an attribute");
    }
    document()
        .body()
        .expect("a body")
        .append_child(element.as_ref())
        .expect("the element went into the page");
    check();
    element.remove();
}

fn with_assets_meta(content: &str, check: impl FnOnce()) {
    with_element(
        "meta",
        &[("name", ASSETS_META), ("content", content)],
        check,
    );
}

#[wasm_bindgen_test]
fn files_on_the_page_s_own_origin_are_addressed_from_the_site_s_root() {
    with_assets_meta("/site/", || {
        assert_eq!(asset_base(Some(ASSETS_META)).as_deref(), Some("/site/"));
    });
}

#[wasm_bindgen_test]
fn files_on_another_origin_keep_their_full_address() {
    with_assets_meta("https://cdn.example.com/site/", || {
        assert_eq!(
            asset_base(Some(ASSETS_META)).as_deref(),
            Some("https://cdn.example.com/site/")
        );
    });
}

#[wasm_bindgen_test]
fn a_meta_under_another_name_says_nothing_about_where_the_files_are() {
    with_assets_meta("https://cdn.example.com/site/", || {
        assert_ne!(
            asset_base(Some("another-name")).as_deref(),
            Some("https://cdn.example.com/site/")
        );
    });
}

#[wasm_bindgen_test]
fn the_app_mounts_on_the_element_the_page_names_and_on_the_body_without_one() {
    with_element("div", &[("id", "test-root")], || {
        assert_eq!(
            host(None, Some("test-root")).expect("a host").id(),
            "test-root"
        );
        assert_eq!(
            host(None, None).expect("a host"),
            document().body().expect("a body")
        );
    });
}
