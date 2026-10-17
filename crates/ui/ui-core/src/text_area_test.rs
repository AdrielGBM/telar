use super::*;
use crate::context::{
    compute_layout, new_container, relayout_if_dirty, reset_layout_runtime, track_layout,
};
use crate::layout_item::LayoutItem;
use layout_core::AvailableSpace;
use renderer_core::Color;

fn key(k: Key) -> Event {
    Event::KeyPressed {
        key: k,
        modifiers: ModifiersState::default(),
        unmodified: None,
    }
}

/// `k` with the primary modifier held: Command on a Mac, Ctrl elsewhere.
fn chord(k: Key) -> Event {
    Event::KeyPressed {
        key: k,
        modifiers: platform_core::keyboard_convention().primary(),
        unmodified: None,
    }
}

fn shifted(k: Key) -> Event {
    Event::KeyPressed {
        key: k,
        modifiers: ModifiersState {
            is_shift: true,
            ..ModifiersState::default()
        },
        unmodified: None,
    }
}

#[test]
fn typing_over_a_selection_replaces_it() {
    let (mut area, value) = focused("one\ntwo");
    area.on_event(&chord(Key::Char('a')));
    area.on_event(&key(Key::Char('x')));
    assert_eq!(value.get(), "x");
}

/// The case the notebook is: a selection that runs across a line break, cut whole.
#[test]
fn a_selection_across_lines_cuts_whole() {
    let (mut area, value) = focused("one\ntwo\nthree");
    area.on_event(&chord(Key::Char('a')));
    area.on_event(&chord(Key::Char('x')));
    assert_eq!(value.get(), "");
}

#[test]
fn shift_arrows_grow_a_selection_and_backspace_takes_it() {
    let (mut area, value) = focused("hello");
    area.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
    area.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
    assert_eq!(area.selection("hello"), Some((3, 5)));
    area.on_event(&key(Key::Named(NamedKey::Backspace)));
    assert_eq!(value.get(), "hel");
}

/// Enter with a selection replaces it with the break, rather than pushing the selected text down a line.
#[test]
fn enter_replaces_a_selection() {
    let (mut area, value) = focused("abcd");
    area.on_event(&chord(Key::Char('a')));
    area.on_event(&key(Key::Named(NamedKey::Enter)));
    assert_eq!(value.get(), "\n");
}

