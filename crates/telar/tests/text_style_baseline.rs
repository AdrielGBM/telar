//! What a text style resolves to today, written down before anything resolves it differently.
//!
//! The inheritance work replaces four independent default systems with one initial-value table, and its whole
//! job is to change nothing: an application that declares no style must draw exactly what it draws now. That
//! is only checkable against a record of what it draws now, and this is it.
//!
//! Asserted on `DrawCommand`s rather than pixels, which is the distinction that makes the net useful: a
//! resolved `TextStyle` is directly comparable, where a screenshot answers "does this look right" and never
//! "which weight did it decide on". Every number here is a *current* value, not a desired one — a change to
//! any of them is either a regression or a decision, and both should have to be made on purpose.

use telar::testing::mount;
use telar::{
    Color, Component, Container, DrawCommand, FontFamily, GlyphRaster, LayoutItem, LayoutStyle,
    RenderNode, Text, TextAlign, TextStyle, box_item,
};

/// A tree of one text, mounted and drawn, with the style it actually resolved to.
fn resolved(build: impl FnOnce() -> Box<dyn LayoutItem>) -> TextStyle {
    struct Root(Box<dyn LayoutItem>);
    impl Component for Root {
        fn view(&self) -> RenderNode {
            self.0.view()
        }
        fn on_event(&mut self, _: &platform_core::Event) -> ui_core::EventResult {
            ui_core::EventResult::Ignored
        }
        fn debug_name(&self) -> &'static str {
            "BaselineRoot"
        }
    }
    // A tree measures its text as it lays out, and a test has no runner to have installed a measurer.
    telar::install_default_text_metrics();
    let tree = mount(Root(build()), 400, 200);
    let style = tree.commands().iter().find_map(|c| match c {
        DrawCommand::Text { style, .. } => Some((**style).clone()),
        _ => None,
    });
    style.expect("the tree drew a text command")
}

/// Every field of a `TextStyle` nobody has said anything about. This is the row `Inherited::initial()` has to
/// reproduce, and the one an application declaring nothing renders against.
#[test]
fn an_undeclared_text_style_resolves_to_these_exact_values() {
    let style = resolved(|| {
        box_item(
            Text::new(
                || "baseline".to_string(),
                LayoutStyle::new(),
                || TextStyle::new(14.0, Color::BLACK),
            )
            .unwrap(),
        )
    });

    assert_eq!(style.font_size, 14.0);
    assert_eq!(style.paint, telar::Paint::Solid(Color::BLACK));
    assert_eq!(style.font_family, FontFamily::SansSerif);
    assert_eq!(style.weight, 400);
    assert!(!style.italic);
    assert_eq!(style.align, TextAlign::Start);
    assert_eq!(style.max_lines, None);
    assert!(!style.ellipsis);
    assert_eq!(style.line_height, None);
    assert_eq!(style.letter_spacing, 0.0);
    assert_eq!(style.raster, GlyphRaster::Smooth);
    assert!(!style.no_wrap);
    assert_eq!(style.shadow, None);
}

/// A style declared on a leaf reaches the command unchanged. Nothing between the widget and the renderer may
/// quietly amend it — which is exactly what a resolve pass inserted in the middle could start doing.
#[test]
fn a_declared_text_style_reaches_the_draw_command_intact() {
    let declared = TextStyle::new(11.0, Color::WHITE)
        .with_weight(700)
        .with_italic(true)
        .with_align(TextAlign::Center)
        .with_max_lines(2)
        .with_ellipsis(true)
        .with_line_height(1.5)
        .with_letter_spacing(0.5)
        .with_raster(GlyphRaster::Pixel)
        .with_no_wrap(true)
        .with_font_family("Some Face");

    let expected = declared.clone();
    let style = resolved(move || {
        let declared = declared.clone();
        box_item(
            Text::new(
                || "declared".to_string(),
                LayoutStyle::new(),
                move || declared.clone(),
            )
            .unwrap(),
        )
    });
    assert_eq!(style, expected);
}

