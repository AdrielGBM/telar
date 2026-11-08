use super::*;

fn warnings(src: &str) -> Vec<String> {
    let doc = telar_parser::parse(src).expect("the probe parses");
    unsigiled_captures(&doc)
        .into_iter()
        .map(|d| d.message)
        .collect()
}

/// The case the compiler catches a whole build later, and answers with the wrong advice for this language.
#[test]
fn a_non_copy_binding_captured_twice_without_its_sigil_is_reported() {
    let found = warnings(
        "[logic]\nlet held = Rc::new(RefCell::new(0));\n\n[view]\ncol\n    button label:\"a\" on_press:(|| { held.take(); })\n    button label:\"b\" on_press:(|| { held.take(); })\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`$held`"), "{}", found[0]);
}

/// Sigiled is the fix, so saying it again would be noise.
#[test]
fn the_sigil_settles_it() {
    let found = warnings(
        "[logic]\nlet held = Rc::new(RefCell::new(0));\n\n[view]\ncol\n    button label:\"a\" on_press:(|| { $held.take(); })\n    button label:\"b\" on_press:(|| { $held.take(); })\n",
    );
    assert!(found.is_empty(), "{found:?}");
}

/// One closure moves it and that is correct, so there is nothing to warn about.
#[test]
fn one_closure_may_take_it() {
    let found = warnings(
        "[logic]\nlet held = Rc::new(RefCell::new(0));\n\n[view]\ncol\n    button label:\"a\" on_press:(|| { held.take(); })\n",
    );
    assert!(found.is_empty(), "{found:?}");
}

/// Nothing here knows types, so the check only fires on constructions whose type the text settles. A `Copy` binding captured by two closures is correct code, and warning about it would be the worse error.
#[test]
fn a_binding_that_might_be_copy_is_left_alone() {
    let found = warnings(
        "[logic]\nlet count = compute();\n\n[view]\ncol\n    button label:\"a\" on_press:(|| { use_it(count); })\n    button label:\"b\" on_press:(|| { use_it(count); })\n",
    );
    assert!(found.is_empty(), "{found:?}");
}

/// The check reads code, not prose. A translation key that happens to spell the binding is not a second closure taking it — the shape every settings form has, and one real project drew 42 warnings from it.
#[test]
fn a_binding_named_inside_a_string_is_not_a_capture() {
    let found = warnings(
        "[logic]\nlet save = Rc::new(|| {});\n\n[view]\ncol\n    label text:(Reactive::of(|| t!(\"settings.save.network\")))\n    button label:\"a\" on_press:save\n",
    );
    assert!(found.is_empty(), "{found:?}");
}
