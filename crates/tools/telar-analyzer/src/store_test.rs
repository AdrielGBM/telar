use super::*;
use telar_transpiler::SourceMap;

fn uri(path: &str) -> Uri {
    path.parse().unwrap()
}

#[test]
fn closing_a_document_drops_its_live_map() {
    let mut store = Store::new();
    let open = uri("file:///crate/src/view.rsx");
    let other = uri("file:///crate/src/other.rsx");
    let view_build = PathBuf::from("/crate/.telar/build/view.rs");
    let other_build = PathBuf::from("/crate/.telar/build/other.rs");
    store.set_live_map(open.clone(), view_build.clone(), SourceMap::default());
    store.set_live_map(other, other_build.clone(), SourceMap::default());

    store.close(&open);

    assert!(store.live_map(&view_build).is_none());
    assert!(store.live_map(&other_build).is_some());
}
