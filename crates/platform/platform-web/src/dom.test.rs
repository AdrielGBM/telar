use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use super::{ASSETS_META, asset_base, document};

wasm_bindgen_test_configure!(run_in_browser);

fn with_assets_meta(content: &str, check: impl FnOnce()) {
    let meta = document().create_element("meta").expect("a meta element");
    meta.set_attribute("name", ASSETS_META).expect("a name");
    meta.set_attribute("content", content).expect("a content");
    document()
        .body()
        .expect("a body")
        .append_child(meta.as_ref())
        .expect("the meta went into the page");
    check();
    meta.remove();
}

#[wasm_bindgen_test]
fn files_on_the_page_s_own_origin_are_addressed_from_the_site_s_root() {
    with_assets_meta("/site/", || {
        assert_eq!(asset_base().as_deref(), Some("/site/"));
    });
}

#[wasm_bindgen_test]
fn files_on_another_origin_keep_their_full_address() {
    with_assets_meta("https://cdn.example.com/site/", || {
        assert_eq!(
            asset_base().as_deref(),
            Some("https://cdn.example.com/site/")
        );
    });
}
