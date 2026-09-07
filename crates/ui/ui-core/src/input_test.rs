use crate::context::reset_layout_runtime;
use layout_core::AvailableSpace;
use platform_core::PointerSource;
use renderer_core::{Color, DrawCommand, LineHeight, Paint, TextAlign};
use std::time::Duration;

use super::*;
use crate::context::{compute_layout, new_container};
use crate::layout_item::LayoutItem;

fn key(k: Key) -> Event {
    Event::KeyPressed {
        key: k,
        modifiers: ModifiersState::default(),
    }
}

fn chord(k: Key) -> Event {
    Event::KeyPressed {
        key: k,
        modifiers: ModifiersState {
            is_ctrl: true,
            ..ModifiersState::default()
        },
    }
}

fn shifted(k: Key) -> Event {
    Event::KeyPressed {
        key: k,
        modifiers: ModifiersState {
            is_shift: true,
            ..ModifiersState::default()
        },
    }
}

#[test]
fn shift_arrows_grow_a_selection_and_a_plain_one_drops_it() {
    let (mut input, _) = focused_input("hello");
    input.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
    input.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
    assert_eq!(input.selection("hello"), Some((3, 5)), "two chars selected");
    input.on_event(&key(Key::Named(NamedKey::ArrowRight)));
    assert_eq!(input.selection("hello"), None, "a plain arrow drops it");
}

/// The behaviour that makes a selection worth having: what you type lands *instead of* it.
#[test]
fn typing_over_a_selection_replaces_it() {
    let (mut input, value) = focused_input("hello");
    input.on_event(&chord(Key::Char('a')));
    input.on_event(&key(Key::Char('x')));
    assert_eq!(value.get(), "x");
}

#[test]
fn backspace_takes_the_selection_rather_than_one_character() {
    let (mut input, value) = focused_input("hello");
    input.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
    input.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
    input.on_event(&key(Key::Named(NamedKey::Backspace)));
    assert_eq!(value.get(), "hel");
}

/// An unshifted arrow with a selection collapses to its edge — pressing Left with three characters selected puts the caret before them, not one step in from wherever the caret happened to be.
#[test]
fn a_plain_arrow_collapses_to_the_selection_edge() {
    let (mut input, _) = focused_input("hello");
    input.on_event(&chord(Key::Char('a')));
    input.on_event(&key(Key::Named(NamedKey::ArrowLeft)));
    assert_eq!(input.caret.get(), 0, "collapsed to the low edge");
}

#[test]
fn cut_removes_the_selection_and_copy_leaves_it() {
    let (mut input, value) = focused_input("hello");
    input.on_event(&chord(Key::Char('a')));
    input.on_event(&chord(Key::Char('c')));
    assert_eq!(value.get(), "hello", "copy leaves the text alone");
    assert_eq!(input.selection("hello"), Some((0, 5)), "and the selection");
    input.on_event(&chord(Key::Char('x')));
    assert_eq!(value.get(), "", "cut takes it");
}

