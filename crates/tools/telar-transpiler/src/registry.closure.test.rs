use super::*;

fn path_fate(path: &str) -> ClosureFate {
    closure_fate(&Callee::Path(path.to_string()))
}

fn method_fate(name: &str) -> ClosureFate {
    closure_fate(&Callee::Method(name.to_string()))
}

#[test]
fn the_keepers_keep_by_their_path_tail() {
    for path in [
        "later",
        "telar::later",
        "Reactive::of",
        "telar::Reactive::of",
        "std::rc::Rc::new",
        "Arc::new",
        "Box::new",
        "memo",
        "derive",
        "derive_pair",
    ] {
        assert_eq!(path_fate(path), ClosureFate::Kept, "{path}");
    }
}

#[test]
fn a_constructor_keeps_what_it_is_handed() {
    for path in ["Some", "Ok", "Wrapper", "Shape::Custom"] {
        assert_eq!(path_fate(path), ClosureFate::Kept, "{path}");
    }
}

#[test]
fn std_runs_its_adaptors_on_the_spot() {
    for path in ["now", "std::array::from_fn", "std::iter::repeat_with"] {
        assert_eq!(path_fate(path), ClosureFate::Now, "{path}");
    }
    for name in [
        "map",
        "filter",
        "unwrap_or_else",
        "and_then",
        "map_or_else",
        "with",
    ] {
        assert_eq!(method_fate(name), ClosureFate::Now, "{name}");
    }
}

#[test]
fn any_other_callee_is_unknown() {
    for path in ["tweened", "my_memo", "Wrapper::build", "Memo::new"] {
        assert_eq!(path_fate(path), ClosureFate::Unknown, "{path}");
    }
    for name in ["on_change", "animate", "new"] {
        assert_eq!(method_fate(name), ClosureFate::Unknown, "{name}");
    }
}

#[test]
fn the_method_list_is_sorted_and_unique() {
    assert!(
        CLOSURE_RUNNING_METHODS.windows(2).all(|w| w[0] < w[1]),
        "keep `CLOSURE_RUNNING_METHODS` sorted, so a name is found at a glance and listed once"
    );
}
