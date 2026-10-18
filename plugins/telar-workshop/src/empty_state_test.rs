use telar::{
    AvailableSpace, ComponentList, DrawCommand, compute_layout, for_each_with_matrix,
    new_container, transform_clip_rect,
};

use super::*;

const EXAMPLES: &[Example] = &[
    Example {
        language: "Rust",
        code: ".label(p.arg(\"label\", \"Save\"))",
    },
    Example {
        language: ".rsx",
        code: "[preview \"Default\"]\nbutton label:\"Save\"",
    },
];

/// Where `needle` is drawn in an empty state laid out in a panel `height` px tall.
fn drawn_at(height: f32, needle: &str) -> telar::Rect {
    telar::install_default_text_metrics();
    telar::reset_layout_runtime();
    let state = EmptyState::new(crate::strings::NO_ARGS)
        .about(crate::strings::NO_ARGS_HINT)
        .examples(EXAMPLES)
        .build()
        .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(800.0).height(height),
        &[state.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(800.0),
        AvailableSpace::Definite(height),
    )
    .unwrap();
    let tree = ComponentList::new(state);
    let mut found = None;
    for_each_with_matrix(&tree.commands(), |command, matrix| {
        if let DrawCommand::Text { text, rect, .. } = command
            && found.is_none()
            && text.contains(needle)
        {
            found = Some(transform_clip_rect(matrix, *rect));
        }
    });
    found.unwrap_or_else(|| panic!("{needle} is not drawn"))
}

#[test]
fn it_says_what_is_missing_what_the_panel_is_for_and_how_to_fill_it() {
    for needle in [
        "This preview has no args",
        "An arg is a value",
        "Rust",
        ".rsx",
        "p.arg(\"label\", \"Save\")",
        "button label:\"Save\"",
    ] {
        drawn_at(800.0, needle);
    }
}

#[test]
fn it_stands_in_the_middle_of_a_panel_with_room_and_at_the_top_of_one_without() {
    let roomy = drawn_at(800.0, "This preview has no args");
    assert!(roomy.y > 200.0, "centred in a tall panel: {roomy:?}");
    let cramped = drawn_at(60.0, "This preview has no args");
    assert!(
        cramped.y >= 0.0 && cramped.y < WORKBENCH_GRID * 3.0,
        "from the top of a short one, rather than cut off above it: {cramped:?}"
    );
}

/// Its lines are kept to a readable measure, and that measure sits in the middle of the panel rather than against its start.
#[test]
fn the_about_text_is_centred_under_the_title() {
    let about = drawn_at(800.0, "An arg is a value");
    assert!(about.width <= ABOUT_MAX_WIDTH);
    let middle = about.x + about.width / 2.0;
    assert!((middle - 400.0).abs() < 1.0, "{about:?}");
}

/// The drawn frames of the Rust and the `.rsx` code blocks, in a panel `width` px wide.
fn blocks(width: f32) -> (telar::Rect, telar::Rect) {
    telar::install_default_text_metrics();
    telar::reset_layout_runtime();
    let examples: &'static [Example] = &[
        Example {
            language: "Rust",
            code: "one line",
        },
        Example {
            language: ".rsx",
            code: "a\nlonger\nblock",
        },
    ];
    let state = EmptyState::new(crate::strings::NO_ARGS)
        .examples(examples)
        .build()
        .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(width).height(800.0),
        &[state.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(width),
        AvailableSpace::Definite(800.0),
    )
    .unwrap();
    let tree = ComponentList::new(state);
    let mut frames = Vec::new();
    for_each_with_matrix(&tree.commands(), |command, matrix| {
        if let DrawCommand::Rect { rect, style } = command
            && style.border.is_some()
        {
            frames.push(transform_clip_rect(matrix, *rect));
        }
    });
    let [rust, rsx] = frames[..] else {
        panic!("two code blocks, not {frames:?}");
    };
    (rust, rsx)
}

#[test]
fn side_by_side_the_blocks_stand_as_tall_as_each_other() {
    let (rust, rsx) = blocks(800.0);
    assert_eq!(rust.y, rsx.y, "on one line");
    assert_eq!(rust.height, rsx.height);
}

#[test]
fn one_under_the_other_the_blocks_stand_as_wide_as_each_other() {
    let (rust, rsx) = blocks(140.0);
    assert!(rsx.y > rust.y, "on lines of their own");
    assert_eq!(rust.width, rsx.width);
}

/// Code wider than the panel scrolls inside its block, rather than standing the block out past the panel's sides.
#[test]
fn code_wider_than_the_panel_keeps_its_block_inside_it() {
    telar::install_default_text_metrics();
    telar::reset_layout_runtime();
    let state = EmptyState::new(crate::strings::NO_PLAY)
        .examples(&[Example {
            language: "Rust",
            code: "canvas.click(by_role(Role::Button).named(\"Save\"))?;",
        }])
        .build()
        .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(240.0).height(400.0),
        &[state.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(240.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let tree = ComponentList::new(state);
    let mut frames = Vec::new();
    for_each_with_matrix(&tree.commands(), |command, matrix| {
        if let DrawCommand::Rect { rect, style } = command
            && style.border.is_some()
        {
            frames.push(transform_clip_rect(matrix, *rect));
        }
    });
    let [block] = frames[..] else {
        panic!("one code block, not {frames:?}");
    };
    assert!(
        block.x >= 0.0 && block.x + block.width <= 240.0,
        "{block:?} in a panel 240 wide"
    );
}
