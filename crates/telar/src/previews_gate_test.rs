// A glob import is shadowed by an item of the same name, so `gated` resolves to the gate's item when it expands and to the fallback when it does not.
#[cfg_attr(feature = "previews", allow(unused_imports))]
use fallback::*;

#[cfg_attr(feature = "previews", allow(dead_code))]
mod fallback {
    pub fn gated() -> &'static str {
        "absent"
    }

    pub fn also_gated() -> &'static str {
        "absent"
    }
}

crate::__previews! {
    fn gated() -> &'static str {
        "present"
    }

    fn also_gated() -> &'static str {
        "present"
    }
}

#[cfg(feature = "previews")]
mod with_previews {
    #[test]
    fn the_gate_expands_to_every_item_it_wraps() {
        assert_eq!(super::gated(), "present");
        assert_eq!(super::also_gated(), "present");
    }
}

#[cfg(not(feature = "previews"))]
mod without_previews {
    #[test]
    fn the_gate_expands_to_nothing() {
        assert_eq!(super::gated(), "absent");
        assert_eq!(super::also_gated(), "absent");
    }
}

crate::__without_previews! {
    #[cfg_attr(not(feature = "previews"), allow(dead_code))]
    fn only_without() -> &'static str {
        "without"
    }
}

#[cfg(not(feature = "previews"))]
#[test]
fn the_inverse_gate_keeps_its_items_without_previews() {
    assert_eq!(only_without(), "without");
}