/// Copy and cut with nothing selected report `Ignored`, so a global shortcut table still sees the chord instead of it being swallowed by a field that did nothing with it.
#[test]
fn copy_without_a_selection_is_not_consumed() {
    let (mut input, _) = focused_input("hello");
    assert_eq!(input.on_event(&chord(Key::Char('c'))), EventResult::Ignored);
}
fn focused_input(initial: &str) -> (Input, RwSignal<String>) {
    reset_layout_runtime();
    let value = signal(initial.to_string());
    let input = Input::new(value, LayoutStyle::new().width(200.0).height(20.0), || {
        TextStyle::new(14.0, Color::BLACK)
    })
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        &[input.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    focus::request(input.id);
    (input, value)
}

/// A field styled to the given alignment, focused, with its rect laid out.
fn aligned_input(initial: &str, align: TextAlign) -> Input {
    reset_layout_runtime();
    let value = signal(initial.to_string());
    let input = Input::new(
        value,
        LayoutStyle::new().width(200.0).height(20.0),
        move || {
            let mut style = TextStyle::new(14.0, Color::BLACK);
            style.text_align = align;
            style
        },
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        &[input.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    focus::request(input.id);
    input
}

/// Where the caret was drawn, and how lit it was.
fn caret_of(input: &Input) -> (Rect, f32) {
    fn walk(node: &RenderNode, found: &mut Option<(Rect, f32)>) {
        match node {
            RenderNode::Primitive(DrawCommand::Rect { rect, style })
                if rect.width == CARET_WIDTH =>
            {
                let lit = match style.fill {
                    Some(Paint::Solid(color)) => color.a,
                    _ => 0.0,
                };
                *found = Some((*rect, lit));
            }
            RenderNode::Group { children }
            | RenderNode::Transform { children, .. }
            | RenderNode::Clip { children, .. }
            | RenderNode::Layer { children, .. } => {
                for child in children.iter() {
                    walk(child, found);
                }
            }
            _ => {}
        }
    }
    let view = input.view();
    let mut found = None;
    walk(&view, &mut found);
    found.expect("a focused field draws its caret")
}

/// **The caret goes where the letters went.** A field inherits its alignment from the region around it — a centred column of chrome hands one down — and the text is placed by the shaper while the caret is placed here, so the two have to answer the same question. They did not: the letters sat in the middle of the box and the caret against its left edge, a whole field's width away from the text it was in.
#[test]
fn the_caret_follows_the_alignment_the_text_was_drawn_with() {
    let left = caret_of(&aligned_input("hola", TextAlign::Start)).0.x;
    let middle = caret_of(&aligned_input("hola", TextAlign::Center)).0.x;
    let right = caret_of(&aligned_input("hola", TextAlign::End)).0.x;

    assert!(
        middle > left && right > middle,
        "el cursor no sigue la alineación: {left} / {middle} / {right}"
    );
    assert!(
        (right - 200.0).abs() < 1.0,
        "alineado a la derecha el cursor va al borde, no a {right}"
    );
}

/// **A caret that does not blink is a caret nobody finds** — a hairline of ink that may be sitting in an empty field, and the eye goes to what changes.
#[test]
fn the_caret_blinks_while_the_field_holds_the_keyboard() {
    let input = aligned_input("hola", TextAlign::Start);
    motion_core::reset();
    input.blink.follow(true);

    let start = web_time::Instant::now();
    motion_core::tick(start);
    let lit = caret_of(&input).1;
    motion_core::tick(start + Duration::from_millis(700));
    let out = caret_of(&input).1;

    assert!(lit > 0.9, "el cursor empieza encendido, no en {lit}");
    assert!(out < 0.1, "y se apaga a la mitad del ciclo, no en {out}");
}

/// And it is lit again by the key, so it is never missing at the moment somebody is looking for it.
#[test]
fn typing_lights_the_caret_again() {
    let (mut input, _) = focused_input("hola");
    motion_core::reset();
    input.blink.follow(true);
    let start = web_time::Instant::now();
    motion_core::tick(start);
    motion_core::tick(start + Duration::from_millis(700));
    assert!(caret_of(&input).1 < 0.1, "el cursor debería estar apagado");

    input.on_event(&key(Key::Char('x')));

    assert!(
        caret_of(&input).1 > 0.9,
        "la tecla no volvió a encender el cursor"
    );
}

/// **The caret stands in the line, not over it.** The height came from the face's natural leading while the text was laid out at whatever the tree declared, so under a `line_height: 1.0` — a pixel face kept on its own grid — the caret hung below the descenders of the very word it was standing in.
#[test]
fn the_caret_is_as_tall_as_the_line_the_text_is_laid_out_on() {
    reset_layout_runtime();
    let value = signal("hola".to_string());
    let input = Input::new(value, LayoutStyle::new().width(200.0).height(20.0), || {
        let mut style = TextStyle::new(14.0, Color::BLACK);
        style.line_height = LineHeight::Times(1.0);
        style
    })
    .unwrap();
    focus::request(input.id);

    let tall = caret_of(&input).0.height;

    assert!(
        (tall - 14.0).abs() < 0.01,
        "una línea de 1.0 sobre 14 mide 14, no {tall}"
    );
    assert!(
        crate::text_metrics::line_height(14.0) > 14.5,
        "la natural es más alta, que es lo que hacía el cursor demasiado largo"
    );
}

/// Escape is a key a focused field eats, so an application watching from the outside cannot tell it from a click elsewhere — and where losing focus commits, those are opposite answers.
#[test]
fn escape_says_so_before_it_hands_the_keyboard_back() {
    let (mut input, _) = focused_input("hola");
    let given_up = Rc::new(std::cell::Cell::new(false));
    let said = Rc::clone(&given_up);
    input = input.on_cancel(move || said.set(true));

    input.on_event(&key(Key::Named(NamedKey::Escape)));

    assert!(given_up.get(), "el campo se rindió sin decirlo");
    assert!(!focus::is_focused(input.id), "y soltó el teclado");
}

/// The other half of the blink, and the one that decides what the window costs: a sequence registered with the ticker is a standing request to redraw, so a field nobody is typing into must not be running one.
#[test]
fn a_field_without_the_keyboard_asks_for_no_frames() {
    motion_core::reset();
    let input = aligned_input("hola", TextAlign::Start);
    input.blink.follow(false);
    motion_core::tick(web_time::Instant::now());
    assert!(
        !motion_core::has_active(),
        "un campo sin el teclado sigue pidiendo cuadros"
    );
}

/// The guard a global shortcut handler consults ([`focus::text_entry_takes_key`]) is a second list of what this editor eats, kept apart from `edit` because it has to answer without running the edit. A key added here and not there re-opens the bug it exists for: typing that also fires the app's shortcuts.
#[test]
fn the_shortcut_guard_covers_every_key_this_field_edits() {
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
        NamedKey::PageDown,
        NamedKey::F5,
        NamedKey::Insert,
    ];
    let keys: Vec<Key> = std::iter::once(Key::Char('3'))
        .chain(std::iter::once(Key::Char('s')))
        .chain(named.into_iter().map(Key::Named))
        .collect();
    for k in keys {
        let (mut input, _value) = focused_input("hello");
        input.caret.set(2);
        // Asked first, as dispatch does: a global handler decides before the field acts, and Escape proves it — the field answers by giving up the focus the guard reads.
        let guarded = focus::text_entry_takes_key(&k, plain);
        let edited = input.edit(&k, &plain) == EventResult::Handled;
        assert!(
            !edited || guarded,
            "{k:?} is edited by the field but the shortcut guard lets it through"
        );
        focus::clear();
    }
}

#[test]
fn autofocus_makes_a_field_typable_without_a_tap() {
    reset_layout_runtime();
    focus::clear();
    let value = signal(String::new());
    let mut input = Input::new(value, LayoutStyle::new().width(200.0).height(20.0), || {
        TextStyle::new(14.0, Color::BLACK)
    })
    .unwrap()
    .autofocus();
    assert!(
        focus::is_focused(input.id),
        "the field holds focus from construction"
    );
    input.on_event(&key(Key::Char('x')));
    assert_eq!(value.get(), "x", "and the very first keystroke is text");

    reset_layout_runtime();
    focus::clear();
    let untouched = signal(String::new());
    let mut plain = Input::new(
        untouched,
        LayoutStyle::new().width(200.0).height(20.0),
        || TextStyle::new(14.0, Color::BLACK),
    )
    .unwrap();
    assert!(
        !focus::is_focused(plain.id),
        "a field without autofocus is not focused for it"
    );
    plain.on_event(&key(Key::Char('x')));
    assert_eq!(untouched.get(), "");
}

/// A capital is typed with `Shift` down, as a keyboard sends it — and `Shift` is the selection modifier, so the anchor it left behind made the next keystroke replace the letter just typed. A name came out with every capital missing but the last, which no test that presses `M` where a hand presses `Shift`+`M` can see.
#[test]
fn a_capital_is_not_a_selection() {
    let (mut input, value) = focused_input("");
    for (c, held) in [
        ('L', true),
        ('a', false),
        (' ', false),
        ('C', true),
        ('u', false),
        ('e', false),
        ('v', false),
        ('a', false),
    ] {
        let event = match (c, held) {
            (' ', _) => key(Key::Named(NamedKey::Space)),
            (c, true) => shifted(Key::Char(c)),
            (c, false) => key(Key::Char(c)),
        };
        input.on_event(&event);
    }
    assert_eq!(value.get(), "La Cueva");
}

/// `Shift` still selects when the key it is held with is a movement.
#[test]
fn a_shifted_arrow_still_selects() {
    let (mut input, value) = focused_input("hola");
    input.on_event(&key(Key::Named(NamedKey::End)));
    for _ in 0..2 {
        input.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
    }
    input.on_event(&key(Key::Char('y')));
    assert_eq!(value.get(), "hoy");
}

#[test]
fn typing_inserts_at_caret() {
    let (mut input, value) = focused_input("");
    for c in "hi".chars() {
        input.on_event(&key(Key::Char(c)));
    }
    assert_eq!(value.get(), "hi");
    assert_eq!(input.caret.get(), 2);
}

#[test]
fn backspace_and_arrows_edit_mid_string() {
    let (mut input, value) = focused_input("abc");
    input.on_event(&key(Key::Named(NamedKey::ArrowLeft)));
    input.on_event(&key(Key::Named(NamedKey::ArrowLeft)));
    assert_eq!(input.caret.get(), 1);
    input.on_event(&key(Key::Named(NamedKey::Backspace)));
    assert_eq!(value.get(), "bc");
    assert_eq!(input.caret.get(), 0);
    input.on_event(&key(Key::Char('X')));
    assert_eq!(value.get(), "Xbc");
}

#[test]
fn keys_ignored_when_not_focused() {
    let (mut input, value) = focused_input("a");
    focus::clear();
    let r = input.on_event(&key(Key::Char('z')));
    assert_eq!(r, EventResult::Ignored);
    assert_eq!(value.get(), "a", "an unfocused input must not edit");
}

#[test]
fn a_masked_field_hides_the_text_without_changing_it() {
    let (mut input, value) = focused_input("");
    input = input.secret();
    for c in ['h', 'u', 'n', 't', 'e', 'r'] {
        input.on_event(&key(Key::Char(c)));
    }
    assert_eq!(
        value.get(),
        "hunter",
        "the bound signal keeps the real text, which is what a submit handler reads"
    );
    assert_eq!(
        input.shown(&value.get()),
        "••••••",
        "and the screen does not"
    );

    // One per character, not per byte: a multi-byte password must not leak its length, and the caret is measured against this string.
    assert_eq!(input.shown("mañana"), "••••••");
    assert_eq!(input.shown(""), "");
}

#[test]
fn an_unmasked_field_is_unchanged() {
    let (input, value) = focused_input("plain");
    assert_eq!(input.shown(&value.get()), "plain");
}

#[test]
fn tap_focuses_and_ctrl_chord_is_ignored() {
    let (mut input, value) = focused_input("hi");
    focus::clear();
    let r = input.on_event(&Event::PointerPressed {
        x: 10.0,
        y: 5.0,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
    assert_eq!(r, EventResult::Handled);
    assert!(
        focus::is_focused(input.id),
        "a tap inside focuses the input"
    );
    let paste = Event::KeyPressed {
        key: Key::Char('v'),
        modifiers: ModifiersState {
            is_ctrl: true,
            ..Default::default()
        },
    };
    assert_eq!(input.on_event(&paste), EventResult::Ignored);
    assert_eq!(value.get(), "hi");
}
