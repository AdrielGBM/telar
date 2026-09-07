use super::*;
use crate::context::reset_layout_runtime;
use layout_core::LayoutStyle;

fn tree() -> (NodeId, NodeId, NodeId) {
    reset_layout_runtime();
    with_cascade(|c| *c = Cascade::default());
    let (leaf, _) = layout_reactive::new_leaf(LayoutStyle::new()).unwrap();
    let inner = layout_reactive::new_container(LayoutStyle::new(), &[leaf]).unwrap();
    let outer = layout_reactive::new_container(LayoutStyle::new(), &[inner]).unwrap();
    (outer, inner, leaf)
}

/// The state every tree is in until markup can declare anything: nothing is declared, so every node resolves to the same initial row and shares one value.
#[test]
fn an_undeclared_tree_resolves_every_node_to_the_initial_row() {
    let (outer, inner, leaf) = tree();
    let initial = Inherited::initial();
    for node in [outer, inner, leaf] {
        assert_eq!(*context(node), initial);
    }
    assert!(
        Rc::ptr_eq(&context(outer), &context(leaf)),
        "nodes resolving to the same value must share it rather than each holding a copy"
    );
}

/// The point of the whole thing: an ancestor that draws no text of its own still says what the text beneath it looks like, the way `body { font-size }` does for a body that draws none.
#[test]
fn a_declaration_reaches_a_leaf_that_did_not_ask_for_it() {
    let (outer, _, leaf) = tree();
    declare(outer, Declared::default().with_font_size(11.0));
    assert_eq!(context(leaf).text.font_size, 11.0);
    assert_eq!(
        context(leaf).text.font_weight,
        Inherited::initial().text.font_weight,
        "a declaration says nothing about the properties it did not name"
    );
}

/// Nearer wins, which is the whole of a cascade.
#[test]
fn the_nearest_declaration_wins() {
    let (outer, inner, leaf) = tree();
    declare(outer, Declared::default().with_font_size(11.0));
    declare(inner, Declared::default().with_font_size(22.0));
    assert_eq!(context(leaf).text.font_size, 22.0);
    assert_eq!(context(outer).text.font_size, 11.0);
}

/// Two declarations at different depths compose rather than replace: the inner one names a size and inherits the outer one's weight without restating it.
#[test]
fn declarations_compose_down_the_tree() {
    let (outer, inner, leaf) = tree();
    declare(outer, Declared::default().with_font_weight(700));
    declare(inner, Declared::default().with_font_size(22.0));
    let at_leaf = context(leaf);
    assert_eq!(at_leaf.text.font_weight, 700);
    assert_eq!(at_leaf.text.font_size, 22.0);
}

/// A remount builds a whole new tree, and the runtime hands out the same `NodeId`s again. A declaration that outlived its tree would land on whatever is built on that id next — text the wrong size under a node nobody declared for, which is not a failure anything reports.
#[test]
fn a_new_tree_inherits_nothing_from_the_one_it_replaced() {
    let (outer, _, _) = tree();
    declare(outer, Declared::default().with_font_size(11.0));
    assert_eq!(context(outer).text.font_size, 11.0);

    crate::context::reset_layout_runtime();
    let (leaf, _) = layout_reactive::new_leaf(LayoutStyle::new()).unwrap();
    let inner = layout_reactive::new_container(LayoutStyle::new(), &[leaf]).unwrap();
    let again = layout_reactive::new_container(LayoutStyle::new(), &[inner]).unwrap();

    assert_eq!(
        again, outer,
        "the id was recycled, which is the whole hazard"
    );
    assert_eq!(
        context(leaf).text.font_size,
        Inherited::initial().text.font_size,
        "and the new tree inherits nothing from the one it replaced"
    );
}

/// The reason the root is not a constant: a theme saying "the body text is 11px" says it once, at the top, instead of every leaf having to be told.
#[test]
fn the_theme_is_what_sits_at_the_root() {
    #[derive(Clone)]
    struct Small;
    impl ThemeTokens for Small {
        fn root(&self) -> Declared {
            Declared::default().with_font_size(11.0)
        }
    }

    /// Answers nothing, so it restores the built-in row — and swapping back is the other half of what is being tested, since a root cached against a stale theme would keep serving 11.
    #[derive(Clone)]
    struct Silent;
    impl ThemeTokens for Silent {}

    let (_, _, leaf) = tree();
    let built_in = context(leaf).text.font_size;
    assert_ne!(built_in, 11.0, "the built-in row is not already 11");

    theme_core::set_theme(Small);
    assert_eq!(context(leaf).text.font_size, 11.0);

    theme_core::set_theme(Silent);
    assert_eq!(context(leaf).text.font_size, built_in);
}

/// A memo that outlived the declaration it came from would serve the old value forever, which is the one way lazy resolution can be wrong.
#[test]
fn changing_a_declaration_is_visible_to_everything_under_it() {
    let (outer, _, leaf) = tree();
    declare(outer, Declared::default().with_font_size(11.0));
    assert_eq!(context(leaf).text.font_size, 11.0);
    declare(outer, Declared::default().with_font_size(9.0));
    assert_eq!(context(leaf).text.font_size, 9.0);
    declare(outer, Declared::default());
    assert_eq!(
        context(leaf).text.font_size,
        Inherited::initial().text.font_size
    );
}
