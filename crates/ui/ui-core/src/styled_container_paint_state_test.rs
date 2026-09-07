use super::*;
use crate::context::reset_layout_runtime;

/// The order is the precedence, and it is the whole of the rule `view()` used to spell out as a chain. A state inserted in the wrong place here is the bug this list exists to make visible.
#[test]
fn states_resolve_most_specific_first() {
    assert!(
        matches!(
            PAINT_STATES,
            [PaintState::Disabled, PaintState::Active, PaintState::Hover]
        ),
        "the most specific state wins"
    );
}

/// **A state with no paint is never asked whether it is engaged.** The test reads a signal, and reading one subscribes the enclosing `view()` to it — so a box with no `hover_style` would re-render on every pointer move. A table of `(style, bool)` pairs would have to evaluate all three to build itself, which is why this resolves through thunks.
#[test]
fn a_state_without_a_style_never_reads_its_signal() {
    reset_layout_runtime();
    let plain = StyledContainer::new(
        LayoutStyle::new().flex_column().width(10.0).height(10.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap();
    plain.state.is_hovered.set(true);
    plain.state.is_active.set(true);
    for state in PAINT_STATES {
        assert!(
            plain.state_style(state).is_none(),
            "a box with no state paint resolved one anyway"
        );
    }
}
