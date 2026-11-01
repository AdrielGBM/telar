use std::sync::Arc;

use reactive_core::{Reactive, signal};
use renderer_assets::SvgData;

use super::IconName;

#[test]
fn the_baked_triple_becomes_a_baked_icon_that_keeps_its_id() {
    let svg = Arc::new(SvgData::from_baked_vector((1.0, 1.0), Vec::new()));
    let name = IconName::from(("lucide:x", Arc::clone(&svg), true));
    let IconName::Baked {
        id,
        svg: baked,
        monochrome,
    } = &name
    else {
        panic!("a baked triple is a baked icon");
    };
    assert_eq!(*id, "lucide:x");
    assert!(Arc::ptr_eq(baked, &svg));
    assert!(*monochrome);
    assert_eq!(name.id(), "lucide:x");
}

#[test]
fn a_string_or_a_signal_is_a_named_icon_whose_id_follows_its_value() {
    assert!(
        matches!(IconName::from("lucide:x"), IconName::Named(Reactive::Const(id)) if id == "lucide:x")
    );

    let which = signal("lucide:x".to_string());
    let name = IconName::from(which);
    assert_eq!(name.id(), "lucide:x");
    which.set("lucide:search".to_string());
    assert_eq!(name.id(), "lucide:search");
}
