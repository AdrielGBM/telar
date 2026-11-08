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

/// The line a text leaf draws, in the leaf's own coordinates.
fn drawn_line(node: &RenderNode) -> Option<Rect> {
    match node {
        RenderNode::Primitive(renderer_core::DrawCommand::Text { rect, .. }) => Some(*rect),
        RenderNode::Transform { children, .. }
        | RenderNode::Group { children }
        | RenderNode::Element { children, .. } => children.iter().find_map(drawn_line),
        _ => None,
    }
}

/// The band from the baseline to the cap height of `style`, as the ink of a capital H: `(top, height)` inside one line, and that line's height.
fn cap_band(style: &TextStyle) -> (f32, f32, f32) {
    let (top, height) = crate::text_metrics::measure_ink_bounds("H", 1_000.0, style);
    let (_, line) = crate::text_metrics::measure_text("H", None, 1_000.0, style);
    (top, height, line)
}

/// A text laid out alone in a `width` × `height` box, as the line it draws.
fn line_in_box(content: &'static str, width: f32, height: f32, style: TextStyle) -> Rect {
    laid_in_box(content, width, height, style).0
}

/// A text laid out alone in a `width` × `height` box, as the line it draws and the height layout gave its leaf.
fn laid_in_box(content: &'static str, width: f32, height: f32, style: TextStyle) -> (Rect, f32) {
    let text = Text::new(
        move || content.to_string(),
        LayoutStyle::new().width(width).height(height),
        move || style.clone(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(width).height(height),
        &[text.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(width),
        AvailableSpace::Definite(height),
    )
    .unwrap();
    let leaf = track_layout(text.layout_node()).unwrap().get().height;
    (
        drawn_line(&text.view()).expect("a text leaf draws text"),
        leaf,
    )
}

/// A row of labels of one style, stretched across the row, sits on one baseline whatever capitals, ascenders and descenders each holds.
#[test]
fn a_row_of_labels_shares_one_baseline_whatever_letters_they_have() {
    const LABELS: [&str; 7] = ["Setup", "Simulation", "HOME", "xo", "lily", "gyp", "Ag"];
    reset_layout_runtime();
    let labels: Vec<Text> = LABELS
        .iter()
        .map(|&label| {
            Text::new(
                move || label.to_string(),
                LayoutStyle::new(),
                || TextStyle::new(13.0, Color::BLACK),
            )
            .unwrap()
        })
        .collect();
    let nodes: Vec<_> = labels.iter().map(|label| label.layout_node()).collect();
    let row = new_container(LayoutStyle::new().flex_row().gap(8.0).height(30.0), &nodes).unwrap();
    compute_layout(
        row,
        AvailableSpace::Definite(600.0),
        AvailableSpace::Definite(30.0),
    )
    .unwrap();
    let tops: Vec<f32> = labels
        .iter()
        .zip(&nodes)
        .map(|(label, node)| {
            track_layout(*node).unwrap().get().y
                + drawn_line(&label.view()).expect("a label draws").y
        })
        .collect();
    assert!(
        tops.iter().all(|top| *top == tops[0]),
        "every label's line starts at the same row: {:?}",
        LABELS.iter().zip(&tops).collect::<Vec<_>>()
    );
}

/// The middle of the band from the baseline to the cap height lands on the middle of the box, for any size and any box taller than the line.
#[test]
fn the_cap_band_lands_on_the_middle_of_the_box() {
    reset_layout_runtime();
    for size in [11.0_f32, 13.0, 16.0, 20.0] {
        let style = TextStyle::new(size, Color::BLACK);
        let (top, height, _) = cap_band(&style);
        assert!(height > 0.0, "the reference inks at {size}px");
        for box_height in [28.0_f32, 29.0, 31.0, 40.0, 44.5] {
            let (line, leaf) = laid_in_box("Setup", 200.0, box_height, style.clone());
            let middle = line.y + top + height / 2.0;
            assert!(
                (middle - leaf / 2.0).abs() <= 0.5,
                "at {size}px in a {leaf}px box the cap band's middle is at {middle}"
            );
        }
    }
}

/// Where the box leaves an odd number of rows around the cap band, the spare one goes below it, where descenders hang, never above the capitals.
#[test]
fn an_odd_spare_row_goes_below_the_cap_band() {
    reset_layout_runtime();
    for size in [11.0_f32, 12.0, 13.0, 14.0, 16.0, 20.0] {
        let style = TextStyle::new(size, Color::BLACK);
        let (top, height, line_height) = cap_band(&style);
        let first = (line_height + 2.0).ceil() as u32;
        for box_height in first..first + 12 {
            let box_height = box_height as f32;
            let (line, leaf) = laid_in_box("Setup", 200.0, box_height, style.clone());
            let above = line.y + top;
            let below = leaf - above - height;
            let spare = (leaf - height) as u32 % 2;
            assert_eq!(
                below - above,
                spare as f32,
                "at {size}px a {leaf}px box leaves {above} above the cap band and {below} below"
            );
        }
    }
}

/// A block of several lines centres the span from its first line's cap height to its last line's baseline, so it moves by the same nudge as one line.
#[test]
fn several_lines_centre_the_span_from_the_first_cap_height_to_the_last_baseline() {
    const PARAGRAPH: &str = "Name regions and say what the model is made of";
    reset_layout_runtime();
    let style = TextStyle::new(13.0, Color::BLACK);
    let (top, height, line_height) = cap_band(&style);
    for box_height in [80.0_f32, 81.0, 95.0, 120.0] {
        let (line, leaf) = laid_in_box(PARAGRAPH, 150.0, box_height, style.clone());
        let lines = (line.height / line_height).round();
        assert!(lines >= 2.0, "the paragraph wraps in 150px: {line:?}");
        let middle = line.y + top + ((lines - 1.0) * line_height + height) / 2.0;
        assert!(
            (middle - leaf / 2.0).abs() <= 0.5,
            "{lines} lines in a {leaf}px box put the band's middle at {middle}"
        );
    }
}

/// A style shown lowercased is centred on the same capital as the one written, rather than on a lowercase ascender.
#[test]
fn a_cased_style_is_centred_on_the_band_of_its_face() {
    reset_layout_runtime();
    let written = TextStyle::new(13.0, Color::BLACK);
    let lowered = written
        .clone()
        .with_text_case(renderer_core::TextCase::Lower);
    assert_eq!(
        line_in_box("Setup", 200.0, 30.0, lowered).y,
        line_in_box("Setup", 200.0, 30.0, written).y
    );
}

/// Installs the bundled faces under the families the tests below name, so a test that needs faces apart does not rely on what the machine has.
fn add_bundled_faces() {
    renderer_text::fonts::installed();
    renderer_text::fonts::add_faces(vec![
        renderer_core::FontAsset::embedded(include_bytes!(
            "../../../renderer/renderer-text/test-fonts/TelarTest.ttf"
        ))
        .named("Telar Centred"),
        renderer_core::FontAsset::embedded(include_bytes!(
            "../../../renderer/renderer-text/test-fonts/TelarTestHighAscent.ttf"
        ))
        .named(HIGH_ASCENT),
        renderer_core::FontAsset::embedded(include_bytes!(
            "../../../renderer/renderer-text/test-fonts/TelarTestDeepDescent.ttf"
        ))
        .named(DEEP_DESCENT),
    ]);
}

/// A face that declares room above its capitals, as a platform's UI face does for stacked accents, and more ascent and descent than a line holds.
const HIGH_ASCENT: &str = "Telar Test High Ascent";
/// A face that declares a descent deep enough that centring what it declares lifts its ascenders out of the top of the line box.
const DEEP_DESCENT: &str = "Telar Test Deep Descent";

/// A text whose face changes at the same size measures the band of the face it now has.
#[test]
fn a_text_that_changes_face_is_centred_on_the_new_face() {
    const BOX: f32 = 40.0;
    add_bundled_faces();
    reset_layout_runtime();
    let first = TextStyle::new(13.0, Color::BLACK)
        .with_font_family(renderer_core::FontFamily::Named(DEEP_DESCENT.into()));
    let second = TextStyle::new(13.0, Color::BLACK)
        .with_font_family(renderer_core::FontFamily::Named("Telar Centred".into()));
    let fresh = line_in_box("Setup", 200.0, BOX, second.clone()).y;
    assert_ne!(
        line_in_box("Setup", 200.0, BOX, first.clone()).y,
        fresh,
        "the test needs two faces centred apart"
    );
    let face = signal(first);
    let text = Text::new(
        || "Setup".to_string(),
        LayoutStyle::new().width(200.0).height(BOX),
        move || face.get(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(200.0).height(BOX),
        &[text.layout_node()],
    )
    .unwrap();
    let lay_out = || {
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(BOX),
        )
        .unwrap()
    };
    lay_out();
    drawn_line(&text.view()).expect("the text draws");
    face.set(second);
    lay_out();
    assert_eq!(drawn_line(&text.view()).expect("the text draws").y, fresh);
}

/// Text lands on a whole pixel row whatever its box measures: a glyph drawn half a row down is resampled across two rows and goes soft, so only the vertical axis is snapped.
#[test]
fn text_lands_on_a_whole_pixel_row() {
    reset_layout_runtime();
    for height in [29.0_f32, 30.0, 31.0, 44.5] {
        let y = line_in_box("Setup", 200.0, height, TextStyle::new(13.0, Color::BLACK)).y;
        assert_eq!(y, y.round(), "a {height}px box put the text at {y}");
    }
}

/// Nothing a text draws leaves its leaf: its whole line box is drawn from inside the top and its descenders stay above the bottom, even where the box is exactly one line tall and centring the cap band would push the line out through its top, and whatever ascent and descent its face declares.
#[test]
fn nothing_is_drawn_outside_the_leaf() {
    const TALL: &str = "Agjpy";
    add_bundled_faces();
    reset_layout_runtime();
    for family in [
        renderer_core::FontFamily::SansSerif,
        renderer_core::FontFamily::Named(HIGH_ASCENT.into()),
        renderer_core::FontFamily::Named(DEEP_DESCENT.into()),
    ] {
        for size in [11.0_f32, 13.0, 15.0, 24.0] {
            let style = TextStyle::new(size, Color::BLACK).with_font_family(family.clone());
            let (_, one_line) = crate::text_metrics::measure_text(TALL, None, 200.0, &style);
            let (ink_top, ink_height) =
                crate::text_metrics::measure_ink_bounds(TALL, 200.0, &style);
            assert!(ink_height > 0.0, "{TALL} inks at {size}px in {family:?}");
            for extra in [0.0_f32, 1.0, 2.0, 3.0, 7.0] {
                let (line, leaf) = laid_in_box(TALL, 200.0, one_line + extra, style.clone());
                assert!(
                    line.y >= 0.0 && line.height == one_line,
                    "at {size}px in {family:?} a {leaf}px leaf draws its whole line from its top down: {line:?}"
                );
                assert!(
                    line.y + ink_top >= 0.0 && line.y + ink_top + ink_height <= leaf + 0.01,
                    "at {size}px in {family:?} a {leaf}px leaf inks from {} to {}",
                    line.y + ink_top,
                    line.y + ink_top + ink_height
                );
            }
        }
    }
}

/// A block that wraps in a box its own size starts at the top of it, since the nudge is one line's correction rather than one per line.
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

    let (_, _, one_line) = cap_band(&style());
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
    let y = drawn_line(&text.view()).expect("a text leaf draws text").y;
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
        let set_in = text.set_in().width;
        assert!(
            set_in >= word,
            "{label:?} needs {word}px unbroken and is set in {set_in}px"
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

/// A lowercase name cut by a clip, as a find field cuts it, is laid out as any other text: its face's cap band in the middle of the control, in a system face and in an embedded one.
#[test]
fn clipped_lowercase_text_is_centred_in_a_control_by_its_cap_band() {
    const CONTROL: f32 = 28.0;
    const NAME: &str = "catalogue";
    renderer_text::fonts::installed();
    for family in [
        renderer_core::FontFamily::SansSerif,
        renderer_core::FontFamily::Named("Telar Centred".into()),
    ] {
        if family != renderer_core::FontFamily::SansSerif {
            renderer_text::fonts::add_faces(vec![
                renderer_core::FontAsset::embedded(include_bytes!(
                    "../../../renderer/renderer-text/test-fonts/TelarTest.ttf"
                ))
                .named("Telar Centred"),
            ]);
        }
        reset_layout_runtime();
        let style = move || {
            TextStyle::new(13.0, Color::BLACK)
                .with_font_family(family.clone())
                .with_text_wrap(renderer_core::TextWrap::NoWrap)
        };
        let text = Text::new(
            || NAME.to_string(),
            LayoutStyle::new().min_width(0.0),
            style.clone(),
        )
        .unwrap();
        let clipped = crate::ClippedItem::new(Box::new(text), crate::Clip::x());
        let row = new_container(
            LayoutStyle::new()
                .flex_row()
                .align_items(layout_core::AlignItems::CENTER)
                .height(CONTROL),
            &[clipped.layout_node()],
        )
        .unwrap();
        compute_layout(
            row,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(CONTROL),
        )
        .unwrap();
        let tree = crate::ComponentList::new(clipped);
        let mut line_top = None;
        renderer_core::for_each_with_matrix(&tree.commands(), |command, matrix| {
            if let renderer_core::DrawCommand::Text { text, rect, .. } = command
                && text.as_ref() == NAME
            {
                line_top = Some(matrix[5] + rect.y);
            }
        });
        let line_top = line_top.expect("the name is drawn");
        let (band_top, band_height, _) = cap_band(&style());
        let centre = line_top + band_top + band_height / 2.0;
        assert!(
            (centre - CONTROL / 2.0).abs() <= 0.5,
            "{:?}: the cap band is centred at {centre} in a {CONTROL}px control",
            style().font_family
        );
    }
}

fn drawn_style(item: impl LayoutItem + 'static) -> TextStyle {
    compute_layout(
        item.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let tree = ui_tree::ComponentList::new(item);
    tree.commands()
        .iter()
        .find_map(|command| match command {
            renderer_core::DrawCommand::Text { style, .. } => Some((**style).clone()),
            _ => None,
        })
        .expect("the text is drawn")
}

fn titled() -> Text {
    Text::declaring(
        || "Title".to_string(),
        LayoutStyle::new(),
        |inherited| {
            let size = inherited.font_size * 2.0;
            inherited.with_font_size(size).with_font_weight(700)
        },
    )
    .unwrap()
}

/// What the call placing a text authors lands after the text's own style, so it wins where a declaration around it would lose: the weight and size given replace the ones the widget chose, and what it did not name stays the widget's.
#[test]
fn what_the_call_authors_wins_over_the_text_s_own_style() {
    use crate::inherit::AuthoredText;
    use renderer_core::Declared;
    reset_layout_runtime();
    let text = titled().authored_text(|| {
        Declared::default()
            .with_font_weight(400)
            .with_font_size(20.0)
    });
    let style = drawn_style(text);
    assert_eq!((style.font_weight, style.font_size), (400, 20.0));
}

/// An authored `em` is of the size the text was handed, not of the one its own style worked out: `1.5em` on a title is half again the text around it.
#[test]
fn an_authored_em_is_of_the_size_the_text_was_handed() {
    use crate::inherit::AuthoredText;
    use renderer_core::{Declared, TextLength};
    reset_layout_runtime();
    let handed = crate::inherit::Inherited::initial().text.font_size;
    let text = titled().authored_text(|| Declared::default().with_font_size(TextLength::Em(1.5)));
    assert_eq!(drawn_style(text).font_size, handed * 1.5);
}

/// A container's authoring is inherited the way a declaration is, and a text inside still has the last word on what it names itself, as a style on an element in CSS is inherited by its children without overruling theirs.
#[test]
fn what_a_container_authors_is_inherited_and_a_text_inside_still_overrides_it() {
    use crate::inherit::AuthoredText;
    use renderer_core::Declared;
    reset_layout_runtime();
    let text = titled();
    let row = crate::Container::new(LayoutStyle::new(), vec![crate::box_item(text)])
        .unwrap()
        .authored_text(|| {
            Declared::default()
                .with_font_weight(300)
                .with_letter_spacing(2.0)
        });
    let style = drawn_style(row);
    assert_eq!(style.font_weight, 700);
    assert_eq!(style.letter_spacing, 2.0);
}