#[test]
fn copy_leaves_the_text_and_the_selection_alone() {
    let (mut area, value) = focused("one\ntwo");
    area.on_event(&chord(Key::Char('a')));
    assert_eq!(area.on_event(&chord(Key::Char('c'))), EventResult::Handled);
    assert_eq!(value.get(), "one\ntwo");
    assert_eq!(area.selection("one\ntwo"), Some((0, 7)));
}
fn focused(initial: &str) -> (TextArea, RwSignal<String>) {
    reset_layout_runtime();
    let value = signal(initial.to_string());
    let area = TextArea::new(value, LayoutStyle::new().width(400.0), || {
        TextStyle::new(14.0, Color::BLACK)
    })
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[area.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    focus::request(area.id);
    (area, value)
}

/// The multi-line twin of `Input`'s guard test: this editor takes Enter and the vertical arrows as text, so a global shortcut on any of them must stand aside while the caret is here.
#[test]
fn the_shortcut_guard_covers_every_key_this_editor_edits() {
    let plain = ModifiersState::default();
    let named = [
        NamedKey::Space,
        NamedKey::Backspace,
        NamedKey::Delete,
        NamedKey::ArrowLeft,
        NamedKey::ArrowRight,
        NamedKey::ArrowUp,
        NamedKey::ArrowDown,
        NamedKey::Home,
        NamedKey::End,
        NamedKey::Enter,
        NamedKey::Escape,
        NamedKey::Tab,
        NamedKey::PageUp,
        NamedKey::F5,
    ];
    let keys: Vec<Key> = std::iter::once(Key::Char('x'))
        .chain(named.into_iter().map(Key::Named))
        .collect();
    let style = TextStyle::new(14.0, Color::BLACK);
    for k in keys {
        let (mut area, _value) = focused("one\ntwo");
        area.caret.set(5);
        // Asked first, as dispatch does — Escape answers by giving up the focus the guard reads.
        let guarded = focus::text_entry_takes_key(&k, plain);
        let edited = area.edit(&k, &plain, &style) == EventResult::Handled;
        assert!(
            !edited || guarded,
            "{k:?} is edited by the editor but the shortcut guard lets it through"
        );
        focus::clear();
    }
}

#[test]
fn enter_inserts_newline_and_typing_continues_on_new_line() {
    let (mut area, value) = focused("ab");
    area.on_event(&key(Key::Named(NamedKey::Enter)));
    area.on_event(&key(Key::Char('c')));
    assert_eq!(value.get(), "ab\nc");
    assert_eq!(row(&area), 1);
}

#[test]
fn backspace_at_line_start_joins_lines() {
    let (mut area, value) = focused("ab\ncd");
    area.on_event(&key(Key::Named(NamedKey::Home)));
    area.on_event(&key(Key::Named(NamedKey::Backspace)));
    assert_eq!(value.get(), "abcd");
}

#[test]
fn arrow_up_down_moves_between_lines() {
    let (mut area, _value) = focused("aaaa\nbb");
    area.on_event(&key(Key::Named(NamedKey::ArrowUp)));
    assert_eq!(row(&area), 0);
    area.on_event(&key(Key::Named(NamedKey::ArrowDown)));
    assert_eq!(row(&area), 1);
}

#[test]
fn click_focuses_and_positions_caret_without_reborrow() {
    use platform_core::{PointerButton, PointerSource};
    let (mut area, _value) = focused("hello\nworld");
    focus::clear();
    // The caret set must not run inside a `value.with` closure, which would re-borrow the reactive runtime.
    let r = area.leaf.rect.get();
    let handled = area.on_event(&Event::PointerPressed {
        x: (r.x + 5.0) as f64,
        y: (r.y + 2.0) as f64,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
    assert_eq!(handled, EventResult::Handled);
    assert!(
        focus::is_focused(area.id),
        "a press inside focuses the area"
    );
}

#[test]
fn ctrl_chord_is_ignored_as_shortcut() {
    let (mut area, value) = focused("hi");
    let save = Event::KeyPressed {
        key: Key::Char('s'),
        modifiers: ModifiersState {
            is_ctrl: true,
            ..Default::default()
        },
        unmodified: None,
    };
    assert_eq!(area.on_event(&save), EventResult::Ignored);
    assert_eq!(value.get(), "hi");
}

/// The area and a column holding it, laid out whenever its height is asked for.
struct Laid {
    area: TextArea,
    root: layout_core::NodeId,
}

impl Laid {
    fn new(area: TextArea) -> Self {
        let root = new_container(LayoutStyle::new().flex_column(), &[area.layout_node()]).unwrap();
        Self { area, root }
    }

    fn height(&self) -> f32 {
        relayout_if_dirty();
        compute_layout(
            self.root,
            AvailableSpace::MaxContent,
            AvailableSpace::MaxContent,
        )
        .unwrap();
        track_layout(self.area.layout_node()).unwrap().get().height
    }
}

/// A style that follows state moves the line height, so it measures the area again the way a new line does; a size or family read from a signal used to leave the box at the old height.
#[test]
fn a_restyled_area_is_measured_again() {
    reset_layout_runtime();
    let size = signal(12.0f32);
    let laid = Laid::new(
        TextArea::new(
            signal("one\ntwo".to_string()),
            LayoutStyle::new(),
            move || TextStyle::new(size.get(), Color::BLACK),
        )
        .unwrap(),
    );
    let small = laid.height();
    size.set(48.0);
    let large = laid.height();
    assert!(large > small * 3.5, "small={small} large={large}");
}

#[test]
fn a_new_line_measures_the_area_again() {
    reset_layout_runtime();
    let value = signal("one".to_string());
    let laid = Laid::new(
        TextArea::new(value, LayoutStyle::new(), || {
            TextStyle::new(14.0, Color::BLACK)
        })
        .unwrap(),
    );
    let one = laid.height();
    value.set("one\ntwo\nthree".to_string());
    let three = laid.height();
    assert!(three > one * 2.5, "one={one} three={three}");
}

/// What `.rsx` text does: it takes the size declared above it, and follows it when it moves.
#[test]
fn a_declaring_area_is_sized_by_the_tree_above_it() {
    use renderer_core::Declared;
    reset_layout_runtime();
    let laid = Laid::new(
        TextArea::declaring(
            signal("one\ntwo".to_string()),
            LayoutStyle::new(),
            |inherited| inherited,
        )
        .unwrap(),
    );
    crate::inherit::declare(laid.root, Declared::default().with_font_size(20.0));
    assert_eq!((laid.area.style)().font_size, 20.0);
    let small = laid.height();
    crate::inherit::declare(laid.root, Declared::default().with_font_size(40.0));
    let large = laid.height();
    assert!(large > small * 1.8, "small={small} large={large}");
}

#[test]
fn a_declaring_area_follows_the_surfaces_family_and_a_complete_one_does_not() {
    use renderer_core::FontFamily;
    let surface = crate::Surface::new();
    let _entered = surface.enter();
    reset_layout_runtime();
    let declaring = TextArea::declaring(signal(String::new()), LayoutStyle::new(), |t| t).unwrap();
    let complete = TextArea::new(signal(String::new()), LayoutStyle::new(), || {
        TextStyle::new(14.0, Color::BLACK)
    })
    .unwrap();
    crate::set_font_family(Some(FontFamily::Monospace));
    assert_eq!((declaring.style)().font_family, FontFamily::Monospace);
    assert_eq!((complete.style)().font_family, FontFamily::SansSerif);
    crate::set_font_family(None);
    assert_eq!((declaring.style)().font_family, FontFamily::SansSerif);
}

/// Like a field, an area shows what was typed: its caret and selection index the value, so a case inherited from above would put them between the wrong letters.
#[test]
fn an_area_shows_what_was_typed_whatever_case_it_inherits() {
    reset_layout_runtime();
    let laid = Laid::new(
        TextArea::declaring(signal("typed".to_string()), LayoutStyle::new(), |t| t).unwrap(),
    );
    crate::inherit::declare(
        laid.root,
        renderer_core::Declared::default().with_text_case(renderer_core::TextCase::Upper),
    );
    assert_eq!(
        (laid.area.style)().text_case,
        renderer_core::TextCase::AsWritten
    );
}

/// An area taller than the scroll it sits in keeps the line being typed on in view: a reveal of the whole area could not, and the caret would walk off the bottom of the view.
#[test]
fn the_caret_stays_in_view_of_the_scroll_the_area_sits_in() {
    reset_layout_runtime();
    let style = || TextStyle::new(14.0, Color::BLACK);
    let line_h = crate::text_metrics::line_box(&style());
    let value = signal(
        (1..=20)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let made: Rc<RefCell<Option<(FocusId, crate::ScrollViewport)>>> = Rc::default();
    let sink = Rc::clone(&made);
    let mut scroll = crate::LayoutScrollArea::new_with(
        LayoutStyle::new().width(300.0).height(line_h * 3.0),
        move |viewport| {
            let area = TextArea::new(value, LayoutStyle::new().width(300.0), style)?;
            *sink.borrow_mut() = Some((area.focus_id(), viewport));
            Ok(Box::new(area) as Box<dyn LayoutItem>)
        },
    )
    .unwrap();
    compute_layout(
        scroll.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(line_h * 3.0),
    )
    .unwrap();
    let (id, viewport) = made.borrow().clone().expect("the builder ran");
    assert_eq!(viewport.peek_offset().1, 0.0);

    focus::request(id);
    reactive_core::batch(|| {});
    let bottom = line_h * 20.0 - line_h * 3.0;
    assert!(
        (viewport.peek_offset().1 - bottom).abs() < 0.5,
        "the caret starts on the last line, so the view is scrolled to it: {:?}",
        viewport.peek_offset()
    );

    for _ in 0..19 {
        scroll.on_event(&key(Key::Named(NamedKey::ArrowUp)));
    }
    relayout_if_dirty();
    reactive_core::batch(|| {});
    assert_eq!(
        viewport.peek_offset().1,
        0.0,
        "walking the caret up to the first line brings the view up with it"
    );
    focus::clear();
}

fn row(area: &TextArea) -> usize {
    let text = area.value.get();
    let style = (area.style)();
    area.placed(&text, &style)
        .row_of(area.caret.get(), area.affinity.get())
}

const PARAGRAPH: &str =
    "the quick brown fox jumps over the lazy dog and then keeps on running far away";

fn line_h() -> f32 {
    crate::text_metrics::line_box(&TextStyle::new(14.0, Color::BLACK))
}

/// An area that wraps at `width`, laid out in a column that wide and focused.
struct Wrapping {
    area: TextArea,
    value: RwSignal<String>,
    root: layout_core::NodeId,
}

impl Wrapping {
    fn new(initial: &str, width: f32) -> Self {
        Self::with(
            TextArea::new(signal(initial.to_string()), LayoutStyle::new(), || {
                TextStyle::new(14.0, Color::BLACK)
            }),
            width,
        )
    }

    fn with(area: Result<TextArea, LayoutError>, width: f32) -> Self {
        let area = area.unwrap().text_wrap(TextWrap::Wrap);
        let value = area.value;
        let root = new_container(column(width), &[area.layout_node()]).unwrap();
        let wrapping = Self { area, value, root };
        wrapping.lay_out(width);
        focus::request(wrapping.area.id);
        wrapping
    }

    fn lay_out(&self, width: f32) {
        crate::context::set_layout_style(self.root, column(width)).unwrap();
        relayout_if_dirty();
        compute_layout(
            self.root,
            AvailableSpace::Definite(width),
            AvailableSpace::Definite(800.0),
        )
        .unwrap();
    }

    fn lines(&self) -> Rc<VisualLines> {
        let text = self.value.get();
        let style = (self.area.style)();
        self.area.placed(&text, &style).lines
    }

    fn height(&self) -> f32 {
        relayout_if_dirty();
        self.area.leaf.rect.get().height
    }

    fn press(&mut self, k: Key) {
        self.area.on_event(&key(k));
    }

    fn click(&mut self, x: f32, y: f32) {
        let r = self.area.leaf.rect.get();
        self.area.on_event(&Event::PointerPressed {
            x: (r.x + x) as f64,
            y: (r.y + y) as f64,
            button: PointerButton::Primary,
            source: platform_core::PointerSource::Mouse,
        });
    }
}

fn column(width: f32) -> LayoutStyle {
    LayoutStyle::new().flex_column().width(width).height(800.0)
}

/// Every rect and text the area draws, in its own coordinates.
fn drawn(area: &TextArea) -> (Vec<Rect>, Vec<Rect>) {
    fn walk(node: &RenderNode, rects: &mut Vec<Rect>, texts: &mut Vec<Rect>) {
        match node {
            RenderNode::Primitive(renderer_core::DrawCommand::Rect { rect, .. }) => {
                rects.push(*rect)
            }
            RenderNode::Primitive(renderer_core::DrawCommand::Text { rect, .. }) => {
                texts.push(*rect)
            }
            RenderNode::Group { children }
            | RenderNode::Element { children, .. }
            | RenderNode::Transform { children, .. }
            | RenderNode::Clip { children, .. }
            | RenderNode::Layer { children, .. } => {
                for child in children.iter() {
                    walk(child, rects, texts);
                }
            }
            _ => {}
        }
    }
    let (mut rects, mut texts) = (Vec::new(), Vec::new());
    walk(&area.view(), &mut rects, &mut texts);
    (rects, texts)
}

fn caret_rect(area: &TextArea) -> Rect {
    drawn(area)
        .0
        .into_iter()
        .find(|rect| rect.width == CARET_WIDTH)
        .expect("a focused area draws its caret")
}

#[test]
fn an_area_that_does_not_wrap_keeps_a_long_line_on_one_row() {
    let (area, value) = focused(PARAGRAPH);
    let text = value.get();
    let style = (area.style)();
    assert_eq!(area.placed(&text, &style).lines.len(), 1);
    assert!((area.leaf.rect.get().height - line_h()).abs() < 0.5);
    assert_eq!(drawn(&area).1[0].width, NO_WRAP_WIDTH);
}

#[test]
fn a_wrapping_area_is_as_tall_as_the_rows_it_wraps_into_and_draws_them_at_its_width() {
    let wrapping = Wrapping::new(PARAGRAPH, 160.0);
    let rows = wrapping.lines().len();
    assert!(rows >= 3, "{rows} rows");
    assert!(
        (wrapping.height() - rows as f32 * line_h()).abs() < 0.5,
        "{} for {rows} rows",
        wrapping.height()
    );
    assert_eq!(drawn(&wrapping.area).1[0].width, 160.0);
}

#[test]
fn typing_past_the_edge_grows_the_area_a_row_and_taking_it_back_shrinks_it() {
    let mut wrapping = Wrapping::new("abc", 160.0);
    let one = wrapping.height();
    assert!((one - line_h()).abs() < 0.5);
    let mut typed = 0;
    while wrapping.lines().len() < 2 {
        wrapping.press(Key::Named(NamedKey::Space));
        for c in "abc".chars() {
            wrapping.press(Key::Char(c));
        }
        typed += 4;
        assert!(typed < 200, "nothing wrapped");
    }
    assert!((wrapping.height() - 2.0 * line_h()).abs() < 0.5);
    for _ in 0..4 {
        wrapping.press(Key::Named(NamedKey::Backspace));
    }
    assert_eq!(wrapping.lines().len(), 1);
    assert!((wrapping.height() - line_h()).abs() < 0.5);
}

#[test]
fn a_narrower_area_wraps_into_more_rows_and_is_measured_again_when_typed_into() {
    let mut wrapping = Wrapping::new(PARAGRAPH, 400.0);
    let wide = wrapping.lines().len();
    wrapping.lay_out(160.0);
    let narrow = wrapping.lines().len();
    assert!(narrow > wide, "{wide} rows at 400, {narrow} at 160");
    assert!((wrapping.height() - narrow as f32 * line_h()).abs() < 0.5);
    wrapping.press(Key::Char('!'));
    let rows = wrapping.lines().len();
    assert!((wrapping.height() - rows as f32 * line_h()).abs() < 0.5);
}

#[test]
fn a_placeholder_wraps_and_the_area_is_as_tall_as_it_is_drawn() {
    let area = TextArea::new(signal(String::new()), LayoutStyle::new(), || {
        TextStyle::new(14.0, Color::BLACK)
    })
    .map(|area| area.placeholder(PARAGRAPH));
    let wrapping = Wrapping::with(area, 160.0);
    let style = (wrapping.area.style)();
    let rows = VisualLines::of(PARAGRAPH, &style, Some(160.0)).len();
    assert!(rows >= 3);
    assert!((wrapping.height() - rows as f32 * line_h()).abs() < 0.5);
}

#[test]
fn down_and_up_keep_the_caret_s_x_across_wrapped_rows() {
    let mut wrapping = Wrapping::new(PARAGRAPH, 160.0);
    wrapping.area.caret.set(6);
    let x = caret_rect(&wrapping.area).x;
    wrapping.press(Key::Named(NamedKey::ArrowDown));
    assert_eq!(row(&wrapping.area), 1);
    let below = caret_rect(&wrapping.area);
    assert_eq!(below.y, line_h());
    assert!((below.x - x).abs() < 8.0, "{x} above, {} below", below.x);
    wrapping.press(Key::Named(NamedKey::ArrowDown));
    assert_eq!(row(&wrapping.area), 2);
    wrapping.press(Key::Named(NamedKey::ArrowUp));
    wrapping.press(Key::Named(NamedKey::ArrowUp));
    assert_eq!(wrapping.area.caret.get(), 6, "back where it started");
}

/// The column a run of vertical moves began in is the one it goes on in: a short line on the way takes the caret to its end and does not keep it there.
#[test]
fn a_short_row_on_the_way_does_not_pull_the_caret_in_for_good() {
    let (mut area, _value) = focused("abcdefgh\nab\nabcdefgh");
    area.caret.set(6);
    area.on_event(&key(Key::Named(NamedKey::ArrowDown)));
    assert_eq!(area.caret.get(), 11);
    area.on_event(&key(Key::Named(NamedKey::ArrowDown)));
    assert_eq!(area.caret.get(), 18);
    area.on_event(&key(Key::Named(NamedKey::ArrowLeft)));
    area.on_event(&key(Key::Named(NamedKey::ArrowUp)));
    assert_eq!(
        area.caret.get(),
        11,
        "a horizontal move starts a new column"
    );
}

#[test]
fn home_and_end_go_to_the_ends_of_the_row_on_screen() {
    let mut wrapping = Wrapping::new(PARAGRAPH, 160.0);
    let lines = wrapping.lines();
    wrapping.area.caret.set(2);
    wrapping.press(Key::Named(NamedKey::End));
    assert_eq!(wrapping.area.caret.get(), lines.row(0).end);
    let end = caret_rect(&wrapping.area);
    assert_eq!(
        end.y, 0.0,
        "the end of a wrapped row is drawn on it, not on the next"
    );
    assert!(end.x > 100.0, "{}", end.x);
    wrapping.press(Key::Named(NamedKey::Home));
    assert_eq!(wrapping.area.caret.get(), 0);
    wrapping.press(Key::Named(NamedKey::ArrowDown));
    wrapping.press(Key::Named(NamedKey::Home));
    assert_eq!(wrapping.area.caret.get(), lines.row(1).start);
    assert_eq!(caret_rect(&wrapping.area).y, line_h());
    wrapping.press(Key::Named(NamedKey::End));
    assert_eq!(wrapping.area.caret.get(), lines.row(1).end);
    assert_eq!(caret_rect(&wrapping.area).y, line_h());
}

#[test]
fn typing_at_the_end_of_a_wrapped_row_moves_the_caret_on_with_the_letter() {
    let mut wrapping = Wrapping::new(PARAGRAPH, 160.0);
    let end = wrapping.lines().row(0).end;
    wrapping.press(Key::Named(NamedKey::Home));
    wrapping.area.caret.set(0);
    wrapping.press(Key::Named(NamedKey::End));
    wrapping.press(Key::Char('x'));
    assert_eq!(&wrapping.value.get()[end..=end], "x");
    assert_eq!(wrapping.area.caret.get(), end + 1);
    assert_eq!(wrapping.area.affinity.get(), Affinity::Downstream);
}

#[test]
fn the_primary_modifier_with_home_and_end_goes_to_the_ends_of_the_text() {
    use platform_core::{KeyboardConvention, set_keyboard_convention};
    set_keyboard_convention(KeyboardConvention::Pc);
    let (mut area, value) = focused("one\ntwo\nthree");
    area.caret.set(5);
    let ctrl = ModifiersState {
        is_ctrl: true,
        ..ModifiersState::default()
    };
    let style = (area.style)();
    area.edit(&Key::Named(NamedKey::Home), &ctrl, &style);
    assert_eq!(area.caret.get(), 0);
    area.edit(&Key::Named(NamedKey::End), &ctrl, &style);
    assert_eq!(area.caret.get(), value.get().len());
    set_keyboard_convention(KeyboardConvention::native());
}

#[test]
fn on_a_mac_command_with_the_arrows_goes_to_the_ends_of_the_row_and_of_the_text() {
    use platform_core::{KeyboardConvention, set_keyboard_convention};
    set_keyboard_convention(KeyboardConvention::Mac);
    let mut wrapping = Wrapping::new(PARAGRAPH, 160.0);
    let lines = wrapping.lines();
    let command = ModifiersState {
        is_meta: true,
        ..ModifiersState::default()
    };
    let style = (wrapping.area.style)();
    let start = lines.row(1).start;
    wrapping.area.caret.set(start + 2);
    let mut press = |k: NamedKey| {
        wrapping.area.edit(&Key::Named(k), &command, &style);
        wrapping.area.caret.get()
    };
    assert_eq!(press(NamedKey::ArrowRight), lines.row(1).end);
    assert_eq!(press(NamedKey::ArrowLeft), start);
    assert_eq!(press(NamedKey::ArrowDown), PARAGRAPH.len());
    assert_eq!(press(NamedKey::ArrowUp), 0);
    set_keyboard_convention(KeyboardConvention::native());
}

#[test]
fn a_press_past_the_end_of_a_wrapped_row_lands_at_its_end_and_one_at_a_row_s_start_on_it() {
    let mut wrapping = Wrapping::new(PARAGRAPH, 160.0);
    let lines = wrapping.lines();
    wrapping.click(159.0, line_h() * 0.5);
    assert_eq!(wrapping.area.caret.get(), lines.row(0).end);
    assert_eq!(caret_rect(&wrapping.area).y, 0.0);
    wrapping.click(0.5, line_h() * 1.5);
    assert_eq!(wrapping.area.caret.get(), lines.row(1).start);
    assert_eq!(caret_rect(&wrapping.area).y, line_h());
}

#[test]
fn a_selection_across_wrapped_rows_paints_one_band_on_each() {
    let mut wrapping = Wrapping::new(PARAGRAPH, 160.0);
    let rows = wrapping.lines().len();
    wrapping.area.on_event(&chord(Key::Char('a')));
    let bands: Vec<Rect> = drawn(&wrapping.area)
        .0
        .into_iter()
        .filter(|rect| rect.width != CARET_WIDTH)
        .collect();
    assert_eq!(bands.len(), rows);
    for (row, band) in bands.iter().enumerate() {
        assert_eq!(band.y, row as f32 * line_h());
        assert!(band.x + band.width <= 160.0, "{band:?} stays in the box");
    }
}

#[test]
fn the_caret_on_a_wrapped_row_stays_in_view_of_the_scroll_the_area_sits_in() {
    reset_layout_runtime();
    let style = || TextStyle::new(14.0, Color::BLACK);
    let value = signal(PARAGRAPH.repeat(3));
    let made: Rc<RefCell<Option<(FocusId, crate::ScrollViewport)>>> = Rc::default();
    let sink = Rc::clone(&made);
    let height = line_h() * 2.0;
    let mut scroll = crate::LayoutScrollArea::new_with(
        LayoutStyle::new().width(160.0).height(height),
        move |viewport| {
            let area = TextArea::new(value, LayoutStyle::new(), style)?.text_wrap(TextWrap::Wrap);
            *sink.borrow_mut() = Some((area.focus_id(), viewport));
            Ok(Box::new(area) as Box<dyn LayoutItem>)
        },
    )
    .unwrap();
    compute_layout(
        scroll.layout_node(),
        AvailableSpace::Definite(160.0),
        AvailableSpace::Definite(height),
    )
    .unwrap();
    let (id, viewport) = made.borrow().clone().expect("the builder ran");
    focus::request(id);
    scroll.on_event(&chord(Key::Named(NamedKey::End)));
    relayout_if_dirty();
    reactive_core::batch(|| {});
    let offset = viewport.peek_offset().1;
    assert!(
        offset > line_h() * 4.0,
        "the view follows the caret down the rows: {offset}"
    );
    focus::clear();
}

fn ime(event: Ime) -> Event {
    Event::Ime(event)
}

fn composition_lines(area: &TextArea) -> Vec<Rect> {
    drawn(area)
        .0
        .into_iter()
        .filter(|rect| rect.height == 1.0)
        .collect()
}

#[test]
fn a_commit_types_its_text_once_lines_and_all() {
    let (mut area, value) = focused("ab");
    area.caret.set(1);
    area.on_event(&ime(Ime::preedit("か", None)));
    assert_eq!(value.get(), "ab", "a composition is not typed yet");
    area.on_event(&ime(Ime::Commit("x\r\ny".into())));
    assert_eq!(value.get(), "ax\nyb");
    assert_eq!(area.caret.get(), 4);
    assert!(composition_lines(&area).is_empty());
}

#[test]
fn a_composition_replaces_a_selection_across_lines() {
    let (mut area, value) = focused("one\ntwo");
    area.on_event(&chord(Key::Char('a')));
    area.on_event(&ime(Ime::preedit("か", Some((3, 3)))));
    assert_eq!(value.get(), "");
    area.on_event(&ime(Ime::Commit("可".into())));
    assert_eq!(value.get(), "可");
}

#[test]
fn keys_that_reach_a_composing_area_edit_nothing() {
    let (mut area, value) = focused("ab");
    area.on_event(&ime(Ime::preedit("か", None)));
    area.on_event(&key(Key::Named(NamedKey::Enter)));
    assert_eq!(value.get(), "ab");
}

/// **A composition wraps with the text.** One long enough to run past the edge is drawn on the next row, underlined on both, and the area grows to hold it as typing would.
#[test]
fn a_composition_that_runs_past_the_edge_wraps_and_grows_the_area() {
    let mut wrapping = Wrapping::new("abc", 160.0);
    let one = wrapping.height();
    let long = "あいうえおかきくけこさしすせそたちつてと";
    wrapping
        .area
        .on_event(&ime(Ime::preedit(long, Some((long.len(), long.len())))));
    assert_eq!(wrapping.value.get(), "abc");
    assert_eq!(
        composition_lines(&wrapping.area).len(),
        2,
        "one line per row"
    );
    assert!(
        (wrapping.height() - 2.0 * one).abs() < 0.5,
        "{} for two rows",
        wrapping.height()
    );
    let caret = caret_rect(&wrapping.area);
    assert!(
        caret.y >= line_h(),
        "the caret is on the second row: {caret:?}"
    );
    let reported = crate::text_input().expect("the area holds the keyboard");
    let at = wrapping.area.leaf.rect.get();
    assert_eq!(
        reported.caret,
        Rect::new(at.x + caret.x, at.y + caret.y, caret.width, caret.height)
    );

    wrapping.area.on_event(&ime(Ime::preedit("", None)));
    assert!((wrapping.height() - one).abs() < 0.5, "and shrinks back");
}
