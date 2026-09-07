use super::tests::css;
use super::*;

/// Taffy resolves an absolute child against its parent node; a document resolves it against the nearest positioned ancestor. Left `static`, a box is not one, and what it was meant to overlay it escapes.
#[test]
fn an_ordinary_box_is_a_containing_block_for_its_children() {
    assert!(
        css(LayoutStyle::new()).contains("position:relative;"),
        "every box positions its own children: {}",
        css(LayoutStyle::new())
    );
}

#[test]
fn a_box_that_positions_itself_still_says_so() {
    let out = css(LayoutStyle::new().absolute_fill());
    assert!(out.contains("position:absolute;"), "{out}");
    assert!(!out.contains("position:relative;"), "{out}");
    // Overlaying its parent is the whole point of the inset, and it is what escaped.
    assert!(
        out.contains("top:0px;") && out.contains("left:0px;"),
        "{out}"
    );
}
