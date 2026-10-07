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
                RenderNode::Transform { children, .. }
                | RenderNode::Group { children }
                | RenderNode::Element { children, .. } => children.iter().find_map(text_y),
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
            RenderNode::Transform { children, .. }
            | RenderNode::Group { children }
            | RenderNode::Element { children, .. } => children.iter().find_map(text_y),
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
            RenderNode::Transform { children, .. }
            | RenderNode::Group { children }
            | RenderNode::Element { children, .. } => children.iter().find_map(text_rect),
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
            RenderNode::Transform { children, .. }
            | RenderNode::Group { children }
            | RenderNode::Element { children, .. } => children.iter().find_map(text_y),
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

/// A face that arrives after a text was measured in its fallback — a page's font finishing its download, a face the user picked — has to resize that text on the next layout without anything touching it.
#[test]
fn text_measured_in_a_fallback_is_measured_again_when_its_face_arrives() {
    reset_layout_runtime();
    let text = Text::new(
        || "Wide Words".to_string(),
        LayoutStyle::new(),
        || {
            TextStyle::new(16.0, Color::BLACK).with_font_family(renderer_core::FontFamily::stack([
                renderer_core::FontFamily::Named("Telar Late Arrival".into()),
                renderer_core::FontFamily::Monospace,
            ]))
        },
    )
    .unwrap();
    let node = text.layout_node();
    let root = new_container(LayoutStyle::new().flex_row(), &[node]).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(800.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    let fallback = track_layout(node).unwrap().get().width;

    renderer_text::fonts::add_faces(vec![
        renderer_core::FontAsset::embedded(include_bytes!(
            "../../../renderer/renderer-text/test-fonts/TelarTest.ttf"
        ))
        .named("Telar Late Arrival"),
    ]);
    relayout_if_dirty();
    let arrived = track_layout(node).unwrap().get().width;
    assert_ne!(
        arrived, fallback,
        "the text must be measured again in the face that arrived"
    );
}

/// A row too narrow for its labels squeezes them, but never below their longest word: that is a text's min-content width, and a label squeezed past it is drawn broken mid-word ("Pricin / g") in a box that looked like it had room.
#[test]
fn a_squeezed_row_never_narrows_a_label_below_its_longest_word() {
    reset_layout_runtime();
    let style = || TextStyle::new(16.0, Color::BLACK);
    let labels: [&'static str; 3] = ["Overview", "Pricing", "Team"];
    let texts: Vec<Text> = labels
        .iter()
        .map(|&label| Text::new(move || label.to_string(), LayoutStyle::new(), style).unwrap())
        .collect();
    let tabs: Vec<_> = texts
        .iter()
        .map(|text| {
            new_container(
                LayoutStyle::new().flex_row().padding_horizontal(24.0),
                &[text.layout_node()],
            )
            .unwrap()
        })
        .collect();
    let root = new_container(LayoutStyle::new().flex_row().width(240.0), &tabs).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(240.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    for (label, text) in labels.iter().zip(&texts) {
        let (word, _) = crate::text_metrics::measure_text(label, None, f32::MAX, &style());
        let laid_out = track_layout(text.layout_node()).unwrap().get().width;
        assert!(
            laid_out >= word,
            "{label:?} needs {word}px unbroken and was squeezed to {laid_out}px"
        );
    }
}

/// A style that follows state is measured again when it changes, the way new content is: a size, a weight or an axis read from a signal moves the box, not only the glyphs.
#[test]
fn a_restyled_text_is_measured_again() {
    reset_layout_runtime();
    let size = signal(12.0f32);
    let text = Text::new(
        || "Measured".to_string(),
        LayoutStyle::new(),
        move || TextStyle::new(size.get(), Color::BLACK),
    )
    .unwrap();
    let root = new_container(LayoutStyle::new().flex_row(), &[text.layout_node()]).unwrap();
    let width = || {
        relayout_if_dirty();
        compute_layout(root, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
        track_layout(text.layout_node()).unwrap().get().width
    };
    let small = width();
    size.set(48.0);
    let large = width();
    assert!(large > small * 2.0, "small={small} large={large}");
}

/// A tap on a link run follows it, a tap beside it does nothing, and a drag that starts on it is no tap.
#[test]
fn a_tap_on_a_link_run_follows_it() {
    use platform_core::{
        Destination, Location, PointerButton, PointerSource, location_history,
        receive_location_history,
    };
    use renderer_core::{Declared, Span};
    reset_layout_runtime();
    receive_location_history(vec![Location::root()]);
    let team = Location::root().segment("team");
    let link = Destination::Route(team.clone());
    let mut text = Text::spanned(
        || "Meet the team today".to_string(),
        move || vec![Span::new(9..13, Declared::default()).linking_to(link.clone())],
        LayoutStyle::new(),
        || TextStyle::new(16.0, Color::BLACK),
    )
    .unwrap();
    let root = new_container(LayoutStyle::new().flex_row(), &[text.layout_node()]).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(600.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let rect = track_layout(text.layout_node()).unwrap().get();
    let style = TextStyle::new(16.0, Color::BLACK);
    let before = crate::text_metrics::measure_text("Meet the ", None, 600.0, &style).0;
    let (x, y) = (
        (rect.x + before + 4.0) as f64,
        (rect.y + rect.height / 2.0) as f64,
    );
    let tap = |text: &mut Text, x: f64| {
        text.on_event(&Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        text.on_event(&Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        })
    };
    assert_eq!(tap(&mut text, (rect.x + 2.0) as f64), EventResult::Ignored);
    assert_eq!(location_history(), vec![Location::root()]);
    assert_eq!(tap(&mut text, x), EventResult::Handled);
    assert_eq!(location_history(), vec![Location::root(), team.clone()]);
    assert!(crate::link::activate_run(text.layout_node().into(), 0));
}

/// A size written as a fraction of the surface follows the surface, and the text is measured again at the new size.
#[test]
fn a_size_relative_to_the_surface_follows_it() {
    use renderer_core::TextLength;
    reset_layout_runtime();
    crate::context::set_surface_size(geometry_core::Size::new(1000.0, 600.0));
    let text = Text::declaring(
        || "Name".to_string(),
        LayoutStyle::new(),
        |inherited| {
            inherited.with_font_size_in(
                TextLength::SurfaceWidth(0.1),
                crate::context::use_surface_size(),
            )
        },
    )
    .unwrap();
    let root = new_container(LayoutStyle::new().flex_row(), &[text.layout_node()]).unwrap();
    let width = || {
        relayout_if_dirty();
        compute_layout(root, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
        track_layout(text.layout_node()).unwrap().get().width
    };
    let wide = width();
    crate::context::set_surface_size(geometry_core::Size::new(500.0, 600.0));
    let narrow = width();
    assert!(
        (wide / narrow - 2.0).abs() < 0.2,
        "half the surface, half the name: {wide} -> {narrow}"
    );
}

/// A container that says `em` means the size it inherits, and a text beneath it takes the result.
#[test]
fn em_declared_above_is_of_the_size_it_inherits() {
    use renderer_core::{Declared, TextLength};
    reset_layout_runtime();
    let text = Text::declaring(
        || "x".to_string(),
        LayoutStyle::new(),
        |inherited| inherited,
    )
    .unwrap();
    let node = text.layout_node();
    let middle = new_container(LayoutStyle::new(), &[node]).unwrap();
    let outer = new_container(LayoutStyle::new(), &[middle]).unwrap();
    crate::inherit::declare(outer, Declared::default().with_font_size(20.0));
    crate::inherit::declare(
        middle,
        Declared::default().with_font_size(TextLength::Em(1.5)),
    );
    assert_eq!(crate::inherit::inherited_text_style(node).font_size, 30.0);
}

/// A link run something else is drawn in front of — a sibling on top, the part of the text a viewport has scrolled out of view — is not under the pointer: it takes no link shape, and a release there follows nothing.
#[test]
fn a_covered_link_run_takes_no_shape_and_follows_nothing() {
    use platform_core::{
        Cursor, Destination, Location, PointerButton, PointerSource, location_history,
        receive_location_history,
    };
    use renderer_core::{Declared, Span};
    reset_layout_runtime();
    receive_location_history(vec![Location::root()]);
    let link = Destination::Route(Location::root().segment("team"));
    let mut text = Text::spanned(
        || "Meet the team today".to_string(),
        move || vec![Span::new(9..13, Declared::default()).linking_to(link.clone())],
        LayoutStyle::new(),
        || TextStyle::new(16.0, Color::BLACK),
    )
    .unwrap();
    let root = new_container(LayoutStyle::new().flex_row(), &[text.layout_node()]).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(600.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let rect = track_layout(text.layout_node()).unwrap().get();
    let style = TextStyle::new(16.0, Color::BLACK);
    let before = crate::text_metrics::measure_text("Meet the ", None, 600.0, &style).0;
    let (x, y) = (
        (rect.x + before + 4.0) as f64,
        (rect.y + rect.height / 2.0) as f64,
    );
    let moved = Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    };

    {
        let _covered = crate::pointer::occlude();
        text.on_event(&moved);
    }
    assert_eq!(crate::cursor::requested_cursor(), Cursor::Default);
    text.on_event(&moved);
    assert_eq!(crate::cursor::requested_cursor(), Cursor::Pointer);

    text.on_event(&Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
    let released = {
        let _covered = crate::pointer::occlude();
        text.on_event(&Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        })
    };
    assert_eq!(released, EventResult::Ignored);
    assert_eq!(
        location_history(),
        vec![Location::root()],
        "nothing followed"
    );
}

/// A text maps its case by the rules of the language it is written in: the nearest `lang` said on a box above it, or the active locale when none is.
#[test]
fn a_text_is_written_in_the_language_said_nearest_above_it_or_the_locale() {
    std::thread::spawn(|| {
        reset_layout_runtime();
        i18n_core::set_locale("en");
        let text = Text::declaring(
            || "istanbul".to_string(),
            LayoutStyle::new(),
            |inherited| inherited.with_text_case(renderer_core::TextCase::Upper),
        )
        .unwrap();
        let outer = new_container(LayoutStyle::new(), &[text.layout_node()]).unwrap();
        assert_eq!((text.style)().lang.as_deref(), Some("en"));

        let slot = crate::annotation::slot(outer);
        let mut said = slot.peek();
        said.lang = Some("tr".into());
        slot.set(said);
        assert_eq!((text.style)().lang.as_deref(), Some("tr"));
    })
    .join()
    .unwrap();
}

/// Spans scanned out of the text — as many as it holds right now — over a paragraph that takes its style from the tree above it, measured again as either moves.
#[test]
fn spans_scanned_from_an_inheriting_paragraph_follow_both() {
    use renderer_core::{Declared, Span};
    reset_layout_runtime();
    let content = signal("one two".to_string());
    let text = Text::spanned_declaring(
        move || content.get(),
        move || {
            content.with(|t| {
                t.match_indices("two")
                    .map(|(at, found)| {
                        let range = at as u32..(at + found.len()) as u32;
                        Span::new(range, Declared::default().with_font_size(40.0))
                    })
                    .collect()
            })
        },
        LayoutStyle::new(),
        |inherited| inherited,
    )
    .unwrap();
    let root = new_container(LayoutStyle::new().flex_row(), &[text.layout_node()]).unwrap();
    crate::inherit::declare(root, Declared::default().with_font_size(20.0));
    assert_eq!((text.style)().font_size, 20.0);
    let width = || {
        relayout_if_dirty();
        compute_layout(root, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
        track_layout(text.layout_node()).unwrap().get().width
    };
    let one = width();
    content.set("one two two".to_string());
    assert_eq!(text.spans.as_ref().unwrap()().len(), 2);
    let two = width();
    assert!(two > one, "one={one} two={two}");
    crate::inherit::declare(root, Declared::default().with_font_size(10.0));
    assert!(
        width() < two,
        "the paragraph around the spans shrinks with what it inherits"
    );
}
