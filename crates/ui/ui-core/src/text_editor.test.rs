use std::cell::RefCell;

use layout_core::AvailableSpace;
use platform_core::ScrollDelta;
use reactive_core::signal;
use renderer_core::DrawCommand;
use ui_tree::Component;

use super::*;
use crate::context::{compute_layout, new_container, relayout_if_dirty, reset_layout_runtime};
use crate::layout_item::LayoutItem;
use crate::text_edit::DOUBLE_CLICK;
use crate::{Input, TextArea};

fn style() -> TextStyle {
    TextStyle::new(14.0, Color::BLACK)
}

fn line_h() -> f32 {
    crate::text_metrics::line_box(&style())
}

fn advance(text: &str) -> f32 {
    let flat = style().with_text_wrap(TextWrap::NoWrap);
    crate::text_metrics::measure_text(text, None, 1.0e6, &flat).0
}

fn key(k: Key) -> Event {
    Event::KeyPressed {
        key: k,
        modifiers: ModifiersState::default(),
        unmodified: None,
    }
}

fn shift(held: bool) {
    crate::keyboard::observe(&Event::ModifiersChanged {
        modifiers: ModifiersState {
            is_shift: held,
            ..ModifiersState::default()
        },
    });
}

/// Every primitive the editor draws, in its own coordinates.
fn commands(view: &RenderNode) -> Vec<DrawCommand> {
    fn walk(node: &RenderNode, out: &mut Vec<DrawCommand>) {
        match node {
            RenderNode::Primitive(command) => out.push(command.clone()),
            RenderNode::Group { children }
            | RenderNode::Element { children, .. }
            | RenderNode::Transform { children, .. }
            | RenderNode::Clip { children, .. }
            | RenderNode::Layer { children, .. } => {
                for child in children.iter() {
                    walk(child, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(view, &mut out);
    out
}

fn drawn_text(view: &RenderNode) -> (Rect, TextStyle) {
    commands(view)
        .into_iter()
        .find_map(|command| match command {
            DrawCommand::Text { rect, style, .. } => Some((rect, (*style).clone())),
            _ => None,
        })
        .expect("the editor draws its text")
}

fn rects(view: &RenderNode, height: f32) -> Vec<Rect> {
    commands(view)
        .into_iter()
        .filter_map(|command| match command {
            DrawCommand::Rect { rect, .. } if (rect.height - height).abs() < 0.01 => Some(rect),
            _ => None,
        })
        .collect()
}

fn caret_of(editor: &Editor) -> Rect {
    commands(&editor.view())
        .into_iter()
        .find_map(|command| match command {
            DrawCommand::Rect { rect, .. } if rect.width == CARET_WIDTH => Some(rect),
            _ => None,
        })
        .expect("a focused editor draws its caret")
}

fn clip_of(view: &RenderNode) -> Option<Rect> {
    match view {
        RenderNode::Clip { rect, .. } => Some(*rect),
        RenderNode::Group { children }
        | RenderNode::Element { children, .. }
        | RenderNode::Transform { children, .. }
        | RenderNode::Layer { children, .. } => children.iter().find_map(clip_of),
        _ => None,
    }
}

/// `item` laid out in a column `width` wide, at the top-left of the surface.
fn laid_out(item: &dyn LayoutItem, width: f32) {
    let root = new_container(
        LayoutStyle::new().flex_column().width(width).height(400.0),
        &[item.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(width),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
}

fn field(initial: &str, width: f32) -> (Input, RwSignal<String>) {
    reset_layout_runtime();
    focus::clear();
    let value = signal(initial.to_string());
    let input = Input::new(value, LayoutStyle::new().width(width).height(24.0), style).unwrap();
    laid_out(&input, width);
    focus::request(input.editor.id);
    reactive_core::batch(|| {});
    (input, value)
}

fn area(initial: &str, width: f32) -> (TextArea, RwSignal<String>) {
    reset_layout_runtime();
    focus::clear();
    let value = signal(initial.to_string());
    let area = TextArea::new(value, LayoutStyle::new(), style).unwrap();
    laid_out(&area, width);
    focus::request(area.editor.id);
    reactive_core::batch(|| {});
    (area, value)
}

fn press_at(editor: &mut Editor, x: f32, y: f32) {
    let at = editor.leaf.rect.get();
    editor.on_event(&Event::PointerPressed {
        x: (at.x + x) as f64,
        y: (at.y + y) as f64,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
}

fn move_to(editor: &mut Editor, x: f32, y: f32) {
    let at = editor.leaf.rect.get();
    editor.on_event(&Event::PointerMoved {
        x: (at.x + x) as f64,
        y: (at.y + y) as f64,
        source: PointerSource::Mouse,
    });
}

fn release(editor: &mut Editor) {
    editor.on_event(&Event::PointerReleased {
        x: 0.0,
        y: 0.0,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
}

fn click(editor: &mut Editor, x: f32, y: f32) {
    press_at(editor, x, y);
    release(editor);
}

/// The x a press lands in the middle of the character at byte `at`, on a field's one line.
fn middle_of(text: &str, at: usize) -> f32 {
    let next = crate::text_edit::next_boundary(text, at);
    (advance(&text[..at]) + advance(&text[..next])) / 2.0
}

fn selected(editor: &Editor) -> Option<String> {
    let text = editor.value.get();
    editor
        .selection(&text)
        .map(|(from, to)| text[from..to].to_string())
}

const LONG: &str = "una línea mucho más larga que el campo donde se escribe";

/// **A field is one line, whatever it holds.** A value longer than the box is drawn on one line that does not wrap, cut at the box's edges, and scrolled under it so the caret at its end is in the box.
#[test]
fn a_long_value_stays_on_one_line_and_scrolls_to_keep_the_caret_in_the_box() {
    let (input, _) = field(LONG, 120.0);
    let view = input.view();
    let (text, drawn) = drawn_text(&view);
    assert_eq!(drawn.text_wrap, TextWrap::NoWrap, "a field never wraps");
    assert_eq!(
        clip_of(&view),
        Some(Rect::new(0.0, 0.0, 120.0, 24.0)),
        "and is cut to its box"
    );
    let caret = caret_of(&input.editor);
    assert!(
        caret.x >= 0.0 && caret.x + caret.width <= 120.0 + 0.01,
        "the caret at the end is in the box: {caret:?}"
    );
    let scrolled = input.editor.shape.scroll_x.get();
    assert!(scrolled > 0.0, "the text scrolled under the box");
    assert!(
        (text.x + scrolled).abs() < 0.01,
        "the text is drawn where it scrolled to: {text:?} for {scrolled}"
    );
    assert!((caret.x - (advance(LONG) - scrolled)).abs() < 0.01);
}

#[test]
fn home_scrolls_back_to_the_start_and_typing_at_the_end_scrolls_on() {
    let (mut input, _) = field(LONG, 120.0);
    input.on_event(&key(Key::Named(NamedKey::Home)));
    reactive_core::batch(|| {});
    assert_eq!(input.editor.shape.scroll_x.get(), 0.0);
    assert_eq!(caret_of(&input.editor).x, 0.0);
    input.on_event(&key(Key::Named(NamedKey::End)));
    reactive_core::batch(|| {});
    let at_end = input.editor.shape.scroll_x.get();
    input.on_event(&key(Key::Char('!')));
    reactive_core::batch(|| {});
    assert!(
        input.editor.shape.scroll_x.get() > at_end,
        "the letter typed past the edge pulls the text along"
    );
    let caret = caret_of(&input.editor);
    assert!(caret.x + caret.width <= 120.0 + 0.01, "{caret:?}");
}

/// A field nobody is typing into shows how its value begins.
#[test]
fn a_field_that_loses_the_keyboard_shows_the_start_of_its_value() {
    let (input, _) = field(LONG, 120.0);
    assert!(input.editor.shape.scroll_x.get() > 0.0);
    focus::clear();
    reactive_core::batch(|| {});
    assert_eq!(input.editor.shape.scroll_x.get(), 0.0);
    assert_eq!(drawn_text(&input.view()).0.x, 0.0);
}

/// What an input method composes past the edge is in view too, and what it marks is drawn where the letters are.
#[test]
fn a_composition_past_the_edge_scrolls_into_view() {
    let (mut input, _) = field("abc", 120.0);
    let preedit = "にほんごのぶんしょうをにゅうりょくする";
    input.on_event(&Event::Ime(Ime::preedit(
        preedit,
        Some((preedit.len(), preedit.len())),
    )));
    reactive_core::batch(|| {});
    let scrolled = input.editor.shape.scroll_x.get();
    assert!(scrolled > 0.0);
    let caret = caret_of(&input.editor);
    assert!(caret.x + caret.width <= 120.0 + 0.01, "{caret:?}");
    let underline = rects(&input.view(), 1.0);
    assert_eq!(underline.len(), 1);
    assert!(
        (underline[0].x - (advance("abc") - scrolled)).abs() < 0.01,
        "{underline:?}"
    );
}

/// A horizontal wheel or swipe over a field whose value runs past it scrolls the value, and stops at its ends.
#[test]
fn a_sideways_wheel_scrolls_a_long_value_and_stops_at_its_ends() {
    let (mut input, _) = field(LONG, 120.0);
    let at = input.editor.leaf.rect.get();
    let wheel = |dx: f32| Event::Scrolled {
        delta: ScrollDelta::Pixels { x: dx, y: 0.0 },
        x: (at.x + 10.0) as f64,
        y: (at.y + 10.0) as f64,
    };
    let start = input.editor.shape.scroll_x.get();
    assert_eq!(input.on_event(&wheel(30.0)), EventResult::Handled);
    assert!((input.editor.shape.scroll_x.get() - (start - 30.0)).abs() < 0.01);
    input.on_event(&wheel(1.0e4));
    assert_eq!(input.editor.shape.scroll_x.get(), 0.0);
    assert_eq!(
        input.on_event(&wheel(10.0)),
        EventResult::Ignored,
        "at the start the wheel is handed on"
    );
    let vertical = Event::Scrolled {
        delta: ScrollDelta::Pixels { x: 0.0, y: -40.0 },
        x: (at.x + 10.0) as f64,
        y: (at.y + 10.0) as f64,
    };
    assert_eq!(
        input.on_event(&vertical),
        EventResult::Ignored,
        "a page scrolling past it is not the field's"
    );
}

/// **A press puts the caret where it lands.** Before or after the letter under it, whichever side is nearer.
#[test]
fn a_press_puts_the_caret_beside_the_letter_under_it() {
    let text = "mañana";
    let (mut input, _) = field(text, 200.0);
    focus::clear();
    let y = input.editor.leaf.rect.get().height / 2.0;
    let n = text.find('ñ').unwrap();
    click(&mut input.editor, advance(&text[..n]) + 1.0, y);
    assert!(focus::is_focused(input.editor.id), "and takes the keyboard");
    assert_eq!(input.editor.caret.get(), n);
    let after = n + 'ñ'.len_utf8();
    wait_past_a_double_click();
    click(&mut input.editor, advance(&text[..after]) - 1.0, y);
    assert_eq!(input.editor.caret.get(), after);
    wait_past_a_double_click();
    click(&mut input.editor, 190.0, y);
    assert_eq!(
        input.editor.caret.get(),
        text.len(),
        "past the end is the end"
    );
}

/// **A drag selects what it runs across**, from where it was pressed, in either direction.
#[test]
fn a_drag_selects_from_the_press_to_the_pointer_either_way() {
    let text = "uno dos tres";
    let (mut input, _) = field(text, 200.0);
    let y = 12.0;
    press_at(&mut input.editor, advance("uno "), y);
    move_to(&mut input.editor, advance("uno dos t"), y);
    assert_eq!(selected(&input.editor).as_deref(), Some("dos t"));
    move_to(&mut input.editor, advance("u"), y);
    assert_eq!(selected(&input.editor).as_deref(), Some("no "));
    release(&mut input.editor);
    move_to(&mut input.editor, advance("uno dos tres"), y);
    assert_eq!(
        selected(&input.editor).as_deref(),
        Some("no "),
        "a move after the release is no drag"
    );
}

/// A drag past the end of a field scrolls the value under it to follow, the way the caret does.
#[test]
fn a_drag_past_the_edge_selects_to_the_end_and_scrolls_to_it() {
    let (mut input, _) = field(LONG, 120.0);
    input.on_event(&key(Key::Named(NamedKey::Home)));
    reactive_core::batch(|| {});
    press_at(&mut input.editor, 2.0, 12.0);
    move_to(&mut input.editor, 1000.0, 12.0);
    reactive_core::batch(|| {});
    let selection = selected(&input.editor).expect("a selection");
    assert!(
        LONG.ends_with(&selection) && selection.len() > LONG.len() / 2,
        "{selection:?}"
    );
    assert!(input.editor.shape.scroll_x.get() > 0.0);
}

/// **A double-click takes a word, a triple-click the line.** A drag after a double-click goes on by whole words.
#[test]
fn a_double_click_takes_a_word_and_a_triple_click_the_whole_line() {
    let text = "dijo «mañana», y se fue";
    let (mut input, _) = field(text, 300.0);
    let n = text.find('ñ').unwrap();
    let x = middle_of(text, n);
    click(&mut input.editor, x, 12.0);
    click(&mut input.editor, x, 12.0);
    assert_eq!(selected(&input.editor).as_deref(), Some("mañana"));
    click(&mut input.editor, x, 12.0);
    assert_eq!(
        selected(&input.editor).as_deref(),
        Some(text),
        "a field's line is all of it"
    );

    let (mut input, _) = field(text, 300.0);
    click(&mut input.editor, x, 12.0);
    press_at(&mut input.editor, x, 12.0);
    let fue = text.find("fue").unwrap();
    move_to(&mut input.editor, middle_of(text, fue + 1), 12.0);
    assert_eq!(
        selected(&input.editor).as_deref(),
        Some("mañana», y se fue"),
        "by whole words"
    );
    let dijo = text.find("ijo").unwrap();
    move_to(&mut input.editor, middle_of(text, dijo), 12.0);
    assert_eq!(
        selected(&input.editor).as_deref(),
        Some("dijo «mañana"),
        "keeping the word pressed on"
    );
}

/// **Shift with a press extends the selection** from where it started, rather than starting a new one.
#[test]
fn shift_with_a_press_extends_the_selection() {
    let text = "uno dos tres";
    let (mut input, _) = field(text, 200.0);
    click(&mut input.editor, advance("uno "), 12.0);
    wait_past_a_double_click();
    shift(true);
    click(&mut input.editor, advance("uno dos t"), 12.0);
    assert_eq!(selected(&input.editor).as_deref(), Some("dos t"));
    click(&mut input.editor, advance("u"), 12.0);
    shift(false);
    assert_eq!(
        selected(&input.editor).as_deref(),
        Some("no "),
        "from the same start, either way"
    );
}

/// A finger on the field puts the caret down and drags the page, not a selection.
#[test]
fn a_touch_puts_the_caret_down_without_dragging_a_selection() {
    let text = "uno dos tres";
    let (mut input, _) = field(text, 200.0);
    let at = input.editor.leaf.rect.get();
    input.on_event(&Event::PointerPressed {
        x: (at.x + advance("uno ")) as f64,
        y: (at.y + 12.0) as f64,
        button: PointerButton::Primary,
        source: PointerSource::Touch { id: 1 },
    });
    move_to(&mut input.editor, advance("uno dos"), 12.0);
    assert_eq!(input.editor.caret.get(), 4);
    assert_eq!(selected(&input.editor), None);
}

/// The same gestures in an editor of several lines: a press lands on the row it is on, a drag runs across rows, and a triple-click takes the line between its breaks.
#[test]
fn presses_and_drags_select_across_the_rows_of_a_text_area() {
    let text = "uno dos\ntres cuatro\ncinco";
    let (mut area, _) = area(text, 300.0);
    let row = |n: usize| (n as f32 + 0.5) * line_h();
    click(&mut area.editor, advance("tr"), row(1));
    assert_eq!(area.editor.caret.get(), text.find("tres").unwrap() + 2);
    wait_past_a_double_click();
    press_at(&mut area.editor, advance("uno "), row(0));
    move_to(&mut area.editor, advance("ci"), row(2));
    assert_eq!(
        selected(&area.editor).as_deref(),
        Some("dos\ntres cuatro\nci")
    );
    release(&mut area.editor);
    wait_past_a_double_click();
    let x = middle_of("tres cuatro", 6);
    for _ in 0..2 {
        click(&mut area.editor, x, row(1));
    }
    assert_eq!(selected(&area.editor).as_deref(), Some("cuatro"));
    click(&mut area.editor, x, row(1));
    assert_eq!(
        selected(&area.editor).as_deref(),
        Some("tres cuatro"),
        "a triple-click takes the line"
    );
    let bands: Vec<Rect> = rects(&area.view(), line_h())
        .into_iter()
        .filter(|band| band.width != CARET_WIDTH)
        .collect();
    assert_eq!(bands.len(), 1, "and paints it on its row: {bands:?}");
}

/// **A composition is marked where its letters are, after letters of more than one byte and across rows.** The caret and the line under it are placed by what is drawn before them on their own row: accented letters before the composition, a row of kana above it, and the space it follows.
#[test]
fn a_composition_after_accented_rows_is_marked_under_its_letters_on_its_own_row() {
    let text = "Mañana será\nにほんご\nnaïve ";
    let (mut area, value) = area(text, 300.0);
    let preedit = "かな";
    area.on_event(&Event::Ime(Ime::preedit(
        preedit,
        Some((preedit.len(), preedit.len())),
    )));
    assert_eq!(value.get(), text, "a composition is not typed yet");
    let view = area.view();
    let underline = rects(&view, 1.0);
    assert_eq!(underline.len(), 1, "{underline:?}");
    let (x0, x1) = (advance("naïve "), advance(&format!("naïve {preedit}")));
    assert!((underline[0].x - x0).abs() < 0.01, "{underline:?} for {x0}");
    assert!(
        (underline[0].x + underline[0].width - x1).abs() < 0.01,
        "{underline:?} for {x1}"
    );
    assert_eq!(
        underline[0].y + underline[0].height,
        3.0 * line_h(),
        "on the foot of the third row"
    );
    let caret = caret_of(&area.editor);
    assert!((caret.x - x1).abs() < 0.01, "{caret:?}");
    assert_eq!(caret.y, 2.0 * line_h());
    let reported = crate::text_input().expect("the area holds the keyboard");
    let at = area.editor.leaf.rect.get();
    assert_eq!(
        reported.caret,
        Rect::new(at.x + caret.x, at.y + caret.y, caret.width, caret.height)
    );
}

/// A composition that is converting part of itself, after accented letters and across a wrap, marks that part on each row it falls on.
#[test]
fn the_part_being_converted_is_marked_on_every_row_it_falls_on() {
    let (mut area, _) = area("áéíóú ", 300.0);
    let preedit = "にほんごのぶんしょう";
    let from = "にほん".len();
    area.on_event(&Event::Ime(Ime::preedit(
        preedit,
        Some((from, preedit.len())),
    )));
    let heavy = rects(&area.view(), 2.0);
    assert_eq!(heavy.len(), 1, "{heavy:?}");
    let x = advance(&format!("áéíóú {}", &preedit[..from]));
    assert!((heavy[0].x - x).abs() < 0.01, "{heavy:?} for {x}");
}

/// In a field: kana composed after accented letters and a space are marked from the end of the space.
#[test]
fn a_field_marks_a_composition_from_the_end_of_the_space_before_it() {
    let text = "Árbol ñandú ";
    let (mut input, _) = field(text, 300.0);
    let preedit = "にほん";
    input.on_event(&Event::Ime(Ime::preedit(
        preedit,
        Some((preedit.len(), preedit.len())),
    )));
    let underline = rects(&input.view(), 1.0);
    assert!(
        (underline[0].x - advance(text)).abs() < 0.01,
        "{underline:?}"
    );
    assert!(
        advance(text) > advance(text.trim_end()),
        "the space takes its room"
    );
    let caret = caret_of(&input.editor);
    assert!((caret.x - advance(&format!("{text}{preedit}"))).abs() < 0.01);
}

/// **The text is drawn on the rows the marks are placed on.** Its line height is said outright, so a renderer that would grow a row for a character from another face cannot move the rows under the caret.
#[test]
fn the_text_is_drawn_at_the_line_height_its_marks_are_placed_at() {
    let (area, _) = area("uno\nにほん\ntres", 300.0);
    let (_, drawn) = drawn_text(&area.view());
    let factor = drawn.line_height.factor().expect("said outright");
    assert!((factor * drawn.font_size - line_h()).abs() < 0.001);
    let (input, _) = field("にほん", 200.0);
    let (_, drawn) = drawn_text(&input.view());
    assert!(
        (drawn.line_height.factor().expect("said outright") * drawn.font_size - line_h()).abs()
            < 0.001
    );
}

/// A text area that does not wrap scrolls a long line under its box to the caret, as a field does.
#[test]
fn a_text_area_that_does_not_wrap_scrolls_a_long_line_to_the_caret() {
    let text = format!("short\n{LONG}\nshort");
    let (mut area, _) = area(&text, 120.0);
    area.editor.caret.set(text.find(LONG).unwrap() + LONG.len());
    reactive_core::batch(|| {});
    let scrolled = area.editor.shape.scroll_x.get();
    assert!(scrolled > 0.0);
    let caret = caret_of(&area.editor);
    assert!(
        caret.x >= 0.0 && caret.x + caret.width <= 120.0 + 0.01,
        "{caret:?}"
    );
    assert_eq!(caret.y, line_h());
    area.on_event(&key(Key::Named(NamedKey::ArrowDown)));
    area.on_event(&key(Key::Named(NamedKey::End)));
    reactive_core::batch(|| {});
    let caret = caret_of(&area.editor);
    assert!(
        caret.x >= 0.0 && caret.x + caret.width <= 120.0 + 0.01,
        "a shorter row brings its end into view: {caret:?}"
    );
    area.on_event(&key(Key::Named(NamedKey::Home)));
    reactive_core::batch(|| {});
    assert_eq!(area.editor.shape.scroll_x.get(), 0.0, "and its start");
}

/// **The view follows the caret down.** Rows typed past the bottom of the scroll an area sits in scroll the view with them.
#[test]
fn rows_typed_past_the_bottom_of_the_view_scroll_it_with_them() {
    reset_layout_runtime();
    focus::clear();
    let value = signal("one".to_string());
    let made: Rc<RefCell<Option<(FocusId, crate::ScrollViewport)>>> = Rc::default();
    let sink = Rc::clone(&made);
    let height = line_h() * 3.0;
    let mut scroll = crate::LayoutScrollArea::new_with(
        LayoutStyle::new().width(300.0).height(height),
        move |viewport| {
            let area = TextArea::new(value, LayoutStyle::new(), style)?;
            *sink.borrow_mut() = Some((area.focus_id(), viewport));
            Ok(Box::new(area) as Box<dyn LayoutItem>)
        },
    )
    .unwrap();
    compute_layout(
        scroll.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(height),
    )
    .unwrap();
    let (id, viewport) = made.borrow().clone().expect("the builder ran");
    focus::request(id);
    for rows in 2..=8 {
        scroll.on_event(&key(Key::Named(NamedKey::Enter)));
        relayout_if_dirty();
        reactive_core::batch(|| {});
        let expected = (rows as f32 * line_h() - height).max(0.0);
        assert!(
            (viewport.peek_offset().1 - expected).abs() < 0.5,
            "{rows} rows: the view is at {:?}, not {expected}",
            viewport.peek_offset()
        );
    }
    focus::clear();
}

/// Long enough after the last press that the next is a click of its own.
fn wait_past_a_double_click() {
    std::thread::sleep(DOUBLE_CLICK + std::time::Duration::from_millis(20));
}