/// **A nested text inherits nothing today**, and that is the point of recording it: a `col` cannot say
/// anything about the text inside it, so a leaf two containers deep resolves to the same row as one at the
/// root. When inheritance lands, a tree that declares nothing must still land here.
#[test]
fn nesting_changes_nothing_about_a_text_style() {
    let style = resolved(|| {
        let leaf = Text::new(
            || "nested".to_string(),
            LayoutStyle::new(),
            || TextStyle::new(14.0, Color::BLACK),
        )
        .unwrap();
        let inner = Container::new(LayoutStyle::new().flex_column(), vec![box_item(leaf)]).unwrap();
        let outer =
            Container::new(LayoutStyle::new().flex_column(), vec![box_item(inner)]).unwrap();
        box_item(outer)
    });

    assert_eq!(style.font_size, 14.0);
    assert_eq!(style.weight, 400);
    assert_eq!(style.font_family, FontFamily::SansSerif);
    assert_eq!(style.raster, GlyphRaster::Smooth);
}

/// The §1.4 split, pinned so the fix is visible when it happens: a `.rsx` `text` bakes `14.0` at transpile
/// time while the catalogue derives its size from the theme, so a control's label and a plain label beside it
/// are two different sizes with nothing naming the difference. Both numbers are recorded here; the step that
/// gives a theme one place to say "body text is 11px" is the step that makes this assertion wrong on purpose.
#[test]
fn the_markup_default_and_the_catalogue_default_are_two_different_numbers() {
    let markup = resolved(|| {
        box_item(
            Text::new(
                || "label".to_string(),
                LayoutStyle::new(),
                || TextStyle::new(14.0, Color::BLACK),
            )
            .unwrap(),
        )
    });
    assert_eq!(
        markup.font_size, 14.0,
        "the transpiler bakes this literal into every `text` with no `size:`"
    );

    // The catalogue's own base, times the ratio `text_field` applies to it. Read through the same public
    // surface an application would, so the day a theme can set it, this stops matching and says so.
    let catalogue = 14.0_f32 * 1.07;
    assert!(
        (catalogue - markup.font_size).abs() > 0.5,
        "the two defaults are supposed to differ today: {catalogue} vs {}",
        markup.font_size
    );
}

/// The headline case, and the one nothing could express before: an ancestor that draws no text of its own
/// says what the text beneath it looks like. Asserted through a real frame rather than the cascade's own unit
/// tests, because the thing being checked is that a `Text` built the ordinary way actually reads it — and
/// re-reads it, since the declaration lands *after* the leaf has already rendered once, which is the order a
/// tree is built in.
#[test]
fn a_container_can_say_what_the_text_below_it_looks_like() {
    telar::install_default_text_metrics();

    struct Root(Box<dyn LayoutItem>);
    impl Component for Root {
        fn view(&self) -> RenderNode {
            self.0.view()
        }
        fn on_event(&mut self, _: &platform_core::Event) -> ui_core::EventResult {
            ui_core::EventResult::Ignored
        }
        fn debug_name(&self) -> &'static str {
            "DeclaringRoot"
        }
    }

    let leaf = Text::declaring(|| "inherits".to_string(), LayoutStyle::new(), |t| t).unwrap();
    let outer = Container::new(LayoutStyle::new().flex_column(), vec![box_item(leaf)]).unwrap();
    let outer_node = outer.layout_node();
    ui_core::declare(
        outer_node,
        telar::Declared::default()
            .with_font_size(11.0)
            .with_weight(700),
    );

    let tree = mount(Root(box_item(outer)), 400, 200);
    let drawn = |tree: &ui_core::ComponentList| {
        tree.commands()
            .iter()
            .find_map(|c| match c {
                DrawCommand::Text { style, .. } => Some((**style).clone()),
                _ => None,
            })
            .expect("the tree drew a text command")
    };

    let style = drawn(&tree);
    assert_eq!(style.font_size, 11.0, "the leaf takes the declared size");
    assert_eq!(style.weight, 700, "and the declared weight");
    assert_eq!(
        style.raster,
        telar::GlyphRaster::Smooth,
        "and keeps the initial value of everything the declaration did not name"
    );

    // A changed declaration has to reach the leaf too, or a theme switch would resolve right and never repaint.
    ui_core::declare(outer_node, telar::Declared::default().with_font_size(9.0));
    assert_eq!(
        drawn(&tree).font_size,
        9.0,
        "a changed declaration repaints"
    );

    ui_core::undeclare(outer_node);
    assert_eq!(
        drawn(&tree).font_size,
        14.0,
        "and withdrawing it returns the leaf to the initial row"
    );
}
