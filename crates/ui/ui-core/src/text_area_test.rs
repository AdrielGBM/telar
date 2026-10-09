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

fn chord(k: Key) -> Event {
    Event::KeyPressed {
        key: k,
        modifiers: ModifiersState {
            is_ctrl: true,
            ..ModifiersState::default()
        },
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
    assert_eq!(line_index(&value.get(), area.caret.get()), 1);
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
    let (mut area, value) = focused("aaaa\nbb");
    area.on_event(&key(Key::Named(NamedKey::ArrowUp)));
    assert_eq!(line_index(&value.get(), area.caret.get()), 0);
    area.on_event(&key(Key::Named(NamedKey::ArrowDown)));
    assert_eq!(line_index(&value.get(), area.caret.get()), 1);
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
