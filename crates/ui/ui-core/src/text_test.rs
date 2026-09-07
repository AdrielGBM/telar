use super::*;
use crate::context::{
    compute_layout, new_container, relayout_if_dirty, reset_layout_runtime, track_layout,
};
use crate::layout_item::LayoutItem;
use layout_core::AvailableSpace;
use reactive_core::signal;
use renderer_core::Color;

// Narrower auto-height text wraps to more lines and must reserve the space, or following content overlaps it.
#[test]
fn auto_text_height_grows_when_narrower() {
    let long = "This is a deliberately long paragraph of text that wraps onto several \
                lines when the available width is small, and fewer lines when it is wide.";
    let height_at = |w: f32| -> f32 {
        reset_layout_runtime();
        let t = Text::new(
            move || long.to_string(),
            LayoutStyle::new(),
            || TextStyle::new(16.0, Color::BLACK),
        )
        .unwrap();
        let node = t.layout_node();
        compute_layout(
            node,
            AvailableSpace::Definite(w),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        track_layout(node).unwrap().get().height
    };
    let narrow = height_at(200.0);
    let wide = height_at(800.0);
    assert!(
        narrow > wide + 20.0,
        "narrow text should be taller: narrow={narrow} wide={wide}"
    );
}

/// Two labels of the same style sit on the same baseline, whatever letters they happen to contain.
///
/// They did not: the optical centring measured each string's own ink, and a string with a descender has ink reaching lower than one without — so `Setup` and `Simulation`, side by side in a row of tabs at the same size in the same box, were drawn 2.5px apart. Measuring the band from a reference run makes the offset a property of the font at that size, which every label in the style then shares.
#[test]
fn two_labels_of_one_style_share_a_baseline_whatever_letters_they_have() {
    reset_layout_runtime();
    let drawn = |content: &'static str| {
        let text = Text::new(
            move || content.to_string(),
            LayoutStyle::new().width(200.0).height(30.0),
            || TextStyle::new(13.0, Color::BLACK),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(200.0).height(30.0),
            &[text.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(30.0),
        )
        .unwrap();
        // The leaf places itself with a transform, so the text command sits under it.
        fn text_y(node: &RenderNode) -> Option<f32> {
            match node {
                RenderNode::Primitive(renderer_core::DrawCommand::Text { rect, .. }) => {
                    Some(rect.y)
                }
                RenderNode::Transform { children, .. } | RenderNode::Group { children } => {
                    children.iter().find_map(text_y)
                }
                _ => None,
            }
        }
        text_y(&text.view()).expect("a text leaf draws text")
    };
    // `Setup` has a descender and `Simulation` has none — the exact pair that drifted.
    let (with_tail, without) = (drawn("Setup"), drawn("Simulation"));
    assert!(
        (with_tail - without).abs() < 0.01,
        "a descender must not move the line: {with_tail} vs {without}"
    );
}

/// Text lands on a whole pixel row, whatever its box measures.
///
/// Centring puts it on a half pixel whenever the box and the text differ by an odd amount, and a glyph drawn half a row down does not move — it is resampled across two rows and goes **soft**. It read as a *placement* bug: the same bubble was crisp beside a button and blurred under one, because the sideways placement centres on its trigger and happened to contribute a second half pixel that cancelled this one. Only the vertical axis is snapped; horizontal subpixel placement is what keeps letter spacing even, and the shaper bins it deliberately.
#[test]
fn text_lands_on_a_whole_pixel_row() {
    fn text_y(node: &RenderNode) -> Option<f32> {
        match node {
            RenderNode::Primitive(renderer_core::DrawCommand::Text { rect, .. }) => Some(rect.y),
            RenderNode::Transform { children, .. } | RenderNode::Group { children } => {
                children.iter().find_map(text_y)
            }
            _ => None,
        }
    }
    reset_layout_runtime();
    // Odd and fractional box heights, where an unsnapped centre lands on the half.
    for height in [29.0_f32, 30.0, 31.0, 44.5] {
        let text = Text::new(
            || "Setup".to_string(),
            LayoutStyle::new().width(200.0).height(height),
            || TextStyle::new(13.0, Color::BLACK),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(200.0).height(height),
            &[text.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(height),
        )
        .unwrap();
        let y = text_y(&text.view()).expect("a text leaf draws text");
        assert_eq!(y, y.round(), "a {height}px box put the text at {y}");
    }
}

/// The optical nudge never walks the glyphs out of the box the layout reserved for them.
///
/// The nudge centres the glyph band, and a box no taller than one line has nothing to centre in — so applying it there put the text at a negative `y`. A `pad:6` status line then inked at row 5, one row above its own padding, which is how an out-of-tree app found this.
#[test]
fn text_stays_inside_a_box_that_is_exactly_one_line_tall() {
    fn text_rect(node: &RenderNode) -> Option<Rect> {
        match node {
            RenderNode::Primitive(renderer_core::DrawCommand::Text { rect, .. }) => Some(*rect),
            RenderNode::Transform { children, .. } | RenderNode::Group { children } => {
                children.iter().find_map(text_rect)
            }
            _ => None,
        }
    }
    reset_layout_runtime();
    for size in [11.0_f32, 13.0, 15.0, 24.0] {
        let text = Text::new(
            || "Ag".to_string(),
            LayoutStyle::new().width(200.0),
            move || TextStyle::new(size, Color::BLACK),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(200.0),
            &[text.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let rect = text_rect(&text.view()).expect("a text leaf draws text");
        assert!(
            rect.y >= 0.0,
            "at {size}px the text starts {}px above its own box",
            -rect.y
        );
    }
}

/// A block that wraps sits where its box is, not half a line below it.
///
/// Optical centring works on the glyph band, and the band is measured from a one-line reference — so centring an N-line block against it pushes the block down by (N-1)/2 lines. A two-line tooltip description came out sunk, with its second line hanging out of the bubble. Centring the *block* and nudging by the one-line correction is what makes the two cases the same case.
#[test]
fn a_wrapped_block_is_not_pushed_down_by_the_lines_it_gained() {
    reset_layout_runtime();
    let style = || TextStyle::new(13.0, Color::BLACK);
    let text = Text::new(
        || "Name regions and say what the model is made of".to_string(),
        LayoutStyle::new(),
        style,
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(150.0),
        &[text.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(150.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();

    let (_, one_line) = crate::text_metrics::measure_text("Hxg", None, 1_000.0, &style());
    let (_, block) = crate::text_metrics::measure_text(
        "Name regions and say what the model is made of",
        None,
        150.0,
        &style(),
    );
    assert!(
        block > one_line * 1.5,
        "the test needs a string that actually wraps"
    );

    fn text_y(node: &RenderNode) -> Option<f32> {
        match node {
            RenderNode::Primitive(renderer_core::DrawCommand::Text { rect, .. }) => Some(rect.y),
            RenderNode::Transform { children, .. } | RenderNode::Group { children } => {
                children.iter().find_map(text_y)
            }
            _ => None,
        }
    }
    let y = text_y(&text.view()).expect("a text leaf draws text");
    assert!(
        y.abs() < one_line / 3.0,
        "a block in a box its own size starts at the top: y = {y} against a {one_line}px line"
    );
}

/// A label that grows re-measures, instead of being shaped into the width the previous string wanted.
///
/// The regression it guards is invisible in the widget tree and obvious on screen: a measured leaf is dirtied by the layout runtime, never by a content closure, so a bar chip whose title went from "Desktop" to a full window title kept the narrow box the short one had measured — and `view` soft-wrapped the long title into it, spilling several lines out of a chip one line tall.
#[test]
fn a_measured_label_re_measures_when_its_content_changes() {
    reset_layout_runtime();
    let title = signal(String::from("Desktop"));
    let read = title.read_only();
    let label = Text::new(
        move || read.get(),
        LayoutStyle::new(),
        || TextStyle::new(13.0, Color::BLACK),
    )
    .unwrap();
    let node = label.layout_node();
    let root = new_container(
        LayoutStyle::new().flex_row().width(1920.0).height(32.0),
        &[node],
    )
    .unwrap();
    let space = || {
        compute_layout(
            root,
            AvailableSpace::Definite(1920.0),
            AvailableSpace::Definite(32.0),
        )
        .unwrap()
    };

    space();
    let short = label.leaf.rect.get().width;

    title.set("hyprshell - Rust - Visual Studio Code".to_string());
    relayout_if_dirty();
    let long = label.leaf.rect.get().width;

    assert!(
        long > short,
        "a title five times longer still measured {long}px, the width \"Desktop\" wanted ({short}px) — \
         it will be wrapped into a box built for the old text"
    );
}
